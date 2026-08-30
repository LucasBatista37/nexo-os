//! `vfs` — sistema de arquivos virtual com **namespace por instância** (cada cliente recebe o
//! seu `vfs` com os pontos de montagem que lhe couberem). Serve o protocolo `nexo.fs` v0 e
//! roteia por prefixo: `/disk` → NexoFS (`fs`), `/boot` → ESP (`espfs`, só leitura),
//! `/tmp` → ramfs interno (gravável, volátil).
//!
//! Handles: 0 = canal do `fs` (se montado), 1 = canal do `espfs` (se montado), 2 = cliente.
//! Argumento: máscara de montagens — bit 0 `/disk`, bit 1 `/boot`, bit 2 `/tmp`.
//! Inodes devolvidos ao cliente carregam a montagem nos bits 28..30:
//! 1 = disk (ino do NexoFS nos bits baixos), 2 = tmp, 3 = boot (índice em tabela de caminhos).
#![no_std]
#![no_main]

use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const FS: Handle = 0;
const ESP: Handle = 1;
const CLIENT: Handle = 2;

const MOUNT_DISK: u64 = 1;
const MOUNT_BOOT: u64 = 2;
const MOUNT_TMP: u64 = 4;

const TAG_SHIFT: u32 = 28;
const TAG_DISK: u32 = 1;
const TAG_TMP: u32 = 2;
const TAG_BOOT: u32 = 3;

// Codigos de erro do nexo.fs (nexofs::FsError::code()) reutilizados.
const E_IO: u8 = 1;
const E_NOT_FOUND: u8 = 3;
const E_EXISTS: u8 = 4;
const E_NO_SPACE: u8 = 8;
const E_TOO_BIG: u8 = 9;
const E_INVALID_NAME: u8 = 10;
const E_INVALID: u8 = 11;
/// Montagem somente leitura.
const E_READ_ONLY: u8 = 13;

// ---- ramfs (/tmp): fixo, volátil ----
const RAM_FILES: usize = 16;
const RAM_FILE_MAX: usize = 16 * 1024;
const NAME_MAX: usize = 55;

struct RamFile {
    used: bool,
    name: [u8; NAME_MAX],
    name_len: u8,
    size: u32,
}

struct Ramfs {
    files: [RamFile; RAM_FILES],
    data: [[u8; RAM_FILE_MAX]; RAM_FILES],
}

static mut RAMFS: Ramfs = Ramfs {
    files: [const {
        RamFile {
            used: false,
            name: [0; NAME_MAX],
            name_len: 0,
            size: 0,
        }
    }; RAM_FILES],
    data: [[0; RAM_FILE_MAX]; RAM_FILES],
};

/// Acesso único ao ramfs (o serviço tem uma só thread).
fn ramfs() -> &'static mut Ramfs {
    // SAFETY: processo com uma única thread; nenhuma reentrância.
    unsafe { &mut *core::ptr::addr_of_mut!(RAMFS) }
}

fn ram_find(name: &[u8]) -> Option<usize> {
    let r = ramfs();
    (0..RAM_FILES)
        .find(|&i| r.files[i].used && r.files[i].name[..r.files[i].name_len as usize] == *name)
}

// ---- tabela de caminhos do /boot (inos sintéticos) ----
const BOOT_PATHS: usize = 16;
const BOOT_PATH_MAX: usize = 128;
static mut BOOT_TABLE: [([u8; BOOT_PATH_MAX], u8); BOOT_PATHS] =
    [([0; BOOT_PATH_MAX], 0); BOOT_PATHS];

fn boot_table() -> &'static mut [([u8; BOOT_PATH_MAX], u8); BOOT_PATHS] {
    // SAFETY: processo com uma única thread.
    unsafe { &mut *core::ptr::addr_of_mut!(BOOT_TABLE) }
}

fn boot_remember(path: &[u8]) -> Option<u32> {
    if path.len() > BOOT_PATH_MAX {
        return None;
    }
    let t = boot_table();
    for (i, (p, l)) in t.iter().enumerate() {
        if *l as usize == path.len() && p[..path.len()] == *path {
            return Some(i as u32);
        }
    }
    for (i, entry) in t.iter_mut().enumerate() {
        if entry.1 == 0 {
            entry.0[..path.len()].copy_from_slice(path);
            entry.1 = path.len() as u8;
            return Some(i as u32);
        }
    }
    None
}

fn fail(code: i64, what: &str) -> ! {
    log!("vfs: falha: {}", what);
    nexo_sys::exit(code)
}

fn reply_into(status: u8, value: u64, data: &[u8], out: &mut [u8; 4096]) -> usize {
    out[0] = status;
    out[1..4].fill(0);
    out[4..12].copy_from_slice(&value.to_le_bytes());
    let n = data.len().min(4096 - 12);
    out[12..12 + n].copy_from_slice(&data[..n]);
    12 + n
}

/// Encaminha um pedido `nexo.fs` cru para `ch` e devolve (status, valor, dados em `buf[12..]`).
fn forward(ch: Handle, req: &[u8], buf: &mut [u8; 4096]) -> (u8, u64, usize) {
    if nexo_sys::channel_send(ch, req, &[]) != Status::Ok {
        return (E_IO, 0, 0);
    }
    let mut hs = [0u32; 1];
    match nexo_sys::channel_recv(ch, buf, &mut hs) {
        Ok((n, _)) if n >= 12 => (
            buf[0],
            u64::from_le_bytes(buf[4..12].try_into().unwrap()),
            n - 12,
        ),
        _ => (E_IO, 0, 0),
    }
}

/// Pedido `nexo.esp` (list/stat/read por caminho).
fn esp_call(op: u8, offset: u64, len: u32, path: &[u8], buf: &mut [u8; 4096]) -> (u8, u64, usize) {
    let mut req = [0u8; 4096];
    req[0] = op;
    req[4..12].copy_from_slice(&offset.to_le_bytes());
    req[12..16].copy_from_slice(&len.to_le_bytes());
    let n = 16 + path.len().min(4096 - 16);
    req[16..n].copy_from_slice(&path[..n - 16]);
    if nexo_sys::channel_send(ESP, &req[..n], &[]) != Status::Ok {
        return (E_IO, 0, 0);
    }
    let mut hs = [0u32; 1];
    match nexo_sys::channel_recv(ESP, buf, &mut hs) {
        Ok((n, _)) if n >= 12 => (
            buf[0],
            u64::from_le_bytes(buf[4..12].try_into().unwrap()),
            n - 12,
        ),
        _ => (E_IO, 0, 0),
    }
}

fn split_mount(path: &[u8]) -> (&[u8], &[u8]) {
    let p = path.strip_prefix(b"/").unwrap_or(path);
    match p.iter().position(|&c| c == b'/') {
        Some(i) => (&p[..i], &p[i + 1..]),
        None => (p, b""),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(mounts: u64) -> ! {
    let mounts = if mounts == 0 {
        MOUNT_DISK | MOUNT_BOOT | MOUNT_TMP
    } else {
        mounts
    };
    log!(
        "vfs: namespace com{}{}{}",
        if mounts & MOUNT_DISK != 0 {
            " /disk"
        } else {
            ""
        },
        if mounts & MOUNT_BOOT != 0 {
            " /boot"
        } else {
            ""
        },
        if mounts & MOUNT_TMP != 0 { " /tmp" } else { "" }
    );
    let mut req = [0u8; 4096];
    let mut out = [0u8; 4096];
    let mut fwd = [0u8; 4096];
    let mut hs = [0u32; 1];
    loop {
        let (n, _) = match nexo_sys::channel_recv(CLIENT, &mut req, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => {
                log!("vfs: cliente desconectou");
                nexo_sys::exit(0)
            }
            Err(_) => fail(50, "recv"),
        };
        if n < 20 {
            let len = reply_into(0xff, 0, &[], &mut out);
            let _ = nexo_sys::channel_send(CLIENT, &out[..len], &[]);
            continue;
        }
        let op = req[0];
        let ino = u32::from_le_bytes(req[4..8].try_into().unwrap());
        let offset = u64::from_le_bytes(req[8..16].try_into().unwrap());
        let len32 = u32::from_le_bytes(req[16..20].try_into().unwrap());
        let (status, value, dlen): (u8, u64, usize) = match op {
            // Operações por caminho.
            0 | 1 | 2 | 3 | 6 => {
                let path = &req[20..n];
                let (mount, rest) = split_mount(path);
                if mount.is_empty() {
                    // raiz do namespace
                    if op == 6 {
                        let mut pos = 0usize;
                        let mut count = 0u64;
                        for (bit, name) in [
                            (MOUNT_BOOT, &b"boot"[..]),
                            (MOUNT_DISK, &b"disk"[..]),
                            (MOUNT_TMP, &b"tmp"[..]),
                        ] {
                            if mounts & bit != 0 {
                                fwd[12 + pos..12 + pos + 4].copy_from_slice(&0u32.to_le_bytes());
                                fwd[12 + pos + 4] = 2; // diretório
                                fwd[12 + pos + 5] = name.len() as u8;
                                fwd[12 + pos + 6..12 + pos + 6 + name.len()].copy_from_slice(name);
                                pos += 6 + name.len();
                                count += 1;
                            }
                        }
                        (0, count, pos)
                    } else if op == 0 {
                        fwd[12] = 2;
                        fwd[13..21].copy_from_slice(&0u64.to_le_bytes());
                        (0, 0, 9)
                    } else {
                        (E_INVALID, 0, 0)
                    }
                } else if mount == b"disk" && mounts & MOUNT_DISK != 0 {
                    // reescreve o caminho e encaminha
                    let mut r = [0u8; 4096];
                    let hn = 20 + rest.len();
                    r[..20].copy_from_slice(&req[..20]);
                    r[20..hn].copy_from_slice(rest);
                    let (st, v, dl) = forward(FS, &r[..hn], &mut fwd);
                    let v = if st == 0 && op <= 2 {
                        (v as u32 | (TAG_DISK << TAG_SHIFT)) as u64
                    } else {
                        v
                    };
                    // desloca os dados para fwd[12..]
                    (st, v, dl)
                } else if mount == b"boot" && mounts & MOUNT_BOOT != 0 {
                    match op {
                        0 => {
                            let (st, _, dl) = esp_call(1, 0, 0, rest, &mut fwd);
                            if st != 0 {
                                (if st == 3 { E_NOT_FOUND } else { E_IO }, 0, 0)
                            } else if dl >= 5 {
                                let attr = fwd[12];
                                let size = u32::from_le_bytes(fwd[13..17].try_into().unwrap());
                                match boot_remember(rest) {
                                    Some(idx) => {
                                        let kind = if attr & 0x10 != 0 { 2u8 } else { 1u8 };
                                        fwd[12] = kind;
                                        fwd[13..21].copy_from_slice(&(size as u64).to_le_bytes());
                                        ((0), (idx | (TAG_BOOT << TAG_SHIFT)) as u64, 9)
                                    }
                                    None => (E_NO_SPACE, 0, 0),
                                }
                            } else {
                                (E_IO, 0, 0)
                            }
                        }
                        6 => {
                            let (st, v, dl) = esp_call(0, 0, 0, rest, &mut fwd);
                            if st != 0 {
                                (if st == 3 { E_NOT_FOUND } else { E_IO }, 0, 0)
                            } else {
                                // converte [attr u8][size u32] -> [ino u32][kind u8]
                                let mut src = 0usize;
                                let mut dst_buf = [0u8; 4096];
                                let mut dst = 0usize;
                                while src + 6 <= dl {
                                    let attr = fwd[12 + src];
                                    let nl = fwd[12 + src + 5] as usize;
                                    dst_buf[dst..dst + 4].copy_from_slice(&0u32.to_le_bytes());
                                    dst_buf[dst + 4] = if attr & 0x10 != 0 { 2 } else { 1 };
                                    dst_buf[dst + 5] = nl as u8;
                                    dst_buf[dst + 6..dst + 6 + nl]
                                        .copy_from_slice(&fwd[12 + src + 6..12 + src + 6 + nl]);
                                    src += 6 + nl;
                                    dst += 6 + nl;
                                }
                                fwd[12..12 + dst].copy_from_slice(&dst_buf[..dst]);
                                (0, v, dst)
                            }
                        }
                        _ => (E_READ_ONLY, 0, 0),
                    }
                } else if mount == b"tmp" && mounts & MOUNT_TMP != 0 {
                    let name = rest;
                    match op {
                        0 => match ram_find(name) {
                            Some(i) => {
                                fwd[12] = 1;
                                fwd[13..21]
                                    .copy_from_slice(&(ramfs().files[i].size as u64).to_le_bytes());
                                (0, (i as u32 | (TAG_TMP << TAG_SHIFT)) as u64, 9)
                            }
                            None if name.is_empty() => {
                                fwd[12] = 2;
                                fwd[13..21].copy_from_slice(&0u64.to_le_bytes());
                                (0, 0, 9)
                            }
                            None => (E_NOT_FOUND, 0, 0),
                        },
                        1 => {
                            if name.is_empty() || name.len() > NAME_MAX || name.contains(&b'/') {
                                (E_INVALID_NAME, 0, 0)
                            } else if ram_find(name).is_some() {
                                (E_EXISTS, 0, 0)
                            } else {
                                let r = ramfs();
                                match (0..RAM_FILES).find(|&i| !r.files[i].used) {
                                    Some(i) => {
                                        r.files[i].used = true;
                                        r.files[i].name[..name.len()].copy_from_slice(name);
                                        r.files[i].name_len = name.len() as u8;
                                        r.files[i].size = 0;
                                        (0, (i as u32 | (TAG_TMP << TAG_SHIFT)) as u64, 0)
                                    }
                                    None => (E_NO_SPACE, 0, 0),
                                }
                            }
                        }
                        2 => (E_INVALID, 0, 0), // sem subdiretórios no ramfs v0
                        3 => match ram_find(name) {
                            Some(i) => {
                                ramfs().files[i].used = false;
                                (0, 0, 0)
                            }
                            None => (E_NOT_FOUND, 0, 0),
                        },
                        6 => {
                            if !name.is_empty() {
                                (E_NOT_FOUND, 0, 0)
                            } else {
                                let r = ramfs();
                                let mut pos = 0usize;
                                let mut count = 0u64;
                                for i in 0..RAM_FILES {
                                    if !r.files[i].used {
                                        continue;
                                    }
                                    let nl = r.files[i].name_len as usize;
                                    fwd[12 + pos..12 + pos + 4].copy_from_slice(
                                        &(i as u32 | (TAG_TMP << TAG_SHIFT)).to_le_bytes(),
                                    );
                                    fwd[12 + pos + 4] = 1;
                                    fwd[12 + pos + 5] = nl as u8;
                                    fwd[12 + pos + 6..12 + pos + 6 + nl]
                                        .copy_from_slice(&r.files[i].name[..nl]);
                                    pos += 6 + nl;
                                    count += 1;
                                }
                                (0, count, pos)
                            }
                        }
                        _ => (E_INVALID, 0, 0),
                    }
                } else {
                    (E_NOT_FOUND, 0, 0)
                }
            }
            // Operações por inode.
            4 | 5 | 9 => {
                let tag = ino >> TAG_SHIFT;
                let raw = ino & ((1 << TAG_SHIFT) - 1);
                match tag {
                    TAG_DISK if mounts & MOUNT_DISK != 0 => {
                        let mut r = [0u8; 4096];
                        r[..n].copy_from_slice(&req[..n]);
                        r[4..8].copy_from_slice(&raw.to_le_bytes());
                        forward(FS, &r[..n], &mut fwd)
                    }
                    TAG_TMP if mounts & MOUNT_TMP != 0 => {
                        let r = ramfs();
                        let i = raw as usize;
                        if i >= RAM_FILES || !r.files[i].used {
                            (E_INVALID, 0, 0)
                        } else {
                            match op {
                                4 => {
                                    let size = r.files[i].size as u64;
                                    if offset >= size {
                                        (0, 0, 0)
                                    } else {
                                        let want = (len32 as usize)
                                            .min(4096 - 12)
                                            .min((size - offset) as usize);
                                        fwd[12..12 + want].copy_from_slice(
                                            &r.data[i][offset as usize..offset as usize + want],
                                        );
                                        (0, want as u64, want)
                                    }
                                }
                                5 => {
                                    let data = &req[20..n];
                                    let end = offset as usize + data.len();
                                    if end > RAM_FILE_MAX {
                                        (E_TOO_BIG, 0, 0)
                                    } else {
                                        r.data[i][offset as usize..end].copy_from_slice(data);
                                        r.files[i].size = r.files[i].size.max(end as u32);
                                        (0, data.len() as u64, 0)
                                    }
                                }
                                _ => {
                                    if offset < r.files[i].size as u64 {
                                        r.files[i].size = offset as u32;
                                    }
                                    (0, 0, 0)
                                }
                            }
                        }
                    }
                    TAG_BOOT if mounts & MOUNT_BOOT != 0 => {
                        if op != 4 {
                            (E_READ_ONLY, 0, 0)
                        } else {
                            let t = boot_table();
                            let i = raw as usize;
                            if i >= BOOT_PATHS || t[i].1 == 0 {
                                (E_INVALID, 0, 0)
                            } else {
                                let mut path = [0u8; BOOT_PATH_MAX];
                                let l = t[i].1 as usize;
                                path[..l].copy_from_slice(&t[i].0[..l]);
                                esp_call(2, offset, len32, &path[..l], &mut fwd)
                            }
                        }
                    }
                    _ => (E_INVALID, 0, 0),
                }
            }
            7 => {
                if mounts & MOUNT_DISK != 0 {
                    let mut r = [0u8; 20];
                    r[0] = 7;
                    forward(FS, &r, &mut fwd)
                } else {
                    (0, 0, 0)
                }
            }
            8 => {
                if mounts & MOUNT_DISK != 0 {
                    let mut r = [0u8; 20];
                    r[0] = 8;
                    forward(FS, &r, &mut fwd)
                } else {
                    (E_INVALID, 0, 0)
                }
            }
            _ => (E_INVALID, 0, 0),
        };
        // dados de resposta ficam em fwd[12..12+dlen]
        out[0] = status;
        out[1..4].fill(0);
        out[4..12].copy_from_slice(&value.to_le_bytes());
        let dlen = dlen.min(4096 - 12);
        out[12..12 + dlen].copy_from_slice(&fwd[12..12 + dlen]);
        if nexo_sys::channel_send(CLIENT, &out[..12 + dlen], &[]) != Status::Ok {
            fail(51, "send");
        }
    }
}

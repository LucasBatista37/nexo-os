//! `vfs` — sistema de arquivos virtual com **namespace por instância** (cada cliente recebe o
//! seu `vfs` com os pontos de montagem que lhe couberem). Serve o protocolo **tipado**
//! `nexo.fs` v1.0 e roteia por prefixo: `/disk` → NexoFS (`fs`, também tipado),
//! `/boot` → ESP (`espfs`, `nexo.esp`, só leitura), `/tmp` → ramfs interno (gravável, volátil).
//!
//! Handles: 0 = canal do `fs` (se montado), 1 = canal do `espfs` (se montado), 2 = cliente.
//! Argumento: máscara de montagens — bit 0 `/disk`, bit 1 `/boot`, bit 2 `/tmp` (0 = todas).
//! Inodes devolvidos ao cliente carregam a montagem nos bits 28..30:
//! 1 = disk (ino do NexoFS nos bits baixos), 2 = tmp, 3 = boot (índice em tabela de caminhos).
//! Erros remotos: 1..=11 = `nexofs::FsError::code()`, 13 = montagem somente leitura.
#![no_std]
#![no_main]

use nexo_proto::ProtoError;
use nexo_proto::esp as pesp;
use nexo_proto::fs as pfs;
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

fn path_of(p: &[u8]) -> ([u8; 256], u32) {
    let mut buf = [0u8; 256];
    let n = p.len().min(256);
    buf[..n].copy_from_slice(&p[..n]);
    (buf, n as u32)
}

/// Envia `msg[..m]` para `ch` e recebe a resposta em `msg`; devolve o tamanho (0 = falha).
fn roundtrip(ch: Handle, msg: &mut [u8; 4096], m: usize) -> usize {
    if nexo_sys::channel_send(ch, &msg[..m], &[]) != Status::Ok {
        return 0;
    }
    let mut hs = [0u32; 1];
    match nexo_sys::channel_recv(ch, msg, &mut hs) {
        Ok((n, _)) => n,
        Err(_) => 0,
    }
}

fn remote_code(e: ProtoError) -> u8 {
    match e {
        ProtoError::Remote(c) => c as u8,
        _ => E_IO,
    }
}

fn split_mount(path: &[u8]) -> (&[u8], &[u8]) {
    let p = path.strip_prefix(b"/").unwrap_or(path);
    match p.iter().position(|&c| c == b'/') {
        Some(i) => (&p[..i], &p[i + 1..]),
        None => (p, b""),
    }
}

/// Resultado interno de uma operação: resposta pronta em `out` (tamanho) ou erro (código).
type Op = Result<usize, u8>;

struct Vfs {
    mounts: u64,
    msg: [u8; 4096],
}

impl Vfs {
    fn stat(&mut self, path: &[u8], out: &mut [u8; 4096]) -> Op {
        let (mount, rest) = split_mount(path);
        if mount.is_empty() {
            return pfs::StatResponse {
                ino: 0,
                kind: 2,
                size: 0,
            }
            .encode_msg(out)
            .map_err(|_| E_IO);
        }
        if mount == b"disk" && self.mounts & MOUNT_DISK != 0 {
            let (path, path_len) = path_of(rest);
            let m = pfs::StatRequest { path, path_len }
                .encode_msg(&mut self.msg)
                .map_err(|_| E_IO)?;
            let n = roundtrip(FS, &mut self.msg, m);
            match pfs::decode_stat_response(&self.msg[..n]) {
                Ok(mut r) => {
                    r.ino |= TAG_DISK << TAG_SHIFT;
                    r.encode_msg(out).map_err(|_| E_IO)
                }
                Err(e) => Err(remote_code(e)),
            }
        } else if mount == b"boot" && self.mounts & MOUNT_BOOT != 0 {
            let (path, path_len) = path_of(rest);
            let m = pesp::StatRequest { path, path_len }
                .encode_msg(&mut self.msg)
                .map_err(|_| E_IO)?;
            let n = roundtrip(ESP, &mut self.msg, m);
            match pesp::decode_stat_response(&self.msg[..n]) {
                Ok(r) => {
                    let idx = boot_remember(rest).ok_or(E_NO_SPACE)?;
                    pfs::StatResponse {
                        ino: idx | (TAG_BOOT << TAG_SHIFT),
                        kind: if r.attr & 0x10 != 0 { 2 } else { 1 },
                        size: r.size as u64,
                    }
                    .encode_msg(out)
                    .map_err(|_| E_IO)
                }
                Err(e) => Err(if remote_code(e) == 3 {
                    E_NOT_FOUND
                } else {
                    E_IO
                }),
            }
        } else if mount == b"tmp" && self.mounts & MOUNT_TMP != 0 {
            if rest.is_empty() {
                return pfs::StatResponse {
                    ino: 0,
                    kind: 2,
                    size: 0,
                }
                .encode_msg(out)
                .map_err(|_| E_IO);
            }
            let i = ram_find(rest).ok_or(E_NOT_FOUND)?;
            pfs::StatResponse {
                ino: i as u32 | (TAG_TMP << TAG_SHIFT),
                kind: 1,
                size: ramfs().files[i].size as u64,
            }
            .encode_msg(out)
            .map_err(|_| E_IO)
        } else {
            Err(E_NOT_FOUND)
        }
    }

    fn create(&mut self, path: &[u8], dir: bool, out: &mut [u8; 4096]) -> Op {
        let (mount, rest) = split_mount(path);
        if mount == b"disk" && self.mounts & MOUNT_DISK != 0 {
            let (path, path_len) = path_of(rest);
            let m = if dir {
                pfs::MkdirRequest { path, path_len }.encode_msg(&mut self.msg)
            } else {
                pfs::CreateRequest { path, path_len }.encode_msg(&mut self.msg)
            }
            .map_err(|_| E_IO)?;
            let n = roundtrip(FS, &mut self.msg, m);
            let ino = if dir {
                pfs::decode_mkdir_response(&self.msg[..n]).map(|r| r.ino)
            } else {
                pfs::decode_create_response(&self.msg[..n]).map(|r| r.ino)
            }
            .map_err(remote_code)?;
            let tagged = ino | (TAG_DISK << TAG_SHIFT);
            if dir {
                pfs::MkdirResponse { ino: tagged }
                    .encode_msg(out)
                    .map_err(|_| E_IO)
            } else {
                pfs::CreateResponse { ino: tagged }
                    .encode_msg(out)
                    .map_err(|_| E_IO)
            }
        } else if mount == b"boot" && self.mounts & MOUNT_BOOT != 0 {
            Err(E_READ_ONLY)
        } else if mount == b"tmp" && self.mounts & MOUNT_TMP != 0 {
            if dir {
                return Err(E_INVALID); // sem subdiretórios no ramfs v0
            }
            let name = rest;
            if name.is_empty() || name.len() > NAME_MAX || name.contains(&b'/') {
                return Err(E_INVALID_NAME);
            }
            if ram_find(name).is_some() {
                return Err(E_EXISTS);
            }
            let r = ramfs();
            let i = (0..RAM_FILES)
                .find(|&i| !r.files[i].used)
                .ok_or(E_NO_SPACE)?;
            r.files[i].used = true;
            r.files[i].name[..name.len()].copy_from_slice(name);
            r.files[i].name_len = name.len() as u8;
            r.files[i].size = 0;
            pfs::CreateResponse {
                ino: i as u32 | (TAG_TMP << TAG_SHIFT),
            }
            .encode_msg(out)
            .map_err(|_| E_IO)
        } else {
            Err(E_NOT_FOUND)
        }
    }

    fn unlink(&mut self, path: &[u8], out: &mut [u8; 4096]) -> Op {
        let (mount, rest) = split_mount(path);
        if mount == b"disk" && self.mounts & MOUNT_DISK != 0 {
            let (path, path_len) = path_of(rest);
            let m = pfs::UnlinkRequest { path, path_len }
                .encode_msg(&mut self.msg)
                .map_err(|_| E_IO)?;
            let n = roundtrip(FS, &mut self.msg, m);
            pfs::decode_unlink_response(&self.msg[..n]).map_err(remote_code)?;
            pfs::UnlinkResponse {}.encode_msg(out).map_err(|_| E_IO)
        } else if mount == b"boot" && self.mounts & MOUNT_BOOT != 0 {
            Err(E_READ_ONLY)
        } else if mount == b"tmp" && self.mounts & MOUNT_TMP != 0 {
            let i = ram_find(rest).ok_or(E_NOT_FOUND)?;
            ramfs().files[i].used = false;
            pfs::UnlinkResponse {}.encode_msg(out).map_err(|_| E_IO)
        } else {
            Err(E_NOT_FOUND)
        }
    }

    fn read(&mut self, ino: u32, offset: u64, len: u32, out: &mut [u8; 4096]) -> Op {
        let (tag, raw) = (ino >> TAG_SHIFT, ino & ((1 << TAG_SHIFT) - 1));
        match tag {
            TAG_DISK if self.mounts & MOUNT_DISK != 0 => {
                let m = pfs::ReadRequest {
                    ino: raw,
                    offset,
                    len,
                }
                .encode_msg(&mut self.msg)
                .map_err(|_| E_IO)?;
                let n = roundtrip(FS, &mut self.msg, m);
                let r = pfs::decode_read_response(&self.msg[..n]).map_err(remote_code)?;
                r.encode_msg(out).map_err(|_| E_IO)
            }
            TAG_TMP if self.mounts & MOUNT_TMP != 0 => {
                let r = ramfs();
                let i = raw as usize;
                if i >= RAM_FILES || !r.files[i].used {
                    return Err(E_INVALID);
                }
                let size = r.files[i].size as u64;
                let mut resp = pfs::ReadResponse {
                    data: [0; 4000],
                    data_len: 0,
                };
                if offset < size {
                    let want = (len as usize).min(4000).min((size - offset) as usize);
                    resp.data[..want]
                        .copy_from_slice(&r.data[i][offset as usize..offset as usize + want]);
                    resp.data_len = want as u32;
                }
                resp.encode_msg(out).map_err(|_| E_IO)
            }
            TAG_BOOT if self.mounts & MOUNT_BOOT != 0 => {
                let t = boot_table();
                let i = raw as usize;
                if i >= BOOT_PATHS || t[i].1 == 0 {
                    return Err(E_INVALID);
                }
                let l = t[i].1 as usize;
                let mut pb = [0u8; BOOT_PATH_MAX];
                pb[..l].copy_from_slice(&t[i].0[..l]);
                let (path, path_len) = path_of(&pb[..l]);
                let m = pesp::ReadRequest {
                    path,
                    path_len,
                    offset,
                    len: len.min(3500),
                }
                .encode_msg(&mut self.msg)
                .map_err(|_| E_IO)?;
                let n = roundtrip(ESP, &mut self.msg, m);
                let r = pesp::decode_read_response(&self.msg[..n]).map_err(remote_code)?;
                let mut resp = pfs::ReadResponse {
                    data: [0; 4000],
                    data_len: r.data().len() as u32,
                };
                resp.data[..r.data().len()].copy_from_slice(r.data());
                resp.encode_msg(out).map_err(|_| E_IO)
            }
            _ => Err(E_INVALID),
        }
    }

    fn write(&mut self, ino: u32, offset: u64, data: &[u8], out: &mut [u8; 4096]) -> Op {
        let (tag, raw) = (ino >> TAG_SHIFT, ino & ((1 << TAG_SHIFT) - 1));
        match tag {
            TAG_DISK if self.mounts & MOUNT_DISK != 0 => {
                let dn = data.len().min(3900);
                let mut rq = pfs::WriteRequest {
                    ino: raw,
                    offset,
                    data: [0; 3900],
                    data_len: dn as u32,
                };
                rq.data[..dn].copy_from_slice(&data[..dn]);
                let m = rq.encode_msg(&mut self.msg).map_err(|_| E_IO)?;
                let n = roundtrip(FS, &mut self.msg, m);
                let r = pfs::decode_write_response(&self.msg[..n]).map_err(remote_code)?;
                r.encode_msg(out).map_err(|_| E_IO)
            }
            TAG_TMP if self.mounts & MOUNT_TMP != 0 => {
                let r = ramfs();
                let i = raw as usize;
                if i >= RAM_FILES || !r.files[i].used {
                    return Err(E_INVALID);
                }
                let end = offset as usize + data.len();
                if end > RAM_FILE_MAX {
                    return Err(E_TOO_BIG);
                }
                r.data[i][offset as usize..end].copy_from_slice(data);
                r.files[i].size = r.files[i].size.max(end as u32);
                pfs::WriteResponse {
                    written: data.len() as u32,
                }
                .encode_msg(out)
                .map_err(|_| E_IO)
            }
            TAG_BOOT if self.mounts & MOUNT_BOOT != 0 => Err(E_READ_ONLY),
            _ => Err(E_INVALID),
        }
    }

    fn truncate(&mut self, ino: u32, size: u64, out: &mut [u8; 4096]) -> Op {
        let (tag, raw) = (ino >> TAG_SHIFT, ino & ((1 << TAG_SHIFT) - 1));
        match tag {
            TAG_DISK if self.mounts & MOUNT_DISK != 0 => {
                let m = pfs::TruncateRequest { ino: raw, size }
                    .encode_msg(&mut self.msg)
                    .map_err(|_| E_IO)?;
                let n = roundtrip(FS, &mut self.msg, m);
                pfs::decode_truncate_response(&self.msg[..n]).map_err(remote_code)?;
                pfs::TruncateResponse {}.encode_msg(out).map_err(|_| E_IO)
            }
            TAG_TMP if self.mounts & MOUNT_TMP != 0 => {
                let r = ramfs();
                let i = raw as usize;
                if i >= RAM_FILES || !r.files[i].used {
                    return Err(E_INVALID);
                }
                if size < r.files[i].size as u64 {
                    r.files[i].size = size as u32;
                }
                pfs::TruncateResponse {}.encode_msg(out).map_err(|_| E_IO)
            }
            TAG_BOOT if self.mounts & MOUNT_BOOT != 0 => Err(E_READ_ONLY),
            _ => Err(E_INVALID),
        }
    }

    fn list(&mut self, path: &[u8], out: &mut [u8; 4096]) -> Op {
        let (mount, rest) = split_mount(path);
        if mount.is_empty() {
            let mut resp = pfs::ListResponse {
                count: 0,
                entries: [0; 3900],
                entries_len: 0,
            };
            let mut pos = 0usize;
            for (bit, name) in [
                (MOUNT_BOOT, &b"boot"[..]),
                (MOUNT_DISK, &b"disk"[..]),
                (MOUNT_TMP, &b"tmp"[..]),
            ] {
                if self.mounts & bit != 0 {
                    resp.entries[pos..pos + 4].copy_from_slice(&0u32.to_le_bytes());
                    resp.entries[pos + 4] = 2;
                    resp.entries[pos + 5] = name.len() as u8;
                    resp.entries[pos + 6..pos + 6 + name.len()].copy_from_slice(name);
                    pos += 6 + name.len();
                    resp.count += 1;
                }
            }
            resp.entries_len = pos as u32;
            return resp.encode_msg(out).map_err(|_| E_IO);
        }
        if mount == b"disk" && self.mounts & MOUNT_DISK != 0 {
            let (path, path_len) = path_of(rest);
            let m = pfs::ListRequest { path, path_len }
                .encode_msg(&mut self.msg)
                .map_err(|_| E_IO)?;
            let n = roundtrip(FS, &mut self.msg, m);
            let r = pfs::decode_list_response(&self.msg[..n]).map_err(remote_code)?;
            r.encode_msg(out).map_err(|_| E_IO)
        } else if mount == b"boot" && self.mounts & MOUNT_BOOT != 0 {
            let (path, path_len) = path_of(rest);
            let m = pesp::ListRequest { path, path_len }
                .encode_msg(&mut self.msg)
                .map_err(|_| E_IO)?;
            let n = roundtrip(ESP, &mut self.msg, m);
            let r = pesp::decode_list_response(&self.msg[..n]).map_err(|e| {
                if remote_code(e) == 3 {
                    E_NOT_FOUND
                } else {
                    E_IO
                }
            })?;
            // converte [attr u8][size u32][len u8][nome] -> [ino u32][kind u8][len u8][nome]
            let src = r.entries();
            let mut resp = pfs::ListResponse {
                count: r.count,
                entries: [0; 3900],
                entries_len: 0,
            };
            let (mut i, mut o) = (0usize, 0usize);
            while i + 6 <= src.len() {
                let attr = src[i];
                let nl = src[i + 5] as usize;
                resp.entries[o..o + 4].copy_from_slice(&0u32.to_le_bytes());
                resp.entries[o + 4] = if attr & 0x10 != 0 { 2 } else { 1 };
                resp.entries[o + 5] = nl as u8;
                resp.entries[o + 6..o + 6 + nl].copy_from_slice(&src[i + 6..i + 6 + nl]);
                i += 6 + nl;
                o += 6 + nl;
            }
            resp.entries_len = o as u32;
            resp.encode_msg(out).map_err(|_| E_IO)
        } else if mount == b"tmp" && self.mounts & MOUNT_TMP != 0 {
            if !rest.is_empty() {
                return Err(E_NOT_FOUND);
            }
            let r = ramfs();
            let mut resp = pfs::ListResponse {
                count: 0,
                entries: [0; 3900],
                entries_len: 0,
            };
            let mut pos = 0usize;
            for i in 0..RAM_FILES {
                if !r.files[i].used {
                    continue;
                }
                let nl = r.files[i].name_len as usize;
                resp.entries[pos..pos + 4]
                    .copy_from_slice(&(i as u32 | (TAG_TMP << TAG_SHIFT)).to_le_bytes());
                resp.entries[pos + 4] = 1;
                resp.entries[pos + 5] = nl as u8;
                resp.entries[pos + 6..pos + 6 + nl].copy_from_slice(&r.files[i].name[..nl]);
                pos += 6 + nl;
                resp.count += 1;
            }
            resp.entries_len = pos as u32;
            resp.encode_msg(out).map_err(|_| E_IO)
        } else {
            Err(E_NOT_FOUND)
        }
    }

    fn sync(&mut self, out: &mut [u8; 4096]) -> Op {
        if self.mounts & MOUNT_DISK != 0 {
            let m = pfs::SyncRequest {}
                .encode_msg(&mut self.msg)
                .map_err(|_| E_IO)?;
            let n = roundtrip(FS, &mut self.msg, m);
            pfs::decode_sync_response(&self.msg[..n]).map_err(remote_code)?;
        }
        pfs::SyncResponse {}.encode_msg(out).map_err(|_| E_IO)
    }

    fn info(&mut self, out: &mut [u8; 4096]) -> Op {
        if self.mounts & MOUNT_DISK == 0 {
            return Err(E_INVALID);
        }
        let m = pfs::InfoRequest {}
            .encode_msg(&mut self.msg)
            .map_err(|_| E_IO)?;
        let n = roundtrip(FS, &mut self.msg, m);
        let r = pfs::decode_info_response(&self.msg[..n]).map_err(remote_code)?;
        r.encode_msg(out).map_err(|_| E_IO)
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
    let mut vfs = Vfs {
        mounts,
        msg: [0; 4096],
    };
    let mut req = [0u8; 4096];
    let mut out = [0u8; 4096];
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
        let request = match pfs::decode_request(&req[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = pfs::encode_error(0, 255, &mut out).unwrap_or(0);
                let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
                continue;
            }
        };
        let (method, r) = match &request {
            pfs::Request::Stat(rq) => {
                let (p, pl) = (rq.path, rq.path_len);
                let path = &p[..(pl as usize).min(256)];
                (pfs::StatRequest::METHOD_ID, vfs.stat(path, &mut out))
            }
            pfs::Request::Create(rq) => {
                let (p, pl) = (rq.path, rq.path_len);
                (
                    pfs::CreateRequest::METHOD_ID,
                    vfs.create(&p[..(pl as usize).min(256)], false, &mut out),
                )
            }
            pfs::Request::Mkdir(rq) => {
                let (p, pl) = (rq.path, rq.path_len);
                (
                    pfs::MkdirRequest::METHOD_ID,
                    vfs.create(&p[..(pl as usize).min(256)], true, &mut out),
                )
            }
            pfs::Request::Unlink(rq) => {
                let (p, pl) = (rq.path, rq.path_len);
                (
                    pfs::UnlinkRequest::METHOD_ID,
                    vfs.unlink(&p[..(pl as usize).min(256)], &mut out),
                )
            }
            pfs::Request::Read(rq) => (
                pfs::ReadRequest::METHOD_ID,
                vfs.read(rq.ino, rq.offset, rq.len, &mut out),
            ),
            pfs::Request::Write(rq) => (
                pfs::WriteRequest::METHOD_ID,
                vfs.write(rq.ino, rq.offset, rq.data(), &mut out),
            ),
            pfs::Request::List(rq) => {
                let (p, pl) = (rq.path, rq.path_len);
                (
                    pfs::ListRequest::METHOD_ID,
                    vfs.list(&p[..(pl as usize).min(256)], &mut out),
                )
            }
            pfs::Request::Sync(_) => (pfs::SyncRequest::METHOD_ID, vfs.sync(&mut out)),
            pfs::Request::Info(_) => (pfs::InfoRequest::METHOD_ID, vfs.info(&mut out)),
            pfs::Request::Truncate(rq) => (
                pfs::TruncateRequest::METHOD_ID,
                vfs.truncate(rq.ino, rq.size, &mut out),
            ),
        };
        let m = match r {
            Ok(m) => m,
            Err(code) => pfs::encode_error(method, code as u32, &mut out).unwrap_or(0),
        };
        if nexo_sys::channel_send(CLIENT, &out[..m], &[]) != Status::Ok {
            fail(51, "send");
        }
    }
}

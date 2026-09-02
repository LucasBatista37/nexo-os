//! `upd` — gerente de atualização A/B (Plano §Fase 8, ADR-0010).
//! Handle 0 = canal de controle; handle 1 = canal `nexo.block` do DISCO DE BOOT (gravável).
//! Localiza `\nexo\slots.bin` no ESP (GPT → FAT via `nexo-fat`) e atende:
//!   "confirma" → **health check pós-boot**: marca o slot arrancado (`last_selected`) como
//!                saudável (sucesso = 1, tentativas repostas, prioridade 2) e, se o OUTRO slot
//!                está confirmado, rebaixa-o para prioridade 1 — mas um update **pendente** em
//!                voo nunca é tocado. É a contraparte do loader, que desconta uma tentativa de
//!                slot pendente a cada boot: sem confirmação o loader volta ao outro slot;
//!   "aplica"   → **atualização atômica**: copia `kernel.elf` + `initrd` do slot ATIVO para o
//!                INATIVO (reescrita FAT à prova de cortes, `nexo-fat::rewrite_file`) e marca o
//!                inativo pendente (prioridade 3, 3 tentativas, sem sucesso) — o boot seguinte
//!                arranca por ele e o confirma, ou o rollback devolve ao atual;
//!   "verifica" → relê `kernel.elf` + `initrd` dos DOIS slots e compara byte a byte;
//!   "estado"   → relê o setor do disco: "sel X A pP tT sS B pP tT sS" (diagnóstico).
#![no_std]
#![no_main]

use nexo_boot_abi::slots::{BYTES, COUNT, State};
use nexo_fat::{Fat, IoError, SECTOR, SectorDevice, SectorDeviceRw, find_esp};
use nexo_proto::block::{self, CapacityRequest, ReadRequest, WriteRequest};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const BLK: Handle = 1;
/// Tentativas repostas ao confirmar/aplicar (o valor inicial do `build-image`).
const TRIES_RESET: u8 = 3;
/// Maior artefato copiável de um slot (kernel + folga; initrd atual ~1,6 MiB).
const FILE_MAX: usize = 4 * 1024 * 1024;

/// Palco da cópia entre slots (um arquivo por vez; processo de uma só thread).
static mut COPYBUF: [u8; FILE_MAX] = [0; FILE_MAX];

fn fail(code: i64, what: &str) -> ! {
    log!("upd: falha: {}", what);
    nexo_sys::exit(code)
}

/// Setores via `nexo.block`, um por pedido (suficiente; a cópia toda tem poucos MiB).
struct ChanDisk {
    sectors: u64,
    req: [u8; 4096],
    reply: [u8; 4096],
}

impl ChanDisk {
    fn open() -> Result<Self, &'static str> {
        let mut d = ChanDisk {
            sectors: 0,
            req: [0; 4096],
            reply: [0; 4096],
        };
        let m = CapacityRequest {}
            .encode_msg(&mut d.req)
            .map_err(|_| "encode capacidade")?;
        if nexo_sys::channel_send(BLK, &d.req[..m], &[]) != Status::Ok {
            return Err("send capacidade");
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut d.reply, &mut hs) {
            Ok((n, _)) => match block::decode_capacity_response(&d.reply[..n]) {
                Ok(r) => {
                    d.sectors = r.sectors;
                    Ok(d)
                }
                Err(_) => Err("capacidade"),
            },
            _ => Err("capacidade"),
        }
    }

    fn read_lba(&mut self, lba: u64, out: &mut [u8; SECTOR]) -> Result<(), IoError> {
        let rq = ReadRequest {
            sector: lba,
            count: 1,
        };
        let m = rq.encode_msg(&mut self.req).map_err(|_| IoError)?;
        if nexo_sys::channel_send(BLK, &self.req[..m], &[]) != Status::Ok {
            return Err(IoError);
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut self.reply, &mut hs) {
            Ok((n, _)) => match block::decode_read_response(&self.reply[..n]) {
                Ok(r) if r.data().len() == SECTOR => {
                    out.copy_from_slice(r.data());
                    Ok(())
                }
                _ => Err(IoError),
            },
            _ => Err(IoError),
        }
    }

    fn write_lba(&mut self, lba: u64, data: &[u8; SECTOR]) -> Result<(), IoError> {
        let mut rq = WriteRequest {
            sector: lba,
            count: 1,
            data: [0; 3584],
            data_len: SECTOR as u32,
        };
        rq.data[..SECTOR].copy_from_slice(data);
        let m = rq.encode_msg(&mut self.req).map_err(|_| IoError)?;
        if nexo_sys::channel_send(BLK, &self.req[..m], &[]) != Status::Ok {
            return Err(IoError);
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut self.reply, &mut hs) {
            Ok((n, _)) if block::decode_write_response(&self.reply[..n]).is_ok() => Ok(()),
            _ => Err(IoError),
        }
    }
}

impl SectorDevice for ChanDisk {
    fn sector_count(&self) -> u64 {
        self.sectors
    }
    fn read_sector(&mut self, lba: u64, buf: &mut [u8; SECTOR]) -> Result<(), IoError> {
        self.read_lba(lba, buf)
    }
}

impl SectorDeviceRw for ChanDisk {
    fn write_sector(&mut self, lba: u64, buf: &[u8; SECTOR]) -> Result<(), IoError> {
        self.write_lba(lba, buf)
    }
}

/// Lê e valida o estado dos slots no setor `lba` do dispositivo do `fs`.
fn read_state(fs: &mut Fat<ChanDisk>, lba: u64) -> Option<State> {
    let mut sec = [0u8; SECTOR];
    fs.device_mut().read_lba(lba, &mut sec).ok()?;
    State::decode(&sec)
}

fn write_state(fs: &mut Fat<ChanDisk>, lba: u64, st: &State) -> Result<(), IoError> {
    fs.device_mut().write_lba(lba, &st.encode())
}

/// Caminhos dos artefatos de um slot (0 = A, 1 = B).
fn slot_paths(slot: usize) -> [&'static [u8]; 2] {
    if slot == 0 {
        [b"/nexo/a/kernel.elf", b"/nexo/a/initrd"]
    } else {
        [b"/nexo/b/kernel.elf", b"/nexo/b/initrd"]
    }
}

/// Lê `path` inteiro para o palco; devolve o tamanho.
fn read_all(fs: &mut Fat<ChanDisk>, path: &[u8], buf: &mut [u8]) -> Option<usize> {
    let e = fs.lookup(path).ok()?;
    let size = e.size as usize;
    if size > buf.len() {
        return None;
    }
    if fs.read(&e, 0, &mut buf[..size]).ok()? != size {
        return None;
    }
    Some(size)
}

/// Copia `kernel.elf` + `initrd` do slot `from` para o slot `to` (reescrita à prova de cortes).
fn copy_slot(fs: &mut Fat<ChanDisk>, from: usize, to: usize) -> Option<u32> {
    // SAFETY: unico acesso; processo de uma so thread (palco estatico para os artefatos).
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(COPYBUF) };
    let mut copied = 0u32;
    for (src, dst) in slot_paths(from).into_iter().zip(slot_paths(to)) {
        let n = read_all(fs, src, buf)?;
        fs.rewrite_file(dst, n as u64, |off, out| {
            let o = off as usize;
            out.copy_from_slice(&buf[o..o + out.len()]);
            Ok(())
        })
        .ok()?;
        copied += 1;
    }
    Some(copied)
}

/// Compara os artefatos dos dois slots byte a byte.
fn slots_equal(fs: &mut Fat<ChanDisk>) -> Option<bool> {
    // SAFETY: unico acesso; processo de uma so thread.
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(COPYBUF) };
    let (a, b) = buf.split_at_mut(FILE_MAX / 2);
    for (pa, pb) in slot_paths(0).into_iter().zip(slot_paths(1)) {
        let na = read_all_half(fs, pa, a)?;
        let nb = read_all_half(fs, pb, b)?;
        if na != nb || a[..na] != b[..nb] {
            return Some(false);
        }
    }
    Some(true)
}

fn read_all_half(fs: &mut Fat<ChanDisk>, path: &[u8], half: &mut [u8]) -> Option<usize> {
    read_all(fs, path, half)
}

/// Resposta de "estado", com posições fixas: `sel X A p_ t_ s_ B p_ t_ s_` (28 bytes;
/// dígitos em 9/12/15 para o A e 20/23/26 para o B; o selecionado em 4).
fn fmt_state(st: &State) -> [u8; 28] {
    let mut r = *b"sel X A p_ t_ s_ B p_ t_ s_ ";
    r[4] = if st.last_selected == 0 { b'A' } else { b'B' };
    for (i, base) in [(0usize, 9usize), (1, 20)] {
        let s = &st.slot[i];
        r[base] = b'0' + s.priority.min(9);
        r[base + 3] = b'0' + s.tries_remaining.min(9);
        r[base + 6] = b'0' + s.successful.min(9);
    }
    r
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut disk = ChanDisk::open().unwrap_or_else(|e| fail(10, e));
    let part = find_esp(&mut disk).unwrap_or_else(|_| fail(11, "GPT/ESP"));
    let mut fs = Fat::mount(disk, part.first_lba).unwrap_or_else(|_| fail(12, "montagem FAT"));
    let entry = fs
        .lookup(b"/nexo/slots.bin")
        .unwrap_or_else(|_| fail(13, "slots.bin ausente (imagem sem layout A/B)"));
    if entry.size as usize != BYTES {
        fail(14, "slots.bin com tamanho inesperado");
    }
    let lba = fs
        .first_sector_lba(&entry)
        .unwrap_or_else(|_| fail(15, "setor do slots.bin"));
    log!("upd: pronto (slots.bin em LBA {})", lba);

    let mut buf = [0u8; 64];
    let mut hs = [0u32; 1];
    loop {
        let (n, _) = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(_) => nexo_sys::exit(0), // cordão de vida
        };
        match &buf[..n] {
            b"confirma" => {
                let Some(mut st) = read_state(&mut fs, lba) else {
                    let _ = nexo_sys::channel_send(PIPE, b"erro estado", &[]);
                    continue;
                };
                let sel = (st.last_selected as usize).min(COUNT - 1);
                let other = 1 - sel;
                st.slot[sel].successful = 1;
                st.slot[sel].tries_remaining = TRIES_RESET;
                st.slot[sel].priority = 2;
                // normaliza o outro APENAS se confirmado — um update pendente em voo
                // (sucesso 0, tentativas > 0) mantém a prioridade para vencer o próximo boot
                if st.slot[other].successful == 1 {
                    st.slot[other].priority = 1;
                }
                if write_state(&mut fs, lba, &st).is_err() {
                    let _ = nexo_sys::channel_send(PIPE, b"erro escrita", &[]);
                    continue;
                }
                let letter = if sel == 0 { b'A' } else { b'B' };
                log!(
                    "upd: slot {} confirmado (health check pos-boot)",
                    letter as char
                );
                let mut r = *b"ok X";
                r[3] = letter;
                let _ = nexo_sys::channel_send(PIPE, &r, &[]);
            }
            b"aplica" => {
                let Some(mut st) = read_state(&mut fs, lba) else {
                    let _ = nexo_sys::channel_send(PIPE, b"erro estado", &[]);
                    continue;
                };
                let sel = (st.last_selected as usize).min(COUNT - 1);
                let to = 1 - sel;
                let Some(copied) = copy_slot(&mut fs, sel, to) else {
                    let _ = nexo_sys::channel_send(PIPE, b"erro copia", &[]);
                    continue;
                };
                st.slot[to].successful = 0;
                st.slot[to].tries_remaining = TRIES_RESET;
                st.slot[to].priority = 3;
                if write_state(&mut fs, lba, &st).is_err() {
                    let _ = nexo_sys::channel_send(PIPE, b"erro escrita", &[]);
                    continue;
                }
                let letter = if to == 0 { b'A' } else { b'B' };
                log!(
                    "upd: atualizacao aplicada no slot {} ({} arquivo(s); pendente, prioridade 3)",
                    letter as char,
                    copied
                );
                let mut r = *b"aplicado X";
                r[9] = letter;
                let _ = nexo_sys::channel_send(PIPE, &r, &[]);
            }
            b"verifica" => {
                let r: &[u8] = match slots_equal(&mut fs) {
                    Some(true) => b"igual",
                    Some(false) => b"difere",
                    None => b"erro leitura",
                };
                let _ = nexo_sys::channel_send(PIPE, r, &[]);
            }
            b"estado" => {
                let Some(st) = read_state(&mut fs, lba) else {
                    let _ = nexo_sys::channel_send(PIPE, b"erro estado", &[]);
                    continue;
                };
                let r = fmt_state(&st);
                let _ = nexo_sys::channel_send(PIPE, &r, &[]);
            }
            _ => {
                let _ = nexo_sys::channel_send(PIPE, b"?", &[]);
            }
        }
    }
}

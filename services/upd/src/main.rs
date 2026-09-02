//! `upd` — gerente de atualização A/B (Plano §Fase 8, ADR-0010): o **health check pós-boot**.
//! Handle 0 = canal de controle; handle 1 = canal `nexo.block` do DISCO DE BOOT (gravável).
//! Localiza `\nexo\slots.bin` no ESP (GPT → FAT via `nexo-fat`) e atende:
//!   "confirma" → marca o slot arrancado (`last_selected`) como saudável (sucesso = 1,
//!                tentativas repostas) e grava o setor de volta. É a contraparte do loader,
//!                que desconta uma tentativa de slot pendente a cada boot: sem confirmação as
//!                tentativas esgotam e o loader volta ao outro slot — rollback automático;
//!   "estado"  → relê o setor DO DISCO e responde "sel X sN tN" (diagnóstico/verificação).
#![no_std]
#![no_main]

use nexo_boot_abi::slots::{BYTES, COUNT, State};
use nexo_fat::{Fat, IoError, SECTOR, SectorDevice, find_esp};
use nexo_proto::block::{self, CapacityRequest, ReadRequest, WriteRequest};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const BLK: Handle = 1;
/// Tentativas repostas ao confirmar (o mesmo valor inicial gravado pelo `build-image`).
const TRIES_RESET: u8 = 3;

fn fail(code: i64, what: &str) -> ! {
    log!("upd: falha: {}", what);
    nexo_sys::exit(code)
}

/// Setores via `nexo.block`, um por pedido (o upd lê pouco: GPT + FAT + um setor de estado).
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

/// Lê e valida o estado dos slots no setor `lba`.
fn read_state(disk: &mut ChanDisk, lba: u64) -> Option<State> {
    let mut sec = [0u8; SECTOR];
    disk.read_lba(lba, &mut sec).ok()?;
    State::decode(&sec)
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
    let mut disk = fs.into_device();
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
                let Some(mut st) = read_state(&mut disk, lba) else {
                    let _ = nexo_sys::channel_send(PIPE, b"erro estado", &[]);
                    continue;
                };
                let sel = (st.last_selected as usize).min(COUNT - 1);
                st.slot[sel].successful = 1;
                st.slot[sel].tries_remaining = TRIES_RESET;
                if disk.write_lba(lba, &st.encode()).is_err() {
                    let _ = nexo_sys::channel_send(PIPE, b"erro escrita", &[]);
                    continue;
                }
                let letter = if sel == 0 { "A" } else { "B" };
                log!("upd: slot {} confirmado (health check pos-boot)", letter);
                let mut r = *b"ok X";
                r[3] = letter.as_bytes()[0];
                let _ = nexo_sys::channel_send(PIPE, &r, &[]);
            }
            b"estado" => {
                let Some(st) = read_state(&mut disk, lba) else {
                    let _ = nexo_sys::channel_send(PIPE, b"erro estado", &[]);
                    continue;
                };
                let sel = (st.last_selected as usize).min(COUNT - 1);
                let s = &st.slot[sel];
                // tentativas de um dígito bastam (TRIES_RESET = 3; o loader só desconta)
                let mut r = *b"sel X sS tT";
                r[4] = if sel == 0 { b'A' } else { b'B' };
                r[7] = b'0' + s.successful.min(9);
                r[10] = b'0' + s.tries_remaining.min(9);
                let _ = nexo_sys::channel_send(PIPE, &r, &[]);
            }
            _ => {
                let _ = nexo_sys::channel_send(PIPE, b"?", &[]);
            }
        }
    }
}

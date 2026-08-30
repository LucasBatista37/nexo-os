//! `espfs` — leitor somente leitura da partição de sistema EFI (GPT → FAT12/16/32) sobre o
//! canal de um `blockdev`. Handle 0 = canal do bloco, handle 1 = cliente.
//! Protocolo **tipado** `nexo.esp` v1.0 (gerado de `idl/esp.idl`): `list`, `stat`, `read`.
#![no_std]
#![no_main]

use nexo_fat::{Fat, FatError, IoError, SECTOR, SectorDevice};
use nexo_proto::block::{self, CapacityRequest, ReadRequest};
use nexo_proto::esp as pesp;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const BLK: Handle = 0;
const CLIENT: Handle = 1;
const CHUNK: u64 = 7; // setores por pedido ao driver (cabem em uma mensagem)

/// Setores via `nexo.block`, com cache do último pedaço de 7 setores.
struct ChanDisk {
    sectors: u64,
    cache_lba: u64,
    cache: [u8; 7 * SECTOR],
    req: [u8; 64],
    reply: [u8; 64 + 7 * SECTOR],
}

impl ChanDisk {
    fn open() -> Result<Self, &'static str> {
        let mut d = ChanDisk {
            sectors: 0,
            cache_lba: u64::MAX,
            cache: [0; 7 * SECTOR],
            req: [0; 64],
            reply: [0; 64 + 7 * SECTOR],
        };
        let m = CapacityRequest {}
            .encode_msg(&mut d.req)
            .map_err(|_| "encode")?;
        if nexo_sys::channel_send(BLK, &d.req[..m], &[]) != Status::Ok {
            return Err("send");
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
}

impl SectorDevice for ChanDisk {
    fn sector_count(&self) -> u64 {
        self.sectors
    }
    fn read_sector(&mut self, lba: u64, buf: &mut [u8; SECTOR]) -> Result<(), IoError> {
        if lba >= self.sectors {
            return Err(IoError);
        }
        if self.cache_lba == u64::MAX || lba < self.cache_lba || lba >= self.cache_lba + CHUNK {
            let start = lba.min(self.sectors.saturating_sub(CHUNK));
            let count = CHUNK.min(self.sectors - start) as u32;
            let rq = ReadRequest {
                sector: start,
                count,
            };
            let m = rq.encode_msg(&mut self.req).map_err(|_| IoError)?;
            if nexo_sys::channel_send(BLK, &self.req[..m], &[]) != Status::Ok {
                return Err(IoError);
            }
            let mut hs = [0u32; 1];
            match nexo_sys::channel_recv(BLK, &mut self.reply, &mut hs) {
                Ok((n, _)) => match block::decode_read_response(&self.reply[..n]) {
                    Ok(r) if r.data().len() == count as usize * SECTOR => {
                        self.cache[..r.data().len()].copy_from_slice(r.data());
                        self.cache_lba = start;
                    }
                    _ => return Err(IoError),
                },
                _ => return Err(IoError),
            }
        }
        let off = ((lba - self.cache_lba) as usize) * SECTOR;
        buf.copy_from_slice(&self.cache[off..off + SECTOR]);
        Ok(())
    }
}

fn fail(code: i64, what: &str) -> ! {
    log!("espfs: falha: {}", what);
    nexo_sys::exit(code)
}

fn code(e: FatError) -> u8 {
    match e {
        FatError::Io => 1,
        FatError::Corrupted(_) => 2,
        FatError::NotFound => 3,
        FatError::NotDir => 5,
        FatError::IsDir => 6,
        FatError::NoEsp => 12,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut disk = ChanDisk::open().unwrap_or_else(|e| fail(40, e));
    let part = match nexo_fat::find_esp(&mut disk) {
        Ok(p) => p,
        Err(e) => {
            log!("espfs: GPT/ESP: {:?}", e);
            nexo_sys::exit(41)
        }
    };
    let mut fs = match Fat::mount(disk, part.first_lba) {
        Ok(fs) => fs,
        Err(e) => {
            log!("espfs: montagem FAT: {:?}", e);
            nexo_sys::exit(42)
        }
    };
    let boot = fs
        .lookup(b"/EFI/BOOT/BOOTX64.EFI")
        .map(|e| e.size)
        .unwrap_or(0);
    let kernel = fs.lookup(b"/nexo/kernel.elf").map(|e| e.size).unwrap_or(0);
    log!(
        "espfs: ESP {:?} em LBA {}..{}: /EFI/BOOT/BOOTX64.EFI {} bytes, /nexo/kernel.elf {} bytes",
        fs.kind(),
        part.first_lba,
        part.last_lba,
        boot,
        kernel
    );
    let mut req = [0u8; 4096];
    let mut out = [0u8; 4096];
    let mut hs = [0u32; 1];
    loop {
        let (n, _) = match nexo_sys::channel_recv(CLIENT, &mut req, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(43, "recv"),
        };
        let request = match pesp::decode_request(&req[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = pesp::encode_error(0, 255, &mut out).unwrap_or(0);
                let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
                continue;
            }
        };
        fn err(method: u32, e: FatError, out: &mut [u8; 4096]) -> usize {
            pesp::encode_error(method, code(e) as u32, out).unwrap_or(0)
        }
        let m = match request {
            pesp::Request::List(rq) => {
                let r = fs.lookup(rq.path()).and_then(|dir| {
                    if !dir.is_dir() {
                        return Err(FatError::NotDir);
                    }
                    let mut resp = pesp::ListResponse {
                        count: 0,
                        entries: [0; 3900],
                        entries_len: 0,
                    };
                    let mut pos = 0usize;
                    fs.for_each_entry(dir.cluster, |e| {
                        let name = e.name();
                        if pos + 6 + name.len() <= resp.entries.len() {
                            resp.entries[pos] = e.attr;
                            resp.entries[pos + 1..pos + 5].copy_from_slice(&e.size.to_le_bytes());
                            resp.entries[pos + 5] = name.len() as u8;
                            resp.entries[pos + 6..pos + 6 + name.len()].copy_from_slice(name);
                            pos += 6 + name.len();
                            resp.count += 1;
                            true
                        } else {
                            false
                        }
                    })?;
                    resp.entries_len = pos as u32;
                    Ok(resp)
                });
                match r {
                    Ok(resp) => resp.encode_msg(&mut out).unwrap_or(0),
                    Err(e) => err(pesp::ListRequest::METHOD_ID, e, &mut out),
                }
            }
            pesp::Request::Stat(rq) => match fs.lookup(rq.path()) {
                Ok(e) => pesp::StatResponse {
                    attr: e.attr,
                    size: e.size,
                }
                .encode_msg(&mut out)
                .unwrap_or(0),
                Err(e) => err(pesp::StatRequest::METHOD_ID, e, &mut out),
            },
            pesp::Request::Read(rq) => {
                let want = (rq.len as usize).min(3500);
                let mut resp = pesp::ReadResponse {
                    data: [0; 3500],
                    data_len: 0,
                };
                let r = fs
                    .lookup(rq.path())
                    .and_then(|e| fs.read(&e, rq.offset, &mut resp.data[..want]));
                match r {
                    Ok(got) => {
                        resp.data_len = got as u32;
                        resp.encode_msg(&mut out).unwrap_or(0)
                    }
                    Err(e) => err(pesp::ReadRequest::METHOD_ID, e, &mut out),
                }
            }
        };
        if nexo_sys::channel_send(CLIENT, &out[..m], &[]) != Status::Ok {
            fail(44, "send");
        }
    }
}

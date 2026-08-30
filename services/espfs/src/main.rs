//! `espfs` — leitor somente leitura da partição de sistema EFI (GPT → FAT12/16/32) sobre o
//! canal de um `blockdev`. Handle 0 = canal do bloco, handle 1 = cliente.
//! Protocolo cru `nexo.esp` v0 (`docs/spec/ipc-compat.md` §5): pedido
//! `[op u8][pad 3][offset u64][len u32][caminho]`; ops 0 list, 1 stat, 2 read;
//! resposta `[status u8][pad 3][valor u64][dados]`.
#![no_std]
#![no_main]

use nexo_fat::{Fat, FatError, IoError, SECTOR, SectorDevice};
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
    req: [u8; 16],
    reply: [u8; 1 + 7 * SECTOR],
}

impl ChanDisk {
    fn open() -> Result<Self, &'static str> {
        let mut d = ChanDisk {
            sectors: 0,
            cache_lba: u64::MAX,
            cache: [0; 7 * SECTOR],
            req: [0; 16],
            reply: [0; 1 + 7 * SECTOR],
        };
        d.req[0] = 2;
        if nexo_sys::channel_send(BLK, &d.req, &[]) != Status::Ok {
            return Err("send");
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut d.reply, &mut hs) {
            Ok((9, _)) if d.reply[0] == 0 => {
                d.sectors = u64::from_le_bytes(d.reply[1..9].try_into().unwrap());
                Ok(d)
            }
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
            self.req[0] = 0;
            self.req[1..4].fill(0);
            self.req[4..12].copy_from_slice(&start.to_le_bytes());
            self.req[12..16].copy_from_slice(&count.to_le_bytes());
            if nexo_sys::channel_send(BLK, &self.req, &[]) != Status::Ok {
                return Err(IoError);
            }
            let mut hs = [0u32; 1];
            match nexo_sys::channel_recv(BLK, &mut self.reply, &mut hs) {
                Ok((n, _)) if n == 1 + count as usize * SECTOR && self.reply[0] == 0 => {
                    self.cache[..n - 1].copy_from_slice(&self.reply[1..n]);
                    self.cache_lba = start;
                }
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

fn reply(status: u8, value: u64, data: &[u8], out: &mut [u8; 4096]) -> usize {
    out[0] = status;
    out[1..4].fill(0);
    out[4..12].copy_from_slice(&value.to_le_bytes());
    let n = data.len().min(4096 - 12);
    out[12..12 + n].copy_from_slice(&data[..n]);
    12 + n
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
    let mut data = [0u8; 4096];
    let mut hs = [0u32; 1];
    loop {
        let (n, _) = match nexo_sys::channel_recv(CLIENT, &mut req, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(43, "recv"),
        };
        if n < 16 {
            let len = reply(0xff, 0, &[], &mut out);
            let _ = nexo_sys::channel_send(CLIENT, &out[..len], &[]);
            continue;
        }
        let op = req[0];
        let offset = u64::from_le_bytes(req[4..12].try_into().unwrap());
        let len = u32::from_le_bytes(req[12..16].try_into().unwrap()) as usize;
        let path = &req[16..n];
        let result: Result<(u64, usize), FatError> = match op {
            0 => fs.lookup(path).and_then(|dir| {
                if !dir.is_dir() {
                    return Err(FatError::NotDir);
                }
                let mut pos = 0usize;
                let mut count = 0u64;
                fs.for_each_entry(dir.cluster, |e| {
                    let name = e.name();
                    if pos + 6 + name.len() <= data.len() - 12 {
                        data[pos] = e.attr;
                        data[pos + 1..pos + 5].copy_from_slice(&e.size.to_le_bytes());
                        data[pos + 5] = name.len() as u8;
                        data[pos + 6..pos + 6 + name.len()].copy_from_slice(name);
                        pos += 6 + name.len();
                        count += 1;
                        true
                    } else {
                        false
                    }
                })?;
                Ok((count, pos))
            }),
            1 => fs.lookup(path).map(|e| {
                data[0] = e.attr;
                data[1..5].copy_from_slice(&e.size.to_le_bytes());
                (e.size as u64, 5)
            }),
            2 => {
                let want = len.min(4096 - 12);
                fs.lookup(path).and_then(|e| {
                    fs.read(&e, offset, &mut data[..want])
                        .map(|r| (r as u64, r))
                })
            }
            _ => Err(FatError::Io),
        };
        let n = match result {
            Ok((value, dlen)) => reply(0, value, &data[..dlen], &mut out),
            Err(e) => reply(code(e), 0, &[], &mut out),
        };
        if nexo_sys::channel_send(CLIENT, &out[..n], &[]) != Status::Ok {
            fail(44, "send");
        }
    }
}

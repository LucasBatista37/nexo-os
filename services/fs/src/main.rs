//! `fs` — servidor NexoFS v0. Handle 0 = canal para o `blockdev` (`nexo.block`),
//! handle 1 = canal do cliente (`nexo.fs` v0, `docs/spec/ipc-compat.md` §5).
//! Argumento: 0 = montar (formata se não houver assinatura **ou se o volume estiver
//! inutilizável** — comportamento de volume de teste, registrado no log), 1 = formatar sempre,
//! 2 = montagem estrita (termina com 32 se o volume não montar).
#![no_std]
#![no_main]

use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexofs::{BLOCK, BlockDevice, Fs, FsError, IoError, Kind, SECTORS_PER_BLOCK};

const BLK: Handle = 0;
const CLIENT: Handle = 1;
/// Setores no fim do disco fora do volume (area crua usada pelos testes de bloco).
const RESERVED_TAIL_SECTORS: u64 = 256;

/// Dispositivo de blocos sobre o canal do `blockdev`, com um cache de leitura pequeno
/// (write-through: escrita atualiza o cache e vai direto ao driver).
const CACHE_BLOCKS: usize = 8;

struct ChanDisk {
    blocks: u64,
    req: [u8; 16 + BLOCK],
    reply: [u8; 1 + BLOCK],
    cache_tags: [u64; CACHE_BLOCKS],
    cache: [[u8; BLOCK]; CACHE_BLOCKS],
    hits: u64,
    misses: u64,
}

impl ChanDisk {
    fn open() -> Result<Self, &'static str> {
        let mut d = ChanDisk {
            blocks: 0,
            req: [0; 16 + BLOCK],
            reply: [0; 1 + BLOCK],
            cache_tags: [u64::MAX; CACHE_BLOCKS],
            cache: [[0; BLOCK]; CACHE_BLOCKS],
            hits: 0,
            misses: 0,
        };
        d.req[0] = 2; // capacidade
        if nexo_sys::channel_send(BLK, &d.req[..16], &[]) != Status::Ok {
            return Err("send");
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut d.reply, &mut hs) {
            Ok((9, _)) if d.reply[0] == 0 => {
                let sectors = u64::from_le_bytes(d.reply[1..9].try_into().unwrap());
                d.blocks = sectors.saturating_sub(RESERVED_TAIL_SECTORS) / SECTORS_PER_BLOCK;
                Ok(d)
            }
            _ => Err("capacidade"),
        }
    }

    fn header(&mut self, op: u8, block: u64) {
        self.req[0] = op;
        self.req[1..4].fill(0);
        self.req[4..12].copy_from_slice(&(block * SECTORS_PER_BLOCK).to_le_bytes());
        self.req[12..16].copy_from_slice(&(SECTORS_PER_BLOCK as u32).to_le_bytes());
    }
}

impl BlockDevice for ChanDisk {
    fn block_count(&self) -> u64 {
        self.blocks
    }
    fn read_block(&mut self, block: u64, buf: &mut [u8; BLOCK]) -> Result<(), IoError> {
        if block >= self.blocks {
            return Err(IoError);
        }
        let slot = (block % CACHE_BLOCKS as u64) as usize;
        if self.cache_tags[slot] == block {
            self.hits += 1;
            buf.copy_from_slice(&self.cache[slot]);
            return Ok(());
        }
        self.misses += 1;
        self.header(0, block);
        if nexo_sys::channel_send(BLK, &self.req[..16], &[]) != Status::Ok {
            return Err(IoError);
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut self.reply, &mut hs) {
            Ok((n, _)) if n == 1 + BLOCK && self.reply[0] == 0 => {
                buf.copy_from_slice(&self.reply[1..1 + BLOCK]);
                self.cache_tags[slot] = block;
                self.cache[slot].copy_from_slice(buf);
                Ok(())
            }
            _ => Err(IoError),
        }
    }
    fn write_block(&mut self, block: u64, buf: &[u8; BLOCK]) -> Result<(), IoError> {
        if block >= self.blocks {
            return Err(IoError);
        }
        let slot = (block % CACHE_BLOCKS as u64) as usize;
        self.cache_tags[slot] = block;
        self.cache[slot].copy_from_slice(buf);
        self.header(1, block);
        self.req[16..].copy_from_slice(buf);
        if nexo_sys::channel_send(BLK, &self.req, &[]) != Status::Ok {
            return Err(IoError);
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut self.reply, &mut hs) {
            Ok((1, _)) if self.reply[0] == 0 => Ok(()),
            _ => Err(IoError),
        }
    }
}

fn fail(code: i64, what: &str) -> ! {
    log!("fs: falha: {}", what);
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

fn err_code(e: FsError) -> u8 {
    e.code()
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(arg: u64) -> ! {
    let disk = ChanDisk::open().unwrap_or_else(|e| fail(30, e));
    let mut fs = if arg == 1 {
        Fs::format(disk, nexofs::DEFAULT_INODES).unwrap_or_else(|_| fail(31, "format"))
    } else {
        match Fs::mount(disk) {
            Ok(fs) => fs,
            Err(FsError::Corrupted("assinatura do superbloco")) => {
                log!("fs: volume sem assinatura; formatando NexoFS v0");
                let disk = ChanDisk::open().unwrap_or_else(|e| fail(30, e));
                Fs::format(disk, nexofs::DEFAULT_INODES).unwrap_or_else(|_| fail(31, "format"))
            }
            Err(e) if arg == 0 => {
                log!(
                    "fs: AVISO: volume inutilizavel ({:?}); formatando NexoFS v0 (volume de teste)",
                    e
                );
                let disk = ChanDisk::open().unwrap_or_else(|e| fail(30, e));
                Fs::format(disk, nexofs::DEFAULT_INODES).unwrap_or_else(|_| fail(31, "format"))
            }
            Err(e) => {
                log!("fs: montagem falhou: {:?}", e);
                nexo_sys::exit(32)
            }
        }
    };
    let info = fs.info();
    log!(
        "fs: montado NexoFS v0: {} blocos, {} livres, {} reparo(s), geracao {}",
        info.total_blocks,
        info.free_blocks,
        info.repairs,
        info.generation
    );
    let mut req = [0u8; 4096];
    let mut out = [0u8; 4096];
    let mut data = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut served = 0u64;
    loop {
        let (n, _) = match nexo_sys::channel_recv(CLIENT, &mut req, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => {
                let _ = fs.sync();
                {
                    let d = fs.device();
                    log!(
                        "fs: cache de blocos: {} acertos, {} leituras do driver",
                        d.hits,
                        d.misses
                    );
                }
                log!(
                    "fs: cliente desconectou apos {} pedidos; volume sincronizado",
                    served
                );
                nexo_sys::exit(0)
            }
            Err(_) => fail(33, "recv"),
        };
        served += 1;
        if n < 20 {
            let len = reply(0xff, 0, &[], &mut out);
            let _ = nexo_sys::channel_send(CLIENT, &out[..len], &[]);
            continue;
        }
        let op = req[0];
        let ino = u32::from_le_bytes(req[4..8].try_into().unwrap());
        let offset = u64::from_le_bytes(req[8..16].try_into().unwrap());
        let len = u32::from_le_bytes(req[16..20].try_into().unwrap()) as usize;
        let payload = &req[20..n];
        let result: Result<(u64, usize), FsError> = match op {
            0 => fs.stat(payload).map(|st| {
                data[0] = st.kind as u8;
                data[1..9].copy_from_slice(&st.size.to_le_bytes());
                (st.ino as u64, 9)
            }),
            1 => fs.create(payload, Kind::File).map(|i| (i as u64, 0)),
            2 => fs.create(payload, Kind::Dir).map(|i| (i as u64, 0)),
            3 => fs.unlink(payload).map(|_| (0, 0)),
            4 => {
                let want = len.min(4096 - 12);
                fs.read(ino, offset, &mut data[..want])
                    .map(|r| (r as u64, r))
            }
            5 => fs.write(ino, offset, payload).map(|w| (w as u64, 0)),
            6 => {
                let mut pos = 0usize;
                let mut count = 0u64;
                fs.list(payload, |name, st| {
                    if pos + 6 + name.len() <= data.len() - 12 {
                        data[pos..pos + 4].copy_from_slice(&st.ino.to_le_bytes());
                        data[pos + 4] = st.kind as u8;
                        data[pos + 5] = name.len() as u8;
                        data[pos + 6..pos + 6 + name.len()].copy_from_slice(name);
                        pos += 6 + name.len();
                        count += 1;
                    }
                })
                .map(|_| (count, pos))
            }
            7 => fs.sync().map(|_| (0, 0)),
            8 => {
                let i = fs.info();
                data[0..8].copy_from_slice(&i.total_blocks.to_le_bytes());
                data[8..16].copy_from_slice(&i.free_blocks.to_le_bytes());
                data[16..24].copy_from_slice(&(i.repairs as u64).to_le_bytes());
                data[24..32].copy_from_slice(&i.generation.to_le_bytes());
                Ok((0, 32))
            }
            9 => fs.truncate(ino, offset).map(|_| (0, 0)),
            _ => Err(FsError::InvalidArgs),
        };
        let n = match result {
            Ok((value, dlen)) => reply(0, value, &data[..dlen], &mut out),
            Err(e) => reply(err_code(e), 0, &[], &mut out),
        };
        if nexo_sys::channel_send(CLIENT, &out[..n], &[]) != Status::Ok {
            fail(34, "send");
        }
    }
}

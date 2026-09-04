//! `fs` — servidor NexoFS v0. Handle 0 = canal para o `blockdev` (`nexo.block` tipado),
//! handle 1 = canal do cliente (protocolo **tipado** `nexo.fs` v1.0, gerado de `idl/fs.idl`).
//! Argumento: 0 = montar (formata se não houver assinatura **ou se o volume estiver
//! inutilizável** — comportamento de volume de teste, registrado no log), 1 = formatar sempre,
//! 2 = montagem estrita (termina com 32 se o volume não montar).
#![no_std]
#![no_main]

use nexo_proto::block::{self, CapacityRequest, ReadRequest, WriteRequest};
use nexo_proto::fs as pfs;
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
    req: [u8; 64 + BLOCK],
    reply: [u8; 64 + BLOCK],
    cache_tags: [u64; CACHE_BLOCKS],
    cache: [[u8; BLOCK]; CACHE_BLOCKS],
    hits: u64,
    misses: u64,
}

impl ChanDisk {
    fn open() -> Result<Self, &'static str> {
        let mut d = ChanDisk {
            blocks: 0,
            req: [0; 64 + BLOCK],
            reply: [0; 64 + BLOCK],
            cache_tags: [u64::MAX; CACHE_BLOCKS],
            cache: [[0; BLOCK]; CACHE_BLOCKS],
            hits: 0,
            misses: 0,
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
                    d.blocks = r.sectors.saturating_sub(RESERVED_TAIL_SECTORS) / SECTORS_PER_BLOCK;
                    Ok(d)
                }
                Err(_) => Err("capacidade"),
            },
            _ => Err("capacidade"),
        }
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
        let rq = ReadRequest {
            sector: block * SECTORS_PER_BLOCK,
            count: SECTORS_PER_BLOCK as u32,
        };
        let m = rq.encode_msg(&mut self.req).map_err(|_| IoError)?;
        if nexo_sys::channel_send(BLK, &self.req[..m], &[]) != Status::Ok {
            return Err(IoError);
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut self.reply, &mut hs) {
            Ok((n, _)) => match block::decode_read_response(&self.reply[..n]) {
                Ok(r) if r.data().len() == BLOCK => {
                    buf.copy_from_slice(r.data());
                    self.cache_tags[slot] = block;
                    self.cache[slot].copy_from_slice(buf);
                    Ok(())
                }
                _ => Err(IoError),
            },
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
        let mut rq = WriteRequest {
            sector: block * SECTORS_PER_BLOCK,
            count: SECTORS_PER_BLOCK as u32,
            data: [0; 3584],
            data_len: BLOCK as u32,
        };
        rq.data[..BLOCK].copy_from_slice(buf);
        let m = rq.encode_msg(&mut self.req).map_err(|_| IoError)?;
        if nexo_sys::channel_send(BLK, &self.req[..m], &[]) != Status::Ok {
            return Err(IoError);
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(BLK, &mut self.reply, &mut hs) {
            Ok((n, _)) => block::decode_write_response(&self.reply[..n])
                .map(|_| ())
                .map_err(|_| IoError),
            _ => Err(IoError),
        }
    }
}

fn fail(code: i64, what: &str) -> ! {
    log!("fs: falha: {}", what);
    nexo_sys::exit(code)
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
        let request = match pfs::decode_request(&req[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = pfs::encode_error(0, 255, &mut out).unwrap_or(0);
                let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
                continue;
            }
        };
        fn err(method: u32, e: FsError, out: &mut [u8; 4096]) -> usize {
            pfs::encode_error(method, e.code() as u32, out).unwrap_or(0)
        }
        let m = match request {
            pfs::Request::Stat(rq) => match fs.stat(rq.path()) {
                Ok(st) => pfs::StatResponse {
                    ino: st.ino,
                    kind: st.kind as u8,
                    size: st.size,
                }
                .encode_msg(&mut out)
                .unwrap_or(0),
                Err(e) => err(pfs::StatRequest::METHOD_ID, e, &mut out),
            },
            pfs::Request::Create(rq) => match fs.create(rq.path(), Kind::File) {
                Ok(i) => pfs::CreateResponse { ino: i }
                    .encode_msg(&mut out)
                    .unwrap_or(0),
                Err(e) => err(pfs::CreateRequest::METHOD_ID, e, &mut out),
            },
            pfs::Request::Mkdir(rq) => match fs.create(rq.path(), Kind::Dir) {
                Ok(i) => pfs::MkdirResponse { ino: i }
                    .encode_msg(&mut out)
                    .unwrap_or(0),
                Err(e) => err(pfs::MkdirRequest::METHOD_ID, e, &mut out),
            },
            pfs::Request::Unlink(rq) => match fs.unlink(rq.path()) {
                Ok(()) => pfs::UnlinkResponse {}.encode_msg(&mut out).unwrap_or(0),
                Err(e) => err(pfs::UnlinkRequest::METHOD_ID, e, &mut out),
            },
            pfs::Request::Read(rq) => {
                let want = (rq.len as usize).min(4000);
                let mut resp = pfs::ReadResponse {
                    data: [0; 4000],
                    data_len: 0,
                };
                match fs.read(rq.ino, rq.offset, &mut resp.data[..want]) {
                    Ok(r) => {
                        resp.data_len = r as u32;
                        resp.encode_msg(&mut out).unwrap_or(0)
                    }
                    Err(e) => err(pfs::ReadRequest::METHOD_ID, e, &mut out),
                }
            }
            pfs::Request::Write(rq) => match fs.write(rq.ino, rq.offset, rq.data()) {
                Ok(w) => pfs::WriteResponse { written: w as u32 }
                    .encode_msg(&mut out)
                    .unwrap_or(0),
                Err(e) => err(pfs::WriteRequest::METHOD_ID, e, &mut out),
            },
            pfs::Request::List(rq) => {
                let mut resp = pfs::ListResponse {
                    count: 0,
                    entries: [0; 3900],
                    entries_len: 0,
                };
                let mut pos = 0usize;
                let r = fs.list(rq.path(), |name, st| {
                    if pos + 6 + name.len() <= resp.entries.len() {
                        resp.entries[pos..pos + 4].copy_from_slice(&st.ino.to_le_bytes());
                        resp.entries[pos + 4] = st.kind as u8;
                        resp.entries[pos + 5] = name.len() as u8;
                        resp.entries[pos + 6..pos + 6 + name.len()].copy_from_slice(name);
                        pos += 6 + name.len();
                        resp.count += 1;
                    }
                });
                match r {
                    Ok(_) => {
                        resp.entries_len = pos as u32;
                        resp.encode_msg(&mut out).unwrap_or(0)
                    }
                    Err(e) => err(pfs::ListRequest::METHOD_ID, e, &mut out),
                }
            }
            pfs::Request::Sync(_) => match fs.sync() {
                Ok(()) => pfs::SyncResponse {}.encode_msg(&mut out).unwrap_or(0),
                Err(e) => err(pfs::SyncRequest::METHOD_ID, e, &mut out),
            },
            pfs::Request::Info(_) => {
                let i = fs.info();
                pfs::InfoResponse {
                    total_blocks: i.total_blocks,
                    free_blocks: i.free_blocks,
                    repairs: i.repairs as u64,
                    generation: i.generation,
                }
                .encode_msg(&mut out)
                .unwrap_or(0)
            }
            pfs::Request::Truncate(rq) => match fs.truncate(rq.ino, rq.size) {
                Ok(()) => pfs::TruncateResponse {}.encode_msg(&mut out).unwrap_or(0),
                Err(e) => err(pfs::TruncateRequest::METHOD_ID, e, &mut out),
            },
            pfs::Request::Rename(rq) => match fs.rename(rq.from(), rq.to()) {
                Ok(()) => pfs::RenameResponse {}.encode_msg(&mut out).unwrap_or(0),
                Err(e) => err(pfs::RenameRequest::METHOD_ID, e, &mut out),
            },
        };
        if nexo_sys::channel_send(CLIENT, &out[..m], &[]) != Status::Ok {
            fail(34, "send");
        }
    }
}

//! `blockdev` — driver VirtIO-block em modo usuário (VirtIO 1.x sobre PCI, interface moderna,
//! fila dividida, MSI-X; transporte em `nexo-virtio`). Handle 0 = concessão de dispositivo;
//! handle 1 = canal de pedidos com o protocolo **tipado** `nexo.block` v1.0 (gerado de
//! `idl/block.idl`; cabeçalho NXIP): `read`, `write`, `capacity`, `identity`. Erros remotos:
//! 1 = pedido inválido, 2 = fora da capacidade, 3 = dados insuficientes, 4 = somente leitura,
//! `0x10|st` = erro do dispositivo VirtIO.
//! Argumento: `crash_after` — cai de propósito após esse número de pedidos (0 = nunca; testes).
#![no_std]
#![no_main]

use nexo_proto::block::{self, ReadResponse, Request, WriteResponse};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};
use nexo_virtio::{
    DESC_NEXT, DESC_WRITE, DmaPage, Error, MapBar, PciConfig, SplitQueue, Transport,
};

const DEV: Handle = 0;
const CHAN: Handle = 1;
const SECTOR: usize = 512;
const MAX_SECTORS: usize = 7; // 7 × 512 + cabeçalho + status cabem em uma mensagem de 4096 B
const QUEUE_SIZE: u16 = 64;

#[repr(C)]
struct BlkReq {
    kind: u32,
    reserved: u32,
    sector: u64,
}

struct Cfg(u16);
impl PciConfig for Cfg {
    fn read32(&mut self, offset: u16) -> u32 {
        nexo_sys::pci_cfg_read(DEV, self.0, offset).unwrap_or(0)
    }
    fn write32(&mut self, offset: u16, value: u32) {
        let _ = nexo_sys::pci_cfg_write(DEV, self.0, offset, value);
    }
}

struct Bars {
    info: PciInfo,
    mapped: [u64; 6],
}
impl MapBar for Bars {
    fn map(&mut self, bar: u8) -> Result<u64, Error> {
        let i = bar as usize;
        if i >= 6 || self.info.bars[i].size == 0 || self.info.bars[i].flags & 1 != 0 {
            return Err(Error::Map);
        }
        if self.mapped[i] == 0 {
            self.mapped[i] =
                nexo_sys::mmio_map(DEV, self.info.bars[i].base, self.info.bars[i].size)
                    .map_err(|_| Error::Map)?;
        }
        Ok(self.mapped[i])
    }
}

struct Device {
    t: Transport,
    q: SplitQueue,
    hdr: DmaPage,
    data: DmaPage,
    status: DmaPage,
    irq: Option<nexo_sys::abi::IrqInfo>,
    irq_seen: u64,
    capacity: u64,
    read_only: bool,
    serial: [u8; 20],
}

/// `VIRTIO_BLK_F_RO`.
const F_RO: u32 = 1 << 5;
/// Pedido `GET_ID` (serial de 20 bytes).
const T_GET_ID: u32 = 8;

fn fail(code: i64, what: &str) -> ! {
    log!("blockdev: falha: {}", what);
    nexo_sys::exit(code)
}

fn dma() -> DmaPage {
    let b = nexo_sys::dma_alloc(DEV).unwrap_or_else(|_| fail(18, "dma_alloc"));
    DmaPage {
        virt: b.virt,
        phys: b.phys,
    }
}

fn find_virtio_blk() -> PciInfo {
    let mut devs = [PciInfo::default(); 32];
    let n = nexo_sys::pci_enum(DEV, &mut devs)
        .unwrap_or_else(|_| fail(10, "pci_enum"))
        .min(32);
    *devs[..n]
        .iter()
        .find(|d| {
            d.is_virtio() && nexo_virtio::device_type(d.device) == Some(nexo_virtio::TYPE_BLOCK)
        })
        .unwrap_or_else(|| fail(11, "virtio-blk nao encontrado na concessao"))
}

fn setup(info: &PciInfo) -> Device {
    let mut cfg = Cfg(info.bdf);
    // Command: memória + bus master, INTx desabilitado.
    let cmd = cfg.read32(4);
    cfg.write32(4, cmd | 0x6 | (1 << 10));
    let caps = nexo_virtio::parse_caps(&mut cfg);
    let mut bars = Bars {
        info: *info,
        mapped: [0; 6],
    };
    let t = Transport::new(&caps, &mut bars).unwrap_or_else(|e| {
        log!("blockdev: transporte: {:?}", e);
        nexo_sys::exit(14)
    });
    if t.device.is_none() {
        fail(14, "configuracao do dispositivo ausente");
    }
    t.reset();
    let (accepted_lo, _) = t.negotiate(F_RO, 0).unwrap_or_else(|e| {
        log!("blockdev: features: {:?}", e);
        nexo_sys::exit(15)
    });
    let irq = nexo_sys::irq_alloc(DEV).ok();
    let irq = match irq {
        Some(i)
            if t.setup_msix(&mut cfg, &caps, &mut bars, i.msi_address, i.msi_data)
                .is_ok() =>
        {
            Some(i)
        }
        _ => None,
    };
    let (desc, avail, used) = (dma(), dma(), dma());
    let vector = if irq.is_some() {
        0
    } else {
        nexo_virtio::NO_VECTOR
    };
    let (size, notify_off) = t
        .setup_queue(0, QUEUE_SIZE, desc.phys, avail.phys, used.phys, vector)
        .unwrap_or_else(|_| fail(17, "fila 0"));
    let q = SplitQueue::new(0, size, notify_off, desc, avail, used);
    t.driver_ok();
    let dcfg = t.device.unwrap();
    let capacity = dcfg.r32(0) as u64 | ((dcfg.r32(4) as u64) << 32);
    let mut dev = Device {
        t,
        q,
        hdr: dma(),
        data: dma(),
        status: dma(),
        irq,
        irq_seen: 0,
        capacity,
        read_only: accepted_lo & F_RO != 0,
        serial: [0; 20],
    };
    // Serial (GET_ID): 20 bytes no buffer de dados; erro nao e fatal.
    if dev.raw_request(T_GET_ID, 0, 20).is_ok() {
        // SAFETY: página de DMA exclusiva; 20 bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(dev.data.virt as *const u8, dev.serial.as_mut_ptr(), 20)
        };
    }
    dev
}

impl Device {
    /// Executa um pedido de `count` setores a partir de `sector` (dados em `self.data`).
    fn request(&mut self, write: bool, sector: u64, count: usize) -> Result<(), u8> {
        self.raw_request(write as u32, sector, (count * SECTOR) as u32)
    }

    /// Pedido cru: tipo `kind`, `len` bytes de dados (escritos pelo dispositivo, salvo OUT).
    fn raw_request(&mut self, kind: u32, sector: u64, len: u32) -> Result<(), u8> {
        let write = kind == 1;
        // SAFETY: páginas de DMA exclusivas deste driver.
        unsafe {
            (self.hdr.virt as *mut BlkReq).write_volatile(BlkReq {
                kind,
                reserved: 0,
                sector,
            });
            (self.status.virt as *mut u8).write_volatile(0xff);
        }
        self.q.set_desc(0, self.hdr.phys, 16, DESC_NEXT, 1);
        self.q.set_desc(
            1,
            self.data.phys,
            len,
            DESC_NEXT | if write { 0 } else { DESC_WRITE },
            2,
        );
        self.q.set_desc(2, self.status.phys, 1, DESC_WRITE, 0);
        self.q.submit(&self.t, 0);
        let mut spins = 0u64;
        while self.q.pop_used().is_none() {
            if let Some(irq) = &self.irq {
                if let Ok(c) = nexo_sys::irq_wait(DEV, irq.vector, self.irq_seen) {
                    self.irq_seen = c;
                }
            } else {
                nexo_sys::yield_now();
            }
            spins += 1;
            if spins > 2_000_000 {
                return Err(0xfe);
            }
        }
        let _ = self.t.isr_ack();
        // SAFETY: página de DMA exclusiva.
        let st = unsafe { (self.status.virt as *const u8).read_volatile() };
        if st == 0 { Ok(()) } else { Err(st) }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(crash_after: u64) -> ! {
    let pci = find_virtio_blk();
    let mut dev = setup(&pci);
    let serial_len = dev.serial.iter().position(|&b| b == 0).unwrap_or(20);
    log!(
        "blockdev: virtio-blk {:04x}:{:04x} bdf {:#06x} capacidade {} setores, fila {}, {}{}, serial '{}'",
        pci.vendor,
        pci.device,
        pci.bdf,
        dev.capacity,
        dev.q.size(),
        if dev.irq.is_some() {
            "MSI-X"
        } else {
            "polling"
        },
        if dev.read_only {
            ", somente leitura"
        } else {
            ""
        },
        core::str::from_utf8(&dev.serial[..serial_len]).unwrap_or("?")
    );
    let mut buf = [0u8; 4096];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut served = 0u64;
    loop {
        let (n, _) = match nexo_sys::channel_recv(CHAN, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => {
                log!("blockdev: canal fechado apos {} pedidos", served);
                nexo_sys::exit(0)
            }
            Err(_) => fail(20, "recv"),
        };
        let request = match block::decode_request(&buf[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = block::encode_error(0, 1, &mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
                continue;
            }
        };
        served += 1;
        if crash_after != 0 && served > crash_after {
            log!("blockdev: caindo de proposito no pedido {}", served);
            // SAFETY: deliberadamente inválido — o kernel encerra este processo.
            let v = unsafe { core::ptr::read_volatile(core::ptr::dangling::<u64>()) };
            nexo_sys::exit(21 + (v & 1) as i64)
        }
        match request {
            Request::Capacity(_) => {
                let m = block::CapacityResponse {
                    sectors: dev.capacity,
                }
                .encode_msg(&mut reply)
                .unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
            Request::Identity(_) => {
                let mut r = block::IdentityResponse {
                    read_only: dev.read_only as u8,
                    serial: [0; 20],
                    serial_len: 20,
                };
                r.serial.copy_from_slice(&dev.serial);
                let m = r.encode_msg(&mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
            Request::Read(rq) => {
                let count = rq.count as usize;
                let m = if count == 0
                    || count > MAX_SECTORS
                    || rq.sector + rq.count as u64 > dev.capacity
                {
                    block::encode_error(block::ReadRequest::METHOD_ID, 2, &mut reply).unwrap_or(0)
                } else {
                    match dev.request(false, rq.sector, count) {
                        Ok(()) => {
                            let bytes = count * SECTOR;
                            let mut r = ReadResponse {
                                data: [0; 3584],
                                data_len: bytes as u32,
                            };
                            // SAFETY: página de DMA exclusiva; `bytes <= 3584`.
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    dev.data.virt as *const u8,
                                    r.data.as_mut_ptr(),
                                    bytes,
                                )
                            };
                            r.encode_msg(&mut reply).unwrap_or(0)
                        }
                        Err(st) => block::encode_error(
                            block::ReadRequest::METHOD_ID,
                            0x10 | (st & 0xf) as u32,
                            &mut reply,
                        )
                        .unwrap_or(0),
                    }
                };
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
            Request::Write(rq) => {
                let count = rq.count as usize;
                let bytes = count * SECTOR;
                let m = if dev.read_only {
                    block::encode_error(block::WriteRequest::METHOD_ID, 4, &mut reply).unwrap_or(0)
                } else if count == 0
                    || count > MAX_SECTORS
                    || rq.sector + rq.count as u64 > dev.capacity
                {
                    block::encode_error(block::WriteRequest::METHOD_ID, 2, &mut reply).unwrap_or(0)
                } else if (rq.data_len as usize) < bytes {
                    block::encode_error(block::WriteRequest::METHOD_ID, 3, &mut reply).unwrap_or(0)
                } else {
                    // SAFETY: página de DMA exclusiva; `bytes <= 3584`.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            rq.data.as_ptr(),
                            dev.data.virt as *mut u8,
                            bytes,
                        )
                    };
                    match dev.request(true, rq.sector, count) {
                        Ok(()) => WriteResponse {}.encode_msg(&mut reply).unwrap_or(0),
                        Err(st) => block::encode_error(
                            block::WriteRequest::METHOD_ID,
                            0x10 | (st & 0xf) as u32,
                            &mut reply,
                        )
                        .unwrap_or(0),
                    }
                };
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
        }
    }
}

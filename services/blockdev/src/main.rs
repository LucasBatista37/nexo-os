//! `blockdev` — driver VirtIO-block em modo usuário (VirtIO 1.x sobre PCI, interface moderna,
//! fila dividida, MSI-X; transporte em `nexo-virtio`). Handle 0 = concessão de dispositivo;
//! handle 1 = canal de pedidos. Protocolo provisório `nexo.block` v0 (`docs/spec/ipc-compat.md` §5):
//! pedido `[op u8][pad 3][setor u64][n u32][dados…]` com `op` 0 = ler, 1 = escrever,
//! 2 = capacidade; resposta `[status u8][dados…]`.
//! Argumento: `crash_after` — cai de propósito após esse número de pedidos (0 = nunca; testes).
#![no_std]
#![no_main]

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
        if n < 16 {
            let _ = nexo_sys::channel_send(CHAN, &[1u8], &[]);
            continue;
        }
        let op = buf[0];
        if op == 2 {
            let mut r = [0u8; 9];
            r[1..9].copy_from_slice(&dev.capacity.to_le_bytes());
            let _ = nexo_sys::channel_send(CHAN, &r, &[]);
            served += 1;
            continue;
        }
        if op == 3 {
            let mut r = [0u8; 22];
            r[1] = dev.read_only as u8;
            r[2..22].copy_from_slice(&dev.serial);
            let _ = nexo_sys::channel_send(CHAN, &r, &[]);
            served += 1;
            continue;
        }
        if op == 1 && dev.read_only {
            let _ = nexo_sys::channel_send(CHAN, &[4u8], &[]);
            continue;
        }
        let sector = u64::from_le_bytes(buf[4..12].try_into().unwrap());
        let count = u32::from_le_bytes(buf[12..16].try_into().unwrap()) as usize;
        if count == 0 || count > MAX_SECTORS || sector + count as u64 > dev.capacity {
            let _ = nexo_sys::channel_send(CHAN, &[2u8], &[]);
            continue;
        }
        if crash_after != 0 && served >= crash_after {
            log!("blockdev: caindo de proposito no pedido {}", served + 1);
            // SAFETY: deliberadamente inválido — o kernel encerra este processo.
            let v = unsafe { core::ptr::read_volatile(core::ptr::dangling::<u64>()) };
            nexo_sys::exit(21 + (v & 1) as i64)
        }
        let bytes = count * SECTOR;
        let result = if op == 1 {
            if n < 16 + bytes {
                let _ = nexo_sys::channel_send(CHAN, &[3u8], &[]);
                continue;
            }
            // SAFETY: página de DMA exclusiva; `bytes <= 3584`.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    buf[16..16 + bytes].as_ptr(),
                    dev.data.virt as *mut u8,
                    bytes,
                )
            };
            dev.request(true, sector, count)
        } else {
            dev.request(false, sector, count)
        };
        let mut reply = [0u8; 4096];
        match result {
            Ok(()) => {
                reply[0] = 0;
                let len = if op == 0 {
                    // SAFETY: página de DMA exclusiva; `bytes <= 3584`.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            dev.data.virt as *const u8,
                            reply[1..].as_mut_ptr(),
                            bytes,
                        )
                    };
                    1 + bytes
                } else {
                    1
                };
                let _ = nexo_sys::channel_send(CHAN, &reply[..len], &[]);
            }
            Err(st) => {
                reply[0] = 0x10 | (st & 0xf);
                let _ = nexo_sys::channel_send(CHAN, &reply[..1], &[]);
            }
        }
        served += 1;
    }
}

//! `inputdev` — driver VirtIO-input (teclado/mouse). Handle 0 = concessão do dispositivo,
//! handle 1 = canal. Fila 0 = eventos (`virtio_input_event`: `[type u16][code u16][value u32]`).
//! Protocolo cru `nexo.input` v0: pedido `[0]` = ler eventos disponíveis (resposta
//! `[0][evento 8 B]…`, possivelmente vazia, sem bloquear).
#![no_std]
#![no_main]

use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};
use nexo_virtio::{DESC_WRITE, DmaPage, Error, MapBar, PciConfig, SplitQueue, Transport};

const DEV: Handle = 0;
const CHAN: Handle = 1;
/// Tipo VirtIO: dispositivo de entrada.
const TYPE_INPUT: u16 = 18;
const EVENT_SIZE: u64 = 8;
const EVENT_BUFS: u16 = 32;

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

fn fail(code: i64, what: &str) -> ! {
    log!("inputdev: falha: {}", what);
    nexo_sys::exit(code)
}

fn dma() -> DmaPage {
    let b = nexo_sys::dma_alloc(DEV).unwrap_or_else(|_| fail(80, "dma_alloc"));
    DmaPage {
        virt: b.virt,
        phys: b.phys,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut devs = [PciInfo::default(); 8];
    let n = nexo_sys::pci_enum(DEV, &mut devs)
        .unwrap_or_else(|_| fail(81, "pci_enum"))
        .min(8);
    let info = *devs[..n]
        .iter()
        .find(|d| d.is_virtio() && nexo_virtio::device_type(d.device) == Some(TYPE_INPUT))
        .unwrap_or_else(|| fail(82, "virtio-input nao encontrado na concessao"));
    let mut cfg = Cfg(info.bdf);
    let cmd = cfg.read32(4);
    cfg.write32(4, cmd | 0x6 | (1 << 10));
    let caps = nexo_virtio::parse_caps(&mut cfg);
    let mut bars = Bars {
        info,
        mapped: [0; 6],
    };
    let t = Transport::new(&caps, &mut bars).unwrap_or_else(|e| {
        log!("inputdev: transporte: {:?}", e);
        nexo_sys::exit(83)
    });
    t.reset();
    t.negotiate(0, 0).unwrap_or_else(|e| {
        log!("inputdev: features: {:?}", e);
        nexo_sys::exit(84)
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
    let vector = if irq.is_some() {
        0
    } else {
        nexo_virtio::NO_VECTOR
    };
    let (eqd, eqa, equ) = (dma(), dma(), dma());
    let (size, notify) = t
        .setup_queue(0, EVENT_BUFS, eqd.phys, eqa.phys, equ.phys, vector)
        .unwrap_or_else(|_| fail(85, "fila de eventos"));
    let mut q = SplitQueue::new(0, size, notify, eqd, eqa, equ);
    t.driver_ok();
    // Um buffer de 8 bytes por descritor, todos na mesma página de DMA.
    let pool = dma();
    let nbufs = size.min(EVENT_BUFS).min((4096 / EVENT_SIZE) as u16);
    for i in 0..nbufs {
        q.set_desc(
            i,
            pool.phys + i as u64 * EVENT_SIZE,
            EVENT_SIZE as u32,
            DESC_WRITE,
            0,
        );
        q.submit(&t, i);
    }
    log!(
        "inputdev: virtio-input bdf {:#06x} pronto (fila {}, {} buffers, {})",
        info.bdf,
        size,
        nbufs,
        if irq.is_some() { "MSI-X" } else { "polling" }
    );
    let mut buf = [0u8; 64];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    loop {
        let (n, _) = match nexo_sys::channel_recv(CHAN, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(86, "recv"),
        };
        if n == 0 || buf[0] != 0 {
            let _ = nexo_sys::channel_send(CHAN, &[1u8], &[]);
            continue;
        }
        let mut out = 1usize;
        while let Some((id, len)) = q.pop_used() {
            let id = id as u16;
            if id < nbufs && len >= EVENT_SIZE as u32 && out + 8 <= reply.len() {
                // SAFETY: página de DMA exclusiva; o evento tem 8 bytes dentro da página.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        (pool.virt + id as u64 * EVENT_SIZE) as *const u8,
                        reply[out..].as_mut_ptr(),
                        8,
                    )
                };
                out += 8;
            }
            if id < nbufs {
                q.set_desc(
                    id,
                    pool.phys + id as u64 * EVENT_SIZE,
                    EVENT_SIZE as u32,
                    DESC_WRITE,
                    0,
                );
                q.submit(&t, id);
            }
        }
        let _ = t.isr_ack();
        reply[0] = 0;
        let _ = nexo_sys::channel_send(CHAN, &reply[..out], &[]);
    }
}

//! `rngdev` — driver VirtIO-RNG em modo usuário. Handle 0 = concessão do dispositivo
//! (restrita à função), handle 1 = canal de pedidos com o protocolo **tipado** `nexo.rng` v1.0
//! (gerado de `idl/rng.idl`; cabeçalho NXIP, `docs/spec/ipc-compat.md` §2). Erros: código 1 =
//! pedido inválido, 2 = dispositivo não respondeu, 3 = mensagem malformada.
#![no_std]
#![no_main]

use nexo_proto::rng::{self, FillRequest, FillResponse, Request};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};
use nexo_virtio::{DESC_WRITE, DmaPage, Error, MapBar, PciConfig, SplitQueue, Transport};

const DEV: Handle = 0;
const CHAN: Handle = 1;
const MAX_REQUEST: usize = 1024;

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
    log!("rngdev: falha: {}", what);
    nexo_sys::exit(code)
}

fn dma() -> DmaPage {
    let b = nexo_sys::dma_alloc(DEV).unwrap_or_else(|_| fail(18, "dma_alloc"));
    DmaPage {
        virt: b.virt,
        phys: b.phys,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut devs = [PciInfo::default(); 4];
    let n = nexo_sys::pci_enum(DEV, &mut devs)
        .unwrap_or_else(|_| fail(10, "pci_enum"))
        .min(4);
    let info = *devs[..n]
        .iter()
        .find(|d| {
            d.is_virtio() && nexo_virtio::device_type(d.device) == Some(nexo_virtio::TYPE_RNG)
        })
        .unwrap_or_else(|| fail(11, "virtio-rng nao encontrado na concessao"));
    let mut cfg = Cfg(info.bdf);
    let cmd = cfg.read32(4);
    cfg.write32(4, cmd | 0x6 | (1 << 10));
    let caps = nexo_virtio::parse_caps(&mut cfg);
    let mut bars = Bars {
        info,
        mapped: [0; 6],
    };
    let t = Transport::new(&caps, &mut bars).unwrap_or_else(|e| {
        log!("rngdev: transporte: {:?}", e);
        nexo_sys::exit(14)
    });
    t.reset();
    t.negotiate(0, 0).unwrap_or_else(|e| {
        log!("rngdev: features: {:?}", e);
        nexo_sys::exit(15)
    });
    let irq = nexo_sys::irq_alloc(DEV).ok();
    let msix = match irq {
        Some(i)
            if t.setup_msix(&mut cfg, &caps, &mut bars, i.msi_address, i.msi_data)
                .is_ok() =>
        {
            Some(i)
        }
        _ => None,
    };
    let (desc, avail, used, buf) = (dma(), dma(), dma(), dma());
    let (size, notify_off) = t
        .setup_queue(
            0,
            8,
            desc.phys,
            avail.phys,
            used.phys,
            if msix.is_some() {
                0
            } else {
                nexo_virtio::NO_VECTOR
            },
        )
        .unwrap_or_else(|_| fail(17, "fila 0"));
    let mut q = SplitQueue::new(0, size, notify_off, desc, avail, used);
    t.driver_ok();
    log!(
        "rngdev: virtio-rng bdf {:#06x} pronto (fila {}, {})",
        info.bdf,
        size,
        if msix.is_some() { "MSI-X" } else { "polling" }
    );
    let mut req = [0u8; 64];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut seen = 0u64;
    loop {
        let (n, _) = match nexo_sys::channel_recv(CHAN, &mut req, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(20, "recv"),
        };
        let want = match rng::decode_request(&req[..n]) {
            Ok(Request::Fill(f)) => f.len as usize,
            Err(_) => {
                let m = rng::encode_error(FillRequest::METHOD_ID, 3, &mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
                continue;
            }
        };
        if want == 0 || want > MAX_REQUEST {
            let m = rng::encode_error(FillRequest::METHOD_ID, 1, &mut reply).unwrap_or(0);
            let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            continue;
        }
        q.set_desc(0, buf.phys, want as u32, DESC_WRITE, 0);
        q.submit(&t, 0);
        let mut got = None;
        let mut spins = 0u32;
        while got.is_none() {
            got = q.pop_used();
            if got.is_none() {
                if let Some(i) = &msix {
                    if let Ok(c) = nexo_sys::irq_wait(DEV, i.vector, seen) {
                        seen = c;
                    }
                } else {
                    nexo_sys::yield_now();
                }
                spins += 1;
                if spins > 1_000_000 {
                    break;
                }
            }
        }
        let _ = t.isr_ack();
        match got {
            Some((_, len)) => {
                let len = (len as usize).min(want);
                let mut resp = FillResponse {
                    data: [0; 1024],
                    data_len: len as u32,
                };
                // SAFETY: página de DMA exclusiva; `len ≤ 1024`.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        buf.virt as *const u8,
                        resp.data.as_mut_ptr(),
                        len,
                    )
                };
                let m = resp.encode_msg(&mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
            None => {
                let m = rng::encode_error(FillRequest::METHOD_ID, 2, &mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
        }
    }
}

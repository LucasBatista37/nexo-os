//! `consoledev` — driver VirtIO-console (porta 0, sem `MULTIPORT`): fila 0 = recepção
//! (host→guest), fila 1 = transmissão. Handle 0 = concessão do dispositivo, handle 1 = canal.
//! Protocolo **tipado** `nexo.console` v1.0 (gerado de `idl/console.idl`; cabeçalho NXIP):
//! `read` (o que houver, sem bloquear) e `write`. Erro remoto 1 = pedido inválido.
#![no_std]
#![no_main]

use nexo_proto::console::{self, ReadResponse, Request, WriteResponse};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};
use nexo_virtio::{DESC_WRITE, DmaPage, Error, MapBar, PciConfig, SplitQueue, Transport};

const DEV: Handle = 0;
const CHAN: Handle = 1;
const RX_BUFS: u16 = 4;
/// Tipo VirtIO: console.
const TYPE_CONSOLE: u16 = 3;

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
    log!("consoledev: falha: {}", what);
    nexo_sys::exit(code)
}

fn dma() -> DmaPage {
    let b = nexo_sys::dma_alloc(DEV).unwrap_or_else(|_| fail(60, "dma_alloc"));
    DmaPage {
        virt: b.virt,
        phys: b.phys,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut devs = [PciInfo::default(); 8];
    let n = nexo_sys::pci_enum(DEV, &mut devs)
        .unwrap_or_else(|_| fail(61, "pci_enum"))
        .min(8);
    let info = *devs[..n]
        .iter()
        .find(|d| d.is_virtio() && nexo_virtio::device_type(d.device) == Some(TYPE_CONSOLE))
        .unwrap_or_else(|| fail(62, "virtio-console nao encontrado na concessao"));
    let mut cfg = Cfg(info.bdf);
    let cmd = cfg.read32(4);
    cfg.write32(4, cmd | 0x6 | (1 << 10));
    let caps = nexo_virtio::parse_caps(&mut cfg);
    let mut bars = Bars {
        info,
        mapped: [0; 6],
    };
    let t = Transport::new(&caps, &mut bars).unwrap_or_else(|e| {
        log!("consoledev: transporte: {:?}", e);
        nexo_sys::exit(63)
    });
    t.reset();
    // Sem MULTIPORT: filas 0/1 pertencem a porta 0.
    t.negotiate(0, 0).unwrap_or_else(|e| {
        log!("consoledev: features: {:?}", e);
        nexo_sys::exit(64)
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
    let (rxd, rxa, rxu) = (dma(), dma(), dma());
    let (size, notify) = t
        .setup_queue(0, RX_BUFS, rxd.phys, rxa.phys, rxu.phys, vector)
        .unwrap_or_else(|_| fail(65, "fila rx"));
    let mut rx = SplitQueue::new(0, size, notify, rxd, rxa, rxu);
    let (txd, txa, txu) = (dma(), dma(), dma());
    let (size, notify) = t
        .setup_queue(1, 4, txd.phys, txa.phys, txu.phys, vector)
        .unwrap_or_else(|_| fail(66, "fila tx"));
    let mut tx = SplitQueue::new(1, size, notify, txd, txa, txu);
    t.driver_ok();
    // Buffers de recepcao pre-postados (1 pagina cada) e um de transmissao.
    let mut rx_bufs = [DmaPage::default(); RX_BUFS as usize];
    for (i, b) in rx_bufs.iter_mut().enumerate() {
        *b = dma();
        rx.set_desc(i as u16, b.phys, 4096, DESC_WRITE, 0);
        rx.submit(&t, i as u16);
    }
    let tx_buf = dma();
    log!(
        "consoledev: virtio-console bdf {:#06x} pronto (rx {}, tx {}, {})",
        info.bdf,
        rx.size(),
        tx.size(),
        if irq.is_some() { "MSI-X" } else { "polling" }
    );
    let mut buf = [0u8; 4096];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut irq_seen = 0u64;
    loop {
        let (n, _) = match nexo_sys::channel_recv(CHAN, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(67, "recv"),
        };
        let request = match console::decode_request(&buf[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = console::encode_error(0, 1, &mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
                continue;
            }
        };
        match request {
            Request::Read(_) => {
                // entrega tudo o que a recepcao tiver (sem bloquear)
                let mut resp = ReadResponse {
                    data: [0; 3500],
                    data_len: 0,
                };
                let mut out = 0usize;
                while let Some((id, len)) = rx.pop_used() {
                    let id = id as usize;
                    let len = (len as usize).min(4096);
                    if id < rx_bufs.len() && out + len <= resp.data.len() {
                        // SAFETY: pagina de DMA exclusiva; len <= 4096 e cabe em resp.data.
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                rx_bufs[id].virt as *const u8,
                                resp.data[out..].as_mut_ptr(),
                                len,
                            )
                        };
                        out += len;
                    }
                    if id < rx_bufs.len() {
                        rx.set_desc(id as u16, rx_bufs[id].phys, 4096, DESC_WRITE, 0);
                        rx.submit(&t, id as u16);
                    }
                }
                let _ = t.isr_ack();
                resp.data_len = out as u32;
                let m = resp.encode_msg(&mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
            Request::Write(w) => {
                let len = w.data().len().min(4096);
                let data = &w.data()[..len];
                // SAFETY: pagina de DMA exclusiva; len <= 4096.
                unsafe {
                    core::ptr::copy_nonoverlapping(data.as_ptr(), tx_buf.virt as *mut u8, len)
                };
                tx.set_desc(0, tx_buf.phys, len as u32, 0, 0);
                tx.submit(&t, 0);
                let mut spins = 0u32;
                while tx.pop_used().is_none() {
                    if let Some(i) = &irq {
                        if let Ok(c) = nexo_sys::irq_wait(DEV, i.vector, irq_seen) {
                            irq_seen = c;
                        }
                    } else {
                        nexo_sys::yield_now();
                    }
                    spins += 1;
                    if spins > 1_000_000 {
                        break;
                    }
                }
                let _ = t.isr_ack();
                let m = WriteResponse {
                    written: len as u32,
                }
                .encode_msg(&mut reply)
                .unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
        }
    }
}

//! `inputdev` — driver VirtIO-input (teclado/mouse). Handle 0 = concessão do dispositivo,
//! handle 1 = canal. Fila 0 = eventos (`virtio_input_event`: `[type u16][code u16][value u32]`).
//! Protocolo **tipado** `nexo.input` v1.0 (gerado de `idl/input.idl`; cabeçalho NXIP):
//! `poll` devolve os eventos disponíveis (8 B cada, formato evdev), sem bloquear.
//! `subscribe{chan}` liga o modo de eventos: o driver passa a **empurrar** cada lote de eventos
//! crus no canal transferido (guiado por interrupção via `irq_channel`); a outra ponta pode ir
//! direto ao compositor (`nexo.wm set_input`), que lê o mesmo formato.
#![no_std]
#![no_main]

use nexo_proto::input::{self, PollResponse, Request};
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
    // Canal de interrupções (IRQ→canal): permite dormir em wait_any({cliente, IRQ}) e empurrar
    // eventos no modo `subscribe` sem varredura ociosa.
    let irq_chan = irq
        .as_ref()
        .and_then(|i| nexo_sys::irq_channel(DEV, i.vector).ok());

    // Drena a fila de eventos para `batch` (8 bytes por evento) e ressubmete os buffers.
    let drain = |q: &mut SplitQueue, batch: &mut [u8]| -> usize {
        let mut out = 0usize;
        while let Some((id, len)) = q.pop_used() {
            let id = id as u16;
            if id < nbufs && len >= EVENT_SIZE as u32 && out + 8 <= batch.len() {
                // SAFETY: página de DMA exclusiva; o evento tem 8 bytes dentro da página.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        (pool.virt + id as u64 * EVENT_SIZE) as *const u8,
                        batch[out..].as_mut_ptr(),
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
        out
    };

    let mut push: Option<nexo_sys::Handle> = None;
    let mut buf = [0u8; 64];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut batch = [0u8; 3500];
    loop {
        let mut worked = false;
        // Pedidos do cliente (poll / subscribe).
        match nexo_sys::channel_try_recv(CHAN, &mut buf, &mut hs) {
            Ok((n, nh)) => {
                worked = true;
                match input::decode_request_with_handles(&buf[..n], &hs[..nh]) {
                    Ok(Request::Subscribe(rq)) => {
                        if let Some(old) = push.replace(rq.chan) {
                            let _ = nexo_sys::handle_close(old);
                        }
                        let m = input::SubscribeResponse {}
                            .encode_msg(&mut reply)
                            .unwrap_or(0);
                        let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
                    }
                    Ok(Request::Poll(_)) => {
                        let mut resp = PollResponse {
                            events: [0; 3500],
                            events_len: 0,
                        };
                        // No modo assinado os eventos vao para o canal; poll devolve vazio.
                        if push.is_none() {
                            resp.events_len = drain(&mut q, &mut resp.events) as u32;
                        }
                        let m = resp.encode_msg(&mut reply).unwrap_or(0);
                        let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
                    }
                    Err(_) => {
                        let m = input::encode_error(0, 1, &mut reply).unwrap_or(0);
                        let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
                    }
                }
            }
            Err(Status::WouldBlock) => {}
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(86, "recv"),
        }
        // Aviso de interrupção: drena e, se assinado, empurra o lote cru.
        if let Some(ic) = irq_chan {
            match nexo_sys::channel_try_recv(ic, &mut buf, &mut hs) {
                Ok(_) => {
                    worked = true;
                    // Sem assinante, os eventos ficam na fila para o `poll` (só consome o aviso).
                    if push.is_some() {
                        let n = drain(&mut q, &mut batch);
                        if n > 0
                            && let Some(pc) = push
                            && nexo_sys::channel_send(pc, &batch[..n], &[]) == Status::PeerClosed
                        {
                            let _ = nexo_sys::handle_close(pc);
                            push = None;
                        }
                    }
                }
                Err(Status::WouldBlock) => {}
                Err(_) => fail(87, "recv irq"),
            }
        }
        if !worked {
            match irq_chan {
                Some(ic) => {
                    let _ = nexo_sys::channel_wait_any(&[CHAN, ic]);
                }
                None => {
                    // sem MSI-X: varre com um cochilo curto
                    nexo_sys::sleep_ns(2_000_000);
                }
            }
        }
    }
}

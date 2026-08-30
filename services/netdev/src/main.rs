//! `netdev` — driver VirtIO-net em modo usuário (VirtIO 1.x, filas 0 = recepção e
//! 1 = transmissão, cabeçalho `virtio_net_hdr` de 12 bytes, MAC da configuração do
//! dispositivo). Handle 0 = concessão do dispositivo, handle 1 = canal com o protocolo
//! tipado `nexo.net` v1.0 (`idl/net.idl`): `mac`, `send`, `recv` (sem bloquear).
#![no_std]
#![no_main]

use nexo_proto::net::{self, MacResponse, RecvResponse, Request, SendResponse};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};
use nexo_virtio::{DESC_WRITE, DmaPage, Error, MapBar, PciConfig, SplitQueue, Transport};

const DEV: Handle = 0;
const CHAN: Handle = 1;
/// Tipo VirtIO: placa de rede.
const TYPE_NET: u16 = 1;
/// `VIRTIO_NET_F_MAC`.
const F_MAC: u32 = 1 << 5;
/// Cabeçalho `virtio_net_hdr` (sem MRG_RXBUF, com VERSION_1): 12 bytes.
const NET_HDR: usize = 12;
const RX_BUFS: u16 = 8;
const FRAME_MAX: usize = 1514;

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
    log!("netdev: falha: {}", what);
    nexo_sys::exit(code)
}

fn dma() -> DmaPage {
    let b = nexo_sys::dma_alloc(DEV).unwrap_or_else(|_| fail(90, "dma_alloc"));
    DmaPage {
        virt: b.virt,
        phys: b.phys,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut devs = [PciInfo::default(); 8];
    let n = nexo_sys::pci_enum(DEV, &mut devs)
        .unwrap_or_else(|_| fail(91, "pci_enum"))
        .min(8);
    let info = *devs[..n]
        .iter()
        .find(|d| d.is_virtio() && nexo_virtio::device_type(d.device) == Some(TYPE_NET))
        .unwrap_or_else(|| fail(92, "virtio-net nao encontrado na concessao"));
    let mut cfg = Cfg(info.bdf);
    let cmd = cfg.read32(4);
    cfg.write32(4, cmd | 0x6 | (1 << 10));
    let caps = nexo_virtio::parse_caps(&mut cfg);
    let mut bars = Bars {
        info,
        mapped: [0; 6],
    };
    let t = Transport::new(&caps, &mut bars).unwrap_or_else(|e| {
        log!("netdev: transporte: {:?}", e);
        nexo_sys::exit(93)
    });
    t.reset();
    let (accepted_lo, _) = t.negotiate(F_MAC, 0).unwrap_or_else(|e| {
        log!("netdev: features: {:?}", e);
        nexo_sys::exit(94)
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
        .unwrap_or_else(|_| fail(95, "fila rx"));
    let mut rx = SplitQueue::new(0, size, notify, rxd, rxa, rxu);
    let (txd, txa, txu) = (dma(), dma(), dma());
    let (size, notify) = t
        .setup_queue(1, 4, txd.phys, txa.phys, txu.phys, vector)
        .unwrap_or_else(|_| fail(96, "fila tx"));
    let mut tx = SplitQueue::new(1, size, notify, txd, txa, txu);
    t.driver_ok();
    // MAC: da configuracao do dispositivo (offset 0..6) se F_MAC foi aceita.
    let mut mac = [0u8; 6];
    if accepted_lo & F_MAC != 0
        && let Some(d) = t.device
    {
        for (i, b) in mac.iter_mut().enumerate() {
            *b = d.r8(i as u64);
        }
    }
    // Buffers de recepcao (1 pagina cada: cabecalho de 12 B + quadro) e um de transmissao.
    let mut rx_bufs = [DmaPage::default(); RX_BUFS as usize];
    for (i, b) in rx_bufs.iter_mut().enumerate() {
        *b = dma();
        rx.set_desc(i as u16, b.phys, 4096, DESC_WRITE, 0);
        rx.submit(&t, i as u16);
    }
    let tx_buf = dma();
    log!(
        "netdev: virtio-net bdf {:#06x} mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} (rx {}, tx {}, {})",
        info.bdf,
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5],
        rx.size(),
        tx.size(),
        if irq.is_some() { "MSI-X" } else { "polling" }
    );
    let mut buf = [0u8; 4096];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut irq_seen = 0u64;
    // Fila local de quadros recebidos (a recepcao anda mesmo sem pedido `recv` pendente).
    let mut rxq: [([u8; FRAME_MAX], usize); 8] = [([0; FRAME_MAX], 0); 8];
    let mut rxq_len = 0usize;
    loop {
        // Drena a virtqueue de recepcao para a fila local.
        while let Some((id, len)) = rx.pop_used() {
            let id = id as usize;
            if id < rx_bufs.len() {
                let flen = (len as usize).saturating_sub(NET_HDR).min(FRAME_MAX);
                if flen >= 14 && rxq_len < rxq.len() {
                    // SAFETY: pagina de DMA exclusiva; flen <= FRAME_MAX.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            (rx_bufs[id].virt + NET_HDR as u64) as *const u8,
                            rxq[rxq_len].0.as_mut_ptr(),
                            flen,
                        )
                    };
                    rxq[rxq_len].1 = flen;
                    rxq_len += 1;
                }
                rx.set_desc(id as u16, rx_bufs[id].phys, 4096, DESC_WRITE, 0);
                rx.submit(&t, id as u16);
            }
        }
        let _ = t.isr_ack();
        let (n, _) = match nexo_sys::channel_recv(CHAN, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(97, "recv"),
        };
        let request = match net::decode_request(&buf[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = net::encode_error(0, 1, &mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
                continue;
            }
        };
        match request {
            Request::Mac(_) => {
                let r = MacResponse {
                    addr: mac,
                    addr_len: 6,
                };
                let m = r.encode_msg(&mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
            Request::Send(rq) => {
                let frame = rq.frame();
                if frame.len() < 14 {
                    let m =
                        net::encode_error(net::SendRequest::METHOD_ID, 1, &mut reply).unwrap_or(0);
                    let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
                    continue;
                }
                // Cabecalho virtio_net_hdr zerado (sem offloads) + quadro.
                // SAFETY: pagina de DMA exclusiva; NET_HDR + frame.len() <= 4096.
                unsafe {
                    core::ptr::write_bytes(tx_buf.virt as *mut u8, 0, NET_HDR);
                    core::ptr::copy_nonoverlapping(
                        frame.as_ptr(),
                        (tx_buf.virt + NET_HDR as u64) as *mut u8,
                        frame.len(),
                    );
                }
                tx.set_desc(0, tx_buf.phys, (NET_HDR + frame.len()) as u32, 0, 0);
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
                let m = SendResponse {}.encode_msg(&mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
            Request::Recv(_) => {
                let mut r = RecvResponse {
                    frame: [0; FRAME_MAX],
                    frame_len: 0,
                };
                if rxq_len > 0 {
                    let (f, l) = rxq[0];
                    r.frame[..l].copy_from_slice(&f[..l]);
                    r.frame_len = l as u32;
                    rxq.copy_within(1..rxq_len, 0);
                    rxq_len -= 1;
                }
                let m = r.encode_msg(&mut reply).unwrap_or(0);
                let _ = nexo_sys::channel_send(CHAN, &reply[..m], &[]);
            }
        }
    }
}

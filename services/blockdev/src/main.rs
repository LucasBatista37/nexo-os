//! `blockdev` — driver VirtIO-block em modo usuário (VirtIO 1.x sobre PCI, interface moderna,
//! fila dividida, MSI-X; transporte em `nexo-virtio`). Handle 0 = concessão de dispositivo;
//! handle 1 = canal de pedidos com o protocolo **tipado** `nexo.block` v1.0 (gerado de
//! `idl/block.idl`; cabeçalho NXIP): `read`, `write`, `capacity`, `identity`. Erros remotos:
//! 1 = pedido inválido, 2 = fora da capacidade, 3 = dados insuficientes, 4 = somente leitura,
//! `0x10|st` = erro do dispositivo VirtIO.
//! Argumento: `crash_after` — cai de propósito após esse número de pedidos (0 = nunca; testes).
//!
//! **Fila assíncrona**: até [`SLOTS`] pedidos de E/S ficam em voo na virtqueue ao mesmo tempo
//! (o cliente pode encadear pedidos sem esperar as respostas); as respostas saem **na ordem de
//! chegada** dos pedidos, como o protocolo exige.
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

/// Pedidos de E/S simultâneos na virtqueue (3 descritores cada).
const SLOTS: usize = 4;

struct Device {
    t: Transport,
    q: SplitQueue,
    hdr: DmaPage,
    data: DmaPage,
    status: DmaPage,
    /// Páginas por slot da fila assíncrona: (cabeçalho+status, dados).
    slot_meta: [DmaPage; SLOTS],
    slot_data: [DmaPage; SLOTS],
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
        slot_meta: [dma(), dma(), dma(), dma()],
        slot_data: [dma(), dma(), dma(), dma()],
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

/// Pedido em voo (ou resposta pronta) na ordem de chegada.
enum Pending {
    /// E/S submetida no slot: (slot, é escrita, bytes).
    Io {
        slot: usize,
        write: bool,
        bytes: usize,
    },
    /// Resposta já codificada (capacity/identity/erro), aguardando a vez na ordem FIFO.
    Ready { len: usize },
}

impl Device {
    /// Submete um pedido de E/S no slot `slot` (descritores `3*slot..3*slot+3`).
    fn submit_slot(&mut self, slot: usize, write: bool, sector: u64, count: usize) {
        let meta = self.slot_meta[slot];
        let data = self.slot_data[slot];
        // SAFETY: páginas de DMA exclusivas do slot.
        unsafe {
            (meta.virt as *mut BlkReq).write_volatile(BlkReq {
                kind: write as u32,
                reserved: 0,
                sector,
            });
            ((meta.virt + 16) as *mut u8).write_volatile(0xff);
        }
        let base = (3 * slot) as u16;
        let len = (count * SECTOR) as u32;
        self.q.set_desc(base, meta.phys, 16, DESC_NEXT, base + 1);
        self.q.set_desc(
            base + 1,
            data.phys,
            len,
            DESC_NEXT | if write { 0 } else { DESC_WRITE },
            base + 2,
        );
        self.q.set_desc(base + 2, meta.phys + 16, 1, DESC_WRITE, 0);
        self.q.submit(&self.t, base);
    }

    /// Status VirtIO do slot (0 = ok; 0xff = ainda em execução).
    fn slot_status(&self, slot: usize) -> u8 {
        // SAFETY: página de DMA exclusiva do slot.
        unsafe { ((self.slot_meta[slot].virt + 16) as *const u8).read_volatile() }
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
    // Fila assincrona: pedidos na ordem de chegada; slots livres da virtqueue.
    let mut pending: [Option<Pending>; SLOTS] = [const { None }; SLOTS];
    let mut order: [usize; SLOTS] = [0; SLOTS]; // indices de `pending` na ordem FIFO
    let mut order_len = 0usize;
    let mut slot_free = [true; SLOTS];
    let mut done: [bool; SLOTS] = [false; SLOTS];
    let mut ready_buf = [[0u8; 4096]; SLOTS];
    let mut max_in_flight = 0usize;
    loop {
        // 1. Completa E/S terminadas (marca `done` pelo id do descritor-cabeca).
        while let Some((id, _len)) = dev.q.pop_used() {
            let slot = (id as usize) / 3;
            if slot < SLOTS {
                done[slot] = true;
            }
        }
        let _ = dev.t.isr_ack();
        // 2. Entrega respostas prontas na ordem de chegada.
        while order_len > 0 {
            let idx = order[0];
            let (len, pop) = match &pending[idx] {
                Some(Pending::Ready { len }) => (*len, true),
                Some(Pending::Io { slot, write, bytes }) if done[*slot] => {
                    let st = dev.slot_status(*slot);
                    let len = if st != 0 {
                        let method = if *write {
                            block::WriteRequest::METHOD_ID
                        } else {
                            block::ReadRequest::METHOD_ID
                        };
                        block::encode_error(method, 0x10 | (st & 0xf) as u32, &mut reply)
                            .unwrap_or(0)
                    } else if *write {
                        WriteResponse {}.encode_msg(&mut reply).unwrap_or(0)
                    } else {
                        let mut r = ReadResponse {
                            data: [0; 3584],
                            data_len: *bytes as u32,
                        };
                        // SAFETY: página de DMA exclusiva do slot; `bytes <= 3584`.
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                dev.slot_data[*slot].virt as *const u8,
                                r.data.as_mut_ptr(),
                                *bytes,
                            )
                        };
                        r.encode_msg(&mut reply).unwrap_or(0)
                    };
                    done[*slot] = false;
                    slot_free[*slot] = true;
                    (len, true)
                }
                _ => (0, false),
            };
            if !pop {
                break;
            }
            let out: &[u8] = if matches!(pending[idx], Some(Pending::Ready { .. })) {
                &ready_buf[idx][..len]
            } else {
                &reply[..len]
            };
            let _ = nexo_sys::channel_send(CHAN, out, &[]);
            pending[idx] = None;
            order.copy_within(1..order_len, 0);
            order_len -= 1;
        }
        // 3. Recebe pedidos: bloqueia so quando nao ha nada em voo.
        let in_flight = order_len > 0;
        let r = if in_flight {
            nexo_sys::channel_try_recv(CHAN, &mut buf, &mut hs)
        } else {
            nexo_sys::channel_recv(CHAN, &mut buf, &mut hs)
        };
        let n = match r {
            Ok((n, _)) => n,
            Err(Status::WouldBlock) => {
                // Espera E/S andar (MSI-X quando ha, senao yield).
                if let Some(i) = &dev.irq {
                    if let Ok(c) = nexo_sys::irq_wait(DEV, i.vector, dev.irq_seen) {
                        dev.irq_seen = c;
                    }
                } else {
                    nexo_sys::yield_now();
                }
                continue;
            }
            Err(Status::PeerClosed) => {
                log!(
                    "blockdev: canal fechado apos {} pedidos (max {} em voo)",
                    served,
                    max_in_flight
                );
                nexo_sys::exit(0)
            }
            Err(_) => fail(20, "recv"),
        };
        // Sem indice livre na fila FIFO: processa sincronamente na proxima volta.
        let Some(free_idx) = (0..SLOTS).find(|i| pending[*i].is_none()) else {
            continue;
        };
        let request = match block::decode_request(&buf[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = block::encode_error(0, 1, &mut ready_buf[free_idx]).unwrap_or(0);
                pending[free_idx] = Some(Pending::Ready { len: m });
                order[order_len] = free_idx;
                order_len += 1;
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
        let ready = |m: usize,
                     pending: &mut [Option<Pending>; SLOTS],
                     order: &mut [usize; SLOTS],
                     order_len: &mut usize| {
            pending[free_idx] = Some(Pending::Ready { len: m });
            order[*order_len] = free_idx;
            *order_len += 1;
        };
        match request {
            Request::Capacity(_) => {
                let m = block::CapacityResponse {
                    sectors: dev.capacity,
                }
                .encode_msg(&mut ready_buf[free_idx])
                .unwrap_or(0);
                ready(m, &mut pending, &mut order, &mut order_len);
            }
            Request::Identity(_) => {
                let mut r = block::IdentityResponse {
                    read_only: dev.read_only as u8,
                    serial: [0; 20],
                    serial_len: 20,
                };
                r.serial.copy_from_slice(&dev.serial);
                let m = r.encode_msg(&mut ready_buf[free_idx]).unwrap_or(0);
                ready(m, &mut pending, &mut order, &mut order_len);
            }
            Request::Read(rq) => {
                let count = rq.count as usize;
                if count == 0 || count > MAX_SECTORS || rq.sector + rq.count as u64 > dev.capacity {
                    let m = block::encode_error(
                        block::ReadRequest::METHOD_ID,
                        2,
                        &mut ready_buf[free_idx],
                    )
                    .unwrap_or(0);
                    ready(m, &mut pending, &mut order, &mut order_len);
                } else {
                    let slot = (0..SLOTS).find(|s| slot_free[*s]).unwrap_or(0);
                    slot_free[slot] = false;
                    dev.submit_slot(slot, false, rq.sector, count);
                    pending[free_idx] = Some(Pending::Io {
                        slot,
                        write: false,
                        bytes: count * SECTOR,
                    });
                    order[order_len] = free_idx;
                    order_len += 1;
                }
            }
            Request::Write(rq) => {
                let count = rq.count as usize;
                let bytes = count * SECTOR;
                if dev.read_only {
                    let m = block::encode_error(
                        block::WriteRequest::METHOD_ID,
                        4,
                        &mut ready_buf[free_idx],
                    )
                    .unwrap_or(0);
                    ready(m, &mut pending, &mut order, &mut order_len);
                } else if count == 0
                    || count > MAX_SECTORS
                    || rq.sector + rq.count as u64 > dev.capacity
                {
                    let m = block::encode_error(
                        block::WriteRequest::METHOD_ID,
                        2,
                        &mut ready_buf[free_idx],
                    )
                    .unwrap_or(0);
                    ready(m, &mut pending, &mut order, &mut order_len);
                } else if (rq.data_len as usize) < bytes {
                    let m = block::encode_error(
                        block::WriteRequest::METHOD_ID,
                        3,
                        &mut ready_buf[free_idx],
                    )
                    .unwrap_or(0);
                    ready(m, &mut pending, &mut order, &mut order_len);
                } else {
                    let slot = (0..SLOTS).find(|s| slot_free[*s]).unwrap_or(0);
                    slot_free[slot] = false;
                    // SAFETY: página de DMA exclusiva do slot; `bytes <= 3584`.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            rq.data.as_ptr(),
                            dev.slot_data[slot].virt as *mut u8,
                            bytes,
                        )
                    };
                    dev.submit_slot(slot, true, rq.sector, count);
                    pending[free_idx] = Some(Pending::Io {
                        slot,
                        write: true,
                        bytes,
                    });
                    order[order_len] = free_idx;
                    order_len += 1;
                }
            }
        }
        max_in_flight = max_in_flight.max(order_len);
    }
}

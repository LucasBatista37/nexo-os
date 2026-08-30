//! `blockdev` — driver VirtIO-block em modo usuário (VirtIO 1.x sobre PCI,
//! interface moderna, fila dividida, MSI-X). Handle 0 = concessão de
//! dispositivo; handle 1 = canal de pedidos. Protocolo provisório
//! (`docs/spec/ipc-compat.md` §5): pedido `[op u8][pad 3][sector u64][count u32][dados…]`
//! com `op` 0 = ler, 1 = escrever; resposta `[status u8][dados…]`.
#![no_std]
#![no_main]

use core::ptr::{read_volatile, write_volatile};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};

const DEV: Handle = 0;
const CHAN: Handle = 1;
const SECTOR: usize = 512;
const MAX_SECTORS: usize = 7; // 7 × 512 + cabeçalho + status cabem em uma mensagem de 4096 B
const QUEUE_SIZE: u16 = 64;

// Capabilities VirtIO-PCI (tipo em cap+3).
const VIRTIO_PCI_CAP_COMMON: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY: u8 = 2;
const VIRTIO_PCI_CAP_ISR: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE: u8 = 4;

// Registradores da configuração comum (offsets).
const C_DEV_FEAT_SEL: u64 = 0;
const C_DEV_FEAT: u64 = 4;
const C_DRV_FEAT_SEL: u64 = 8;
const C_DRV_FEAT: u64 = 12;
const C_MSIX_CFG: u64 = 16;
const C_NUM_QUEUES: u64 = 18;
const C_STATUS: u64 = 20;
const C_Q_SEL: u64 = 22;
const C_Q_SIZE: u64 = 24;
const C_Q_MSIX: u64 = 26;
const C_Q_ENABLE: u64 = 28;
const C_Q_NOTIFY_OFF: u64 = 30;
const C_Q_DESC: u64 = 32;
const C_Q_AVAIL: u64 = 40;
const C_Q_USED: u64 = 48;

const S_ACK: u8 = 1;
const S_DRIVER: u8 = 2;
const S_DRIVER_OK: u8 = 4;
const S_FEATURES_OK: u8 = 8;
const S_FAILED: u8 = 128;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Desc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}
const DESC_NEXT: u16 = 1;
const DESC_WRITE: u16 = 2;

#[repr(C)]
struct BlkReq {
    kind: u32,
    reserved: u32,
    sector: u64,
}

struct Mmio(u64);
impl Mmio {
    // SAFETY (todos): os offsets ficam dentro do intervalo mapeado por `mmio_map`.
    fn r8(&self, off: u64) -> u8 {
        unsafe { read_volatile((self.0 + off) as *const u8) }
    }
    fn r16(&self, off: u64) -> u16 {
        unsafe { read_volatile((self.0 + off) as *const u16) }
    }
    fn r32(&self, off: u64) -> u32 {
        unsafe { read_volatile((self.0 + off) as *const u32) }
    }
    fn w8(&self, off: u64, v: u8) {
        unsafe { write_volatile((self.0 + off) as *mut u8, v) }
    }
    fn w16(&self, off: u64, v: u16) {
        unsafe { write_volatile((self.0 + off) as *mut u16, v) }
    }
    fn w32(&self, off: u64, v: u32) {
        unsafe { write_volatile((self.0 + off) as *mut u32, v) }
    }
    fn w64(&self, off: u64, v: u64) {
        self.w32(off, v as u32);
        self.w32(off + 4, (v >> 32) as u32);
    }
}

struct Device {
    common: Mmio,
    notify_base: u64,
    notify_mult: u32,
    isr: Mmio,
    device_cfg: Mmio,
    // Fila 0.
    desc: nexo_sys::abi::DmaBuffer,
    avail: nexo_sys::abi::DmaBuffer,
    used: nexo_sys::abi::DmaBuffer,
    queue_size: u16,
    notify_off: u16,
    avail_idx: u16,
    used_idx: u16,
    // Buffers de pedido.
    hdr: nexo_sys::abi::DmaBuffer,
    data: nexo_sys::abi::DmaBuffer,
    status: nexo_sys::abi::DmaBuffer,
    irq: Option<nexo_sys::abi::IrqInfo>,
    irq_seen: u64,
    capacity: u64,
}

fn fail(code: i64, what: &str) -> ! {
    log!("blockdev: falha: {}", what);
    nexo_sys::exit(code)
}

fn find_virtio_blk() -> PciInfo {
    let mut devs = [PciInfo::default(); 32];
    let n = match nexo_sys::pci_enum(DEV, &mut devs) {
        Ok(n) => n.min(32),
        Err(_) => fail(10, "pci_enum"),
    };
    for d in &devs[..n] {
        // 0x1001 (transicional) ou 0x1042 (moderno) = virtio-blk
        if d.is_virtio() && (d.device == 0x1001 || d.device == 0x1042) {
            return *d;
        }
    }
    fail(11, "virtio-blk nao encontrado")
}

/// Mapeia o BAR `bar` inteiro; devolve o endereço virtual.
fn map_bar(dev: &PciInfo, bar: u8, mapped: &mut [(u8, u64); 6]) -> u64 {
    for (b, v) in mapped.iter() {
        if *b == bar && *v != 0 {
            return *v;
        }
    }
    let info = dev.bars[bar as usize];
    if info.size == 0 || info.flags & 1 != 0 {
        fail(12, "BAR invalido ou de E/S");
    }
    let v = match nexo_sys::mmio_map(DEV, info.base, info.size) {
        Ok(v) => v,
        Err(_) => fail(13, "mmio_map"),
    };
    for slot in mapped.iter_mut() {
        if slot.1 == 0 {
            *slot = (bar, v);
            break;
        }
    }
    v
}

fn cfg8(bdf: u16, off: u16) -> u8 {
    let v = nexo_sys::pci_cfg_read(DEV, bdf, off & !3).unwrap_or(0);
    (v >> ((off & 3) * 8)) as u8
}
fn cfg32(bdf: u16, off: u16) -> u32 {
    nexo_sys::pci_cfg_read(DEV, bdf, off).unwrap_or(0)
}

fn setup(dev: &PciInfo) -> Device {
    let bdf = dev.bdf;
    // Command: memória + bus master, INTx desabilitado.
    let cmd = cfg32(bdf, 4);
    nexo_sys::pci_cfg_write(
        DEV,
        bdf,
        4,
        (cmd & 0xffff_0000) | (cmd & 0xffff) | 0x6 | (1 << 10),
    );

    let mut mapped = [(0u8, 0u64); 6];
    let mut common = 0u64;
    let mut notify_base = 0u64;
    let mut notify_mult = 0u32;
    let mut isr = 0u64;
    let mut device_cfg = 0u64;
    let mut msix_cap = 0u16;
    // Percorre a lista de capabilities.
    let mut cap = cfg8(bdf, 0x34) as u16;
    let mut guard = 0;
    while cap != 0 && guard < 32 {
        guard += 1;
        let id = cfg8(bdf, cap);
        if id == 0x11 {
            msix_cap = cap;
        } else if id == 0x09 {
            let cfg_type = cfg8(bdf, cap + 3);
            let bar = cfg8(bdf, cap + 4);
            let offset = cfg32(bdf, cap + 8) as u64;
            match cfg_type {
                VIRTIO_PCI_CAP_COMMON => common = map_bar(dev, bar, &mut mapped) + offset,
                VIRTIO_PCI_CAP_NOTIFY => {
                    notify_base = map_bar(dev, bar, &mut mapped) + offset;
                    notify_mult = cfg32(bdf, cap + 16);
                }
                VIRTIO_PCI_CAP_ISR => isr = map_bar(dev, bar, &mut mapped) + offset,
                VIRTIO_PCI_CAP_DEVICE => device_cfg = map_bar(dev, bar, &mut mapped) + offset,
                _ => {}
            }
        }
        cap = cfg8(bdf, cap + 1) as u16;
    }
    if common == 0 || notify_base == 0 || device_cfg == 0 {
        fail(14, "capabilities virtio-pci modernas ausentes");
    }
    let common = Mmio(common);

    // Reset e negociação de features: só VERSION_1 (bit 32).
    common.w8(C_STATUS, 0);
    while common.r8(C_STATUS) != 0 {}
    common.w8(C_STATUS, S_ACK);
    common.w8(C_STATUS, S_ACK | S_DRIVER);
    common.w32(C_DEV_FEAT_SEL, 1);
    let feat_hi = common.r32(C_DEV_FEAT);
    if feat_hi & 1 == 0 {
        fail(15, "dispositivo sem VIRTIO_F_VERSION_1");
    }
    common.w32(C_DRV_FEAT_SEL, 0);
    common.w32(C_DRV_FEAT, 0);
    common.w32(C_DRV_FEAT_SEL, 1);
    common.w32(C_DRV_FEAT, 1);
    common.w8(C_STATUS, S_ACK | S_DRIVER | S_FEATURES_OK);
    if common.r8(C_STATUS) & S_FEATURES_OK == 0 {
        common.w8(C_STATUS, S_FAILED);
        fail(16, "FEATURES_OK rejeitado");
    }

    // MSI-X: entrada 0 aponta para o vetor do kernel.
    let mut irq = None;
    if msix_cap != 0
        && let Ok(info) = nexo_sys::irq_alloc(DEV)
    {
        let ctrl = cfg32(bdf, msix_cap);
        let table = cfg32(bdf, msix_cap + 4);
        let (tbar, toff) = ((table & 7) as u8, (table & !7) as u64);
        let tbase = map_bar(dev, tbar, &mut mapped) + toff;
        let t = Mmio(tbase);
        t.w32(0, info.msi_address as u32);
        t.w32(4, (info.msi_address >> 32) as u32);
        t.w32(8, info.msi_data);
        t.w32(12, 0); // desmascara a entrada 0
        // Habilita MSI-X (bit 15 do message control), sem function mask (bit 14).
        let mc = (ctrl >> 16) as u16;
        let new_mc = (mc | 0x8000) & !0x4000;
        nexo_sys::pci_cfg_write(
            DEV,
            bdf,
            msix_cap,
            (ctrl & 0xffff) | ((new_mc as u32) << 16),
        );
        common.w16(C_MSIX_CFG, 0xffff); // sem interrupcao de configuracao
        irq = Some(info);
    }

    // Fila 0.
    if common.r16(C_NUM_QUEUES) == 0 {
        fail(17, "sem filas");
    }
    common.w16(C_Q_SEL, 0);
    let qmax = common.r16(C_Q_SIZE);
    let queue_size = qmax.min(QUEUE_SIZE);
    common.w16(C_Q_SIZE, queue_size);
    let alloc = || nexo_sys::dma_alloc(DEV).unwrap_or_else(|_| fail(18, "dma_alloc"));
    let (desc, avail, used) = (alloc(), alloc(), alloc());
    common.w64(C_Q_DESC, desc.phys);
    common.w64(C_Q_AVAIL, avail.phys);
    common.w64(C_Q_USED, used.phys);
    common.w16(C_Q_MSIX, if irq.is_some() { 0 } else { 0xffff });
    let notify_off = common.r16(C_Q_NOTIFY_OFF);
    common.w16(C_Q_ENABLE, 1);
    common.w8(C_STATUS, S_ACK | S_DRIVER | S_FEATURES_OK | S_DRIVER_OK);

    let device_cfg = Mmio(device_cfg);
    let capacity = device_cfg.r32(0) as u64 | ((device_cfg.r32(4) as u64) << 32);
    Device {
        common,
        notify_base,
        notify_mult,
        isr: Mmio(isr),
        device_cfg,
        desc,
        avail,
        used,
        queue_size,
        notify_off,
        avail_idx: 0,
        used_idx: 0,
        hdr: alloc(),
        data: alloc(),
        status: alloc(),
        irq,
        irq_seen: 0,
        capacity,
    }
}

impl Device {
    fn desc_write(&self, i: u16, d: Desc) {
        // SAFETY: `i < queue_size`; a tabela de descritores ocupa `16 * queue_size` bytes na página.
        unsafe { write_volatile((self.desc.virt as *mut Desc).add(i as usize), d) }
    }

    /// Executa um pedido de `count` setores a partir de `sector` (dados em `self.data`).
    fn request(&mut self, write: bool, sector: u64, count: usize) -> Result<(), u8> {
        let hdr = self.hdr.virt as *mut BlkReq;
        // SAFETY: página de DMA exclusiva.
        unsafe {
            hdr.write_volatile(BlkReq {
                kind: write as u32,
                reserved: 0,
                sector,
            })
        };
        // SAFETY: idem.
        unsafe { (self.status.virt as *mut u8).write_volatile(0xff) };
        let len = (count * SECTOR) as u32;
        self.desc_write(
            0,
            Desc {
                addr: self.hdr.phys,
                len: 16,
                flags: DESC_NEXT,
                next: 1,
            },
        );
        self.desc_write(
            1,
            Desc {
                addr: self.data.phys,
                len,
                flags: DESC_NEXT | if write { 0 } else { DESC_WRITE },
                next: 2,
            },
        );
        self.desc_write(
            2,
            Desc {
                addr: self.status.phys,
                len: 1,
                flags: DESC_WRITE,
                next: 0,
            },
        );
        let qs = self.queue_size as u64;
        // avail: flags u16 @0, idx u16 @2, ring[i] u16 @4+2i
        let ring = self.avail.virt + 4 + 2 * (self.avail_idx as u64 % qs);
        // SAFETY: página de DMA exclusiva; índices dentro da fila.
        unsafe {
            write_volatile(ring as *mut u16, 0);
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            self.avail_idx = self.avail_idx.wrapping_add(1);
            write_volatile((self.avail.virt + 2) as *mut u16, self.avail_idx);
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }
        // Notificação: escreve o índice da fila no endereço de notify.
        let notify = Mmio(self.notify_base + self.notify_off as u64 * self.notify_mult as u64);
        notify.w16(0, 0);
        // Espera used.idx avançar (interrupcao MSI-X, com fallback de polling).
        let mut spins = 0u64;
        loop {
            // SAFETY: página de DMA exclusiva.
            let used_idx = unsafe { read_volatile((self.used.virt + 2) as *const u16) };
            if used_idx != self.used_idx {
                self.used_idx = used_idx;
                break;
            }
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
        let _ = self.isr.r8(0);
        // SAFETY: página de DMA exclusiva.
        let st = unsafe { (self.status.virt as *const u8).read_volatile() };
        if st == 0 { Ok(()) } else { Err(st) }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(crash_after: u64) -> ! {
    let pci = find_virtio_blk();
    let mut dev = setup(&pci);
    log!(
        "blockdev: virtio-blk {:04x}:{:04x} bdf {:#06x} capacidade {} setores, fila {}, {}",
        pci.vendor,
        pci.device,
        pci.bdf,
        dev.capacity,
        dev.queue_size,
        if dev.irq.is_some() {
            "MSI-X"
        } else {
            "polling"
        }
    );
    let _ = (dev.common.r8(C_STATUS), dev.device_cfg.r32(0));
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

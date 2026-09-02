//! Enumeração PCI (barramentos 0..256, 32 dispositivos, 8 funções) com
//! decodificação de BARs (tamanho por sondagem) e tabela global.

use alloc::vec::Vec;
pub use nexo_arch_x86_64::pci::Bdf;
use nexo_arch_x86_64::pci::{config_read32, config_write32};
use nexo_syscall_abi::{PCI_BARS, PciBar, PciInfo};

use crate::sync::IrqLock;

static DEVICES: IrqLock<Vec<PciInfo>> = IrqLock::new(Vec::new());

/// Lê 32 bits de configuração (serializado).
pub fn cfg_read(bdf: Bdf, offset: u8) -> u32 {
    let _g = CFG_LOCK.lock();
    // SAFETY: acesso serializado pelo lock.
    unsafe { config_read32(bdf, offset) }
}

/// Escreve 32 bits de configuração (serializado).
pub fn cfg_write(bdf: Bdf, offset: u8, value: u32) {
    let _g = CFG_LOCK.lock();
    // SAFETY: acesso serializado pelo lock.
    unsafe { config_write32(bdf, offset, value) }
}

static CFG_LOCK: IrqLock<()> = IrqLock::new(());

fn probe_bars(bdf: Bdf, header_type: u8) -> [PciBar; PCI_BARS] {
    let mut bars = [PciBar::default(); PCI_BARS];
    let count = if header_type & 0x7f == 0 { 6 } else { 2 };
    let mut i = 0;
    while i < count {
        let off = 0x10 + (i as u8) * 4;
        let orig = cfg_read(bdf, off);
        if orig == 0 {
            i += 1;
            continue;
        }
        let io = orig & 1 != 0;
        let is64 = !io && (orig >> 1) & 3 == 2;
        cfg_write(bdf, off, 0xffff_ffff);
        let mask_lo = cfg_read(bdf, off);
        cfg_write(bdf, off, orig);
        let (base_lo, size_mask) = if io {
            (
                (orig & !3) as u64,
                (mask_lo & !3) as u64 | 0xffff_ffff_0000_0000,
            )
        } else {
            ((orig & !0xf) as u64, (mask_lo & !0xf) as u64)
        };
        let mut base = base_lo;
        let mut mask = size_mask;
        if is64 && i + 1 < count {
            let off_hi = off + 4;
            let orig_hi = cfg_read(bdf, off_hi);
            cfg_write(bdf, off_hi, 0xffff_ffff);
            let mask_hi = cfg_read(bdf, off_hi);
            cfg_write(bdf, off_hi, orig_hi);
            base |= (orig_hi as u64) << 32;
            mask = (mask_lo & !0xf) as u64 | ((mask_hi as u64) << 32);
        } else if !io {
            mask |= 0xffff_ffff_0000_0000;
        }
        let size = if mask == 0 {
            0
        } else {
            (!mask).wrapping_add(1) & if is64 { u64::MAX } else { 0xffff_ffff }
        };
        bars[i] = PciBar {
            base,
            size,
            flags: (io as u32) | ((is64 as u32) << 1) | (((orig >> 3) & 1) << 2),
            reserved: 0,
        };
        i += if is64 { 2 } else { 1 };
    }
    bars
}

fn read_function(bdf: Bdf) -> Option<PciInfo> {
    let id = cfg_read(bdf, 0);
    if id & 0xffff == 0xffff {
        return None;
    }
    let class = cfg_read(bdf, 8);
    let hdr = cfg_read(bdf, 0xc);
    let header_type = ((hdr >> 16) & 0xff) as u8;
    let irq = cfg_read(bdf, 0x3c);
    Some(PciInfo {
        bdf: bdf.packed(),
        vendor: (id & 0xffff) as u16,
        device: (id >> 16) as u16,
        revision: (class & 0xff) as u8,
        header_type,
        class: (class >> 24) as u8,
        subclass: ((class >> 16) & 0xff) as u8,
        prog_if: ((class >> 8) & 0xff) as u8,
        irq_line: (irq & 0xff) as u8,
        irq_pin: ((irq >> 8) & 0xff) as u8,
        reserved: [0; 3],
        subsystem: if header_type & 0x7f == 0 {
            cfg_read(bdf, 0x2c)
        } else {
            0
        },
        bars: probe_bars(bdf, header_type),
    })
}

/// Enumera todas as funções e registra a tabela.
pub fn init() {
    let mut found = Vec::new();
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            let f0 = Bdf::new(bus, dev, 0);
            let Some(info) = read_function(f0) else {
                continue;
            };
            let multi = info.header_type & 0x80 != 0;
            found.push(info);
            if multi {
                for func in 1..8u8 {
                    if let Some(i) = read_function(Bdf::new(bus, dev, func)) {
                        found.push(i);
                    }
                }
            }
        }
        // Sem bridges enumeradas explicitamente: barramentos > 0 só aparecem se o firmware os configurou.
        if bus == 0 && !found.iter().any(|d| d.class == 0x06 && d.subclass == 0x04) {
            break;
        }
    }
    kinfo!("pci: {} funcao(oes):", found.len());
    for d in &found {
        let bdf = Bdf::from_packed(d.bdf);
        kinfo!(
            "pci:   {:02x}:{:02x}.{} {:04x}:{:04x} classe {:02x}.{:02x}.{:02x} irq {}/{}{}",
            bdf.bus,
            bdf.device,
            bdf.function,
            d.vendor,
            d.device,
            d.class,
            d.subclass,
            d.prog_if,
            d.irq_line,
            d.irq_pin,
            if d.is_virtio() { " virtio" } else { "" }
        );
        for (i, b) in d.bars.iter().enumerate() {
            if b.size != 0 {
                kdebug!(
                    "pci:     bar{i} {:#x} +{:#x} {}{}",
                    b.base,
                    b.size,
                    if b.flags & 1 != 0 { "io" } else { "mmio" },
                    if b.flags & 2 != 0 { "64" } else { "" }
                );
            }
        }
    }
    *DEVICES.lock() = found;
}

/// Cópia da tabela.
pub fn devices() -> Vec<PciInfo> {
    DEVICES.lock().clone()
}

/// `true` se `[phys, phys+len)` cai dentro de um BAR MMIO de alguma função
/// (ou só da função `bdf`, se informada).
pub fn is_mmio_range(bdf: Option<u16>, phys: u64, len: u64) -> bool {
    let end = match phys.checked_add(len) {
        Some(e) => e,
        None => return false,
    };
    DEVICES
        .lock()
        .iter()
        .filter(|d| bdf.is_none_or(|b| b == d.bdf))
        .any(|d| {
            d.bars.iter().any(|b| {
                b.size != 0 && b.flags & 1 == 0 && phys >= b.base && end <= b.base + b.size
            })
        })
}

/// `true` se a função `bdf` foi enumerada.
pub fn exists(bdf: u16) -> bool {
    DEVICES.lock().iter().any(|d| d.bdf == bdf)
}

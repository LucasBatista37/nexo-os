//! Enumeração PCI (barramentos 0..256, 32 dispositivos, 8 funções) com
//! decodificação de BARs (tamanho por sondagem) e tabela global.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use nexo_arch_x86_64::paging::PageFlags;
pub use nexo_arch_x86_64::pci::Bdf;
use nexo_arch_x86_64::pci::{config_read32, config_write32};
use nexo_mm::{PhysAddr, VirtAddr};
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

/// Lê 32 bits de configuração aceitando os **4096** bytes do PCIe.
///
/// Abaixo de 256 usa o mecanismo legado (que qualquer máquina tem); de 256 em diante exige
/// ECAM e devolve `None` sem ele. Existe porque a syscall truncava o deslocamento a 8 bits em
/// silêncio: pedir `0x100` devolvia o vendor ID, e um driver que procurasse uma capability
/// estendida encontraria lixo plausível.
pub fn cfg_read_ext(bdf: Bdf, offset: u16) -> Option<u32> {
    if offset >= 4096 {
        return None;
    }
    if offset < 256 {
        return Some(cfg_read(bdf, offset as u8));
    }
    ecam_read32(bdf.packed(), offset)
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
    ecam_init();
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

// ---------------------------------------------------------------------------
// ECAM (configuração estendida do PCIe)
// ---------------------------------------------------------------------------

/// Janela virtual do ECAM do barramento 0 (1 MiB: 32 dispositivos × 8 funções × 4 KiB).
const ECAM_VIRT: u64 = 0xffff_ffff_e020_0000;
/// Só o barramento 0 é mapeado — é o único que este sistema enumera.
const ECAM_BUS0_BYTES: u64 = 1 << 20;

static ECAM_PRONTO: AtomicBool = AtomicBool::new(false);

/// Mapeia a janela ECAM do barramento 0, se o firmware anunciou uma faixa que o cubra.
///
/// Sem ECAM o sistema continua igual: os 256 bytes legados bastam para VirtIO, NVMe e AHCI. O
/// que o ECAM traz são os outros 3840 bytes de cada função — onde vivem as capabilities
/// estendidas do PCIe (AER, SR-IOV, ATS) de que o hardware real precisa.
pub fn ecam_init() {
    let plat = crate::acpi::info();
    let Some(faixa) = plat.mcfg[..plat.mcfg_count]
        .iter()
        .flatten()
        .find(|f| f.segment == 0 && f.bus_start == 0)
    else {
        kdebug!("pci: sem faixa ECAM para o barramento 0; acesso legado apenas");
        return;
    };
    let mut off = 0;
    while off < ECAM_BUS0_BYTES {
        let virt = VirtAddr::new(ECAM_VIRT + off);
        let phys = PhysAddr::new(faixa.base + off);
        if let Err(e) = crate::mm::virt::map_page(virt, phys, ECAM_FLAGS) {
            kwarn!("pci: ECAM nao mapeou {virt:?} ({e}); acesso legado apenas");
            return;
        }
        off += 4096;
    }
    ECAM_PRONTO.store(true, Ordering::Release);
    kinfo!(
        "pci: ECAM do barramento 0 mapeado ({:#x} -> {:#x}, {} KiB)",
        faixa.base,
        ECAM_VIRT,
        ECAM_BUS0_BYTES >> 10
    );
}

/// Flags do mapeamento ECAM: memória de dispositivo, nunca em cache.
const ECAM_FLAGS: PageFlags = PageFlags::KERNEL_RW
    .union(PageFlags::NO_CACHE)
    .union(PageFlags::WRITE_THROUGH);

/// `true` se a configuração estendida está disponível.
pub fn ecam_disponivel() -> bool {
    ECAM_PRONTO.load(Ordering::Acquire)
}

/// Lê 32 bits da configuração de `bdf` no deslocamento `offset` pelo ECAM.
///
/// Devolve `None` sem ECAM, fora do barramento 0, ou com deslocamento desalinhado ou além dos
/// 4096 bytes de uma função.
pub fn ecam_read32(bdf: u16, offset: u16) -> Option<u32> {
    if !ecam_disponivel() || bdf >> 8 != 0 || offset >= 4096 || !offset.is_multiple_of(4) {
        return None;
    }
    let dev = u64::from((bdf >> 3) & 0x1f);
    let fun = u64::from(bdf & 7);
    let virt = ECAM_VIRT + (dev << 15) + (fun << 12) + u64::from(offset);
    // SAFETY: página mapeada por `ecam_init` como MMIO sem cache; leitura alinhada de 32 bits
    // dentro da janela de 4 KiB daquela função.
    Some(unsafe { core::ptr::read_volatile(virt as *const u32) })
}

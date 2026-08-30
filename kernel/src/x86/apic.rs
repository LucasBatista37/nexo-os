//! LAPIC/I-O APIC do kernel: mapeamento sem cache, vetores, EOI, roteamento de IRQs ISA.

use alloc::vec::Vec;
use nexo_arch_x86_64::apic::{self, LocalApic};
use nexo_arch_x86_64::ioapic::{IoApic, Redirection};
use nexo_arch_x86_64::paging::PageFlags;
use nexo_arch_x86_64::{cpu, pic};
use nexo_mm::{PhysAddr, VirtAddr};
use nexo_sync::Once;

use crate::sync::IrqLock;

/// Endereço virtual do LAPIC.
pub const LAPIC_VIRT: u64 = 0xffff_ffff_e000_0000;
/// Base virtual dos I/O APICs (uma página cada).
pub const IOAPIC_VIRT: u64 = 0xffff_ffff_e010_0000;

/// Vetores de interrupção usados pelo kernel.
pub mod vectors {
    /// Timer do LAPIC (por CPU).
    pub const TIMER: u8 = 0x20;
    /// Entrada de teste do I/O APIC (PIT via GSI).
    pub const IOAPIC_TEST: u8 = 0x21;
    /// Base das IRQs legadas do PIC (mascarado; apenas para diagnóstico de espúrias).
    pub const PIC_BASE: u8 = 0x30;
    /// IPI: reescalonar.
    pub const RESCHED: u8 = 0xf0;
    /// IPI: parar a CPU (panic).
    pub const HALT: u8 = 0xf1;
    /// IPI: invalidar TLB.
    pub const TLB_FLUSH: u8 = 0xf2;
    /// Erro do LAPIC.
    pub const APIC_ERROR: u8 = 0xfe;
    /// Espúria.
    pub const SPURIOUS: u8 = 0xff;
}

static LAPIC: Once<LocalApic> = Once::new();
static IOAPICS: IrqLock<Vec<IoApic>> = IrqLock::new(Vec::new());

/// Flags de mapeamento para MMIO.
const MMIO_FLAGS: PageFlags = PageFlags::KERNEL_RW
    .union(PageFlags::NO_CACHE)
    .union(PageFlags::WRITE_THROUGH);

/// Inicializa LAPIC da BSP, mascara o PIC e mapeia/mascara os I/O APICs.
pub fn init_bsp() {
    if !apic::cpu_has_apic() {
        kerror!("apic: CPU sem LAPIC; impossivel continuar");
        cpu::halt_forever();
    }
    let plat = crate::acpi::info();
    let (msr_base, enabled, _) = apic::apic_base();
    let phys = if plat.lapic_phys != 0 {
        plat.lapic_phys
    } else {
        msr_base
    };
    if plat.lapic_phys != 0 && plat.lapic_phys != msr_base {
        kwarn!(
            "apic: MADT informa LAPIC em {:#x}, MSR em {:#x}; usando MADT",
            plat.lapic_phys,
            msr_base
        );
    }
    // SAFETY: habilitar o APIC global é pré-requisito do resto da inicialização.
    unsafe { apic::enable_global() };
    if let Err(e) =
        crate::mm::virt::map_page(VirtAddr::new(LAPIC_VIRT), PhysAddr::new(phys), MMIO_FLAGS)
    {
        kerror!("apic: falha ao mapear LAPIC: {e}");
        cpu::halt_forever();
    }
    // SAFETY: página do LAPIC mapeada sem cache.
    let lapic = unsafe { LocalApic::new(LAPIC_VIRT) };
    lapic.enable(vectors::SPURIOUS, vectors::APIC_ERROR);
    let _ = LAPIC.set(lapic);

    // PIC: remapeia para fora das exceções e mascara tudo (IRQs vão pelo I/O APIC).
    // SAFETY: IDT instalada; PIC não será mais usado.
    unsafe {
        pic::remap(vectors::PIC_BASE, vectors::PIC_BASE + 8);
        pic::disable();
    }

    let mut ioapics = IOAPICS.lock();
    for (i, io) in plat.ioapics().iter().enumerate() {
        let virt = IOAPIC_VIRT + (i as u64) * 0x1000;
        if let Err(e) =
            crate::mm::virt::map_page(VirtAddr::new(virt), PhysAddr::new(io.phys), MMIO_FLAGS)
        {
            kwarn!("apic: falha ao mapear I/O APIC {}: {e}", io.id);
            continue;
        }
        // SAFETY: página mapeada sem cache.
        let a = unsafe { IoApic::new(virt, io.gsi_base) };
        a.mask_all();
        kinfo!(
            "apic: I/O APIC id={} versao {:#x} com {} entradas, GSI {}..{} (mascaradas)",
            a.id(),
            a.version(),
            a.entries(),
            io.gsi_base,
            io.gsi_base + a.entries()
        );
        ioapics.push(a);
    }
    drop(ioapics);
    kinfo!(
        "apic: LAPIC id={} versao {:#x} em {:#x} (msr {}), spurious {:#x}, PIC mascarado em {:#x}",
        lapic.id(),
        lapic.version() & 0xff,
        phys,
        if enabled {
            "ja habilitado"
        } else {
            "habilitado agora"
        },
        vectors::SPURIOUS,
        vectors::PIC_BASE
    );
}

/// LAPIC desta CPU.
pub fn lapic() -> &'static LocalApic {
    LAPIC.get().expect("apic::init_bsp nao foi chamado")
}

/// LAPIC, se já inicializado.
pub fn try_lapic() -> Option<&'static LocalApic> {
    LAPIC.get()
}

/// Fim de interrupção.
#[inline]
pub fn eoi() {
    if let Some(l) = LAPIC.get() {
        l.eoi();
    }
}

/// Roteia uma IRQ ISA para `vector` na CPU `dest_apic_id`. Devolve o GSI usado.
pub fn route_isa_irq(irq: u8, vector: u8, dest_apic_id: u32) -> Result<u32, &'static str> {
    let plat = crate::acpi::info();
    let (gsi, flags) = plat.isa_irq_to_gsi(irq);
    let active_low = flags & 0b11 == 0b11;
    let level = (flags >> 2) & 0b11 == 0b11;
    let ioapics = IOAPICS.lock();
    let io = ioapics
        .iter()
        .find(|a| a.handles(gsi))
        .ok_or("nenhum I/O APIC atende o GSI")?;
    io.set_redirection(
        gsi,
        Redirection {
            vector,
            dest_apic_id,
            level_triggered: level,
            active_low,
            masked: false,
        },
    );
    Ok(gsi)
}

/// Mascara um GSI.
pub fn mask_gsi(gsi: u32) {
    let ioapics = IOAPICS.lock();
    if let Some(io) = ioapics.iter().find(|a| a.handles(gsi)) {
        io.mask(gsi);
    }
}

/// Número de I/O APICs mapeados.
pub fn ioapic_count() -> usize {
    IOAPICS.lock().len()
}

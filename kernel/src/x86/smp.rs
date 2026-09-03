//! Inicialização das CPUs de aplicação (APs) via INIT/SIPI e trampolim.

use core::sync::atomic::Ordering;
use nexo_arch_x86_64::apic::LocalApic;
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::paging::PageFlags;
use nexo_arch_x86_64::smp::{
    SIPI_VECTOR, TRAMPOLINE_PHYS, TrampolineParams, install, trampoline_image,
};
use nexo_mm::{PAGE_SIZE, PhysAddr, VirtAddr};

use super::apic::vectors;
use super::percpu::{self, PerCpu};

/// Base virtual das pilhas das APs (slots de 128 KiB: 64 KiB mapeados, resto guarda).
pub const CPU_STACK_BASE: u64 = 0xffff_ffff_a000_0000;
const CPU_STACK_SLOT: u64 = 0x2_0000;
const CPU_STACK_SIZE: u64 = 0x1_0000;

/// Entrada das APs (chega pelo trampolim com `RDI = &PerCpu`, pilha própria, IF=0).
extern "sysv64" fn ap_entry(arg: usize) -> ! {
    let cpu_ref: &'static PerCpu = {
        // SAFETY: a BSP gravou o ponteiro de uma estrutura 'static no trampolim.
        unsafe { &*(arg as *const PerCpu) }
    };
    // SAFETY: primeira ativação nesta CPU.
    unsafe {
        cpu_ref.activate();
        super::traps::load_idt();
        cpu::enable_write_protect();
        cpu::enable_sse();
    }
    let lapic = super::apic::lapic();
    lapic.enable(vectors::SPURIOUS, vectors::APIC_ERROR);
    crate::time::start_local_timer();
    cpu_ref.set_online();
    kinfo!(
        "smp: cpu{} apic_id={} online (lapic id {}, pilha {:#x})",
        cpu_ref.index,
        cpu_ref.apic_id,
        lapic.id(),
        cpu_ref.stack_base + cpu_ref.stack_size
    );
    // SAFETY: IDT carregada e LAPIC configurado nesta CPU.
    unsafe { cpu::enable_interrupts() };
    crate::sched::ap_idle_loop()
}

/// Inicia todas as APs listadas pela ACPI. Devolve quantas ficaram online.
pub fn boot_aps() -> usize {
    let plat = crate::acpi::info();
    let cpus = plat.cpus();
    if cpus.len() <= 1 {
        kinfo!("smp: 1 CPU; nada a iniciar");
        return 1;
    }
    let img = trampoline_image();
    if img.len() > PAGE_SIZE as usize {
        kerror!(
            "smp: trampolim de {} bytes nao cabe em uma pagina",
            img.len()
        );
        return 1;
    }
    // Página do trampolim: identidade (RX) para o código de 16/32 bits + physmap para escrever.
    let ident = VirtAddr::new(TRAMPOLINE_PHYS);
    if let Err(e) =
        crate::mm::virt::map_page(ident, PhysAddr::new(TRAMPOLINE_PHYS), PageFlags::KERNEL_RX)
    {
        kerror!("smp: falha ao mapear trampolim em identidade: {e}");
        return 1;
    }
    let dest = crate::mm::virt::phys_to_virt(PhysAddr::new(TRAMPOLINE_PHYS)).as_mut_ptr::<u8>();
    let pml4 = cpu::read_cr3() & 0x000f_ffff_ffff_f000;
    if pml4 > u32::MAX as u64 {
        kerror!("smp: PML4 acima de 4 GiB; trampolim de 32 bits nao alcanca");
        return 1;
    }
    let lapic: &LocalApic = super::apic::lapic();
    let mut online = 1;
    for (i, info) in cpus.iter().enumerate().skip(1) {
        let slot = CPU_STACK_BASE + i as u64 * CPU_STACK_SLOT;
        let stack_base = slot + CPU_STACK_SLOT - CPU_STACK_SIZE;
        let mut ok = true;
        let mut off = 0;
        while off < CPU_STACK_SIZE {
            if let Err(e) = crate::mm::virt::alloc_and_map(
                VirtAddr::new(stack_base + off),
                PageFlags::KERNEL_RW,
            ) {
                kerror!("smp: sem memoria para a pilha da cpu{i}: {e}");
                ok = false;
                break;
            }
            off += PAGE_SIZE;
        }
        if !ok {
            continue;
        }
        let pc = PerCpu::allocate(i, info.apic_id, stack_base, CPU_STACK_SIZE);
        let params = TrampolineParams {
            pml4: pml4 as u32,
            stack_top: stack_base + CPU_STACK_SIZE,
            entry: ap_entry as *const () as usize as u64,
            arg: pc as *const PerCpu as u64,
        };
        // SAFETY: página 0x8000 reservada (primeiro MiB), acessível pelo physmap.
        unsafe { install(dest, params) };
        kdebug!(
            "smp: cpu{i} apic_id={} INIT/SIPI (vetor {:#x})",
            info.apic_id,
            SIPI_VECTOR
        );
        lapic.send_init(info.apic_id);
        crate::time::delay_us(10_000);
        lapic.send_sipi(info.apic_id, SIPI_VECTOR);
        crate::time::delay_us(300);
        if !pc.online.load(Ordering::Acquire) {
            lapic.send_sipi(info.apic_id, SIPI_VECTOR);
        }
        let mut waited = 0u64;
        while !pc.online.load(Ordering::Acquire) && waited < 1_000_000 {
            crate::time::delay_us(100);
            waited += 100;
        }
        if pc.online.load(Ordering::Acquire) {
            online += 1;
        } else {
            kerror!(
                "smp: cpu{i} (apic_id {}) nao respondeu em {} ms",
                info.apic_id,
                waited / 1000
            );
        }
    }
    let _ = crate::mm::virt::unmap_page(ident);
    kinfo!("smp: {} de {} CPU(s) online", online, cpus.len());
    online
}

/// Envia HALT para as outras CPUs (usado em panic/exceção fatal).
pub fn halt_others() {
    if percpu::online_count() > 1
        && let Some(l) = crate::x86::apic::try_lapic()
    {
        l.send_ipi_all_others(vectors::HALT);
    }
}

/// Invalida o TLB das outras CPUs (fire-and-forget).
pub fn flush_tlb_others() {
    if percpu::online_count() > 1
        && let Some(l) = crate::x86::apic::try_lapic()
    {
        l.send_ipi_all_others(vectors::TLB_FLUSH);
    }
}

/// Envia RESCHED para todas as outras CPUs.
pub fn broadcast_resched() {
    if let Some(l) = crate::x86::apic::try_lapic() {
        l.send_ipi_all_others(vectors::RESCHED);
    }
}

//! Tempo: PIT a 1000 Hz via PIC (APIC chega na Fase 1) e contador monotônico.

use core::sync::atomic::{AtomicU64, Ordering};
use nexo_arch_x86_64::{cpu, pic, pit};

/// Frequência do tick.
pub const HZ: u64 = 1000;

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Remapeia o PIC, programa o PIT e habilita interrupções.
pub fn init() {
    // SAFETY: IDT instalada (x86::traps::init) com handlers para 0x20..0x2f.
    unsafe {
        pic::remap(crate::x86::traps::IRQ_BASE, crate::x86::traps::IRQ_BASE + 8);
        let div = pit::configure_periodic(HZ as u32);
        pic::set_masks(0xfe, 0xff); // apenas IRQ0
        cpu::enable_interrupts();
        kinfo!(
            "time: PIT {} Hz (divisor {}, real {} Hz), IRQ0 habilitada, IF={}",
            HZ,
            div,
            pit::actual_frequency(div),
            cpu::interrupts_enabled()
        );
    }
}

/// Chamado pelo handler da IRQ0.
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Ticks desde o boot.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Milissegundos desde a habilitação do timer.
pub fn uptime_ms() -> u64 {
    ticks() * 1000 / HZ
}

/// Microssegundos (resolução de tick).
pub fn uptime_micros() -> u64 {
    ticks() * 1_000_000 / HZ
}

/// Aguarda `ms` milissegundos (dorme com `hlt` se interrupções estão ativas).
pub fn sleep_ms(ms: u64) {
    let end = ticks() + ms * HZ / 1000;
    while ticks() < end {
        if cpu::interrupts_enabled() {
            cpu::halt();
        } else {
            core::hint::spin_loop();
        }
    }
}

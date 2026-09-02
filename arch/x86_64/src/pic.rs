//! 8259 PIC (mestre/escravo) — usado até o APIC (Fase 1).

use crate::cpu::{inb, outb};

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xa0;
const PIC2_DATA: u16 = 0xa1;

/// Vetor base recomendado para IRQ0.
pub const DEFAULT_OFFSET: u8 = 0x20;

/// Espera curta entre comandos (porta de diagnóstico 0x80).
///
/// # Safety
/// Escreve na porta 0x80 (no-op de temporização): sem outros efeitos.
#[inline]
unsafe fn io_wait() {
    // SAFETY: escrever na porta 0x80 é um no-op de temporização.
    unsafe { outb(0x80, 0) };
}

/// Remapeia os PICs para `offset1`/`offset2` e mascara todas as IRQs.
///
/// # Safety
/// Reprograma o controlador de interrupções.
pub unsafe fn remap(offset1: u8, offset2: u8) {
    // SAFETY: sequência ICW1..ICW4 documentada.
    unsafe {
        outb(PIC1_CMD, 0x11);
        io_wait();
        outb(PIC2_CMD, 0x11);
        io_wait();
        outb(PIC1_DATA, offset1);
        io_wait();
        outb(PIC2_DATA, offset2);
        io_wait();
        outb(PIC1_DATA, 4); // escravo no IRQ2
        io_wait();
        outb(PIC2_DATA, 2);
        io_wait();
        outb(PIC1_DATA, 0x01); // modo 8086
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();
        outb(PIC1_DATA, 0xff);
        outb(PIC2_DATA, 0xff);
    }
}

/// Define as máscaras (bit 1 = mascarada).
///
/// # Safety
/// Habilitar IRQs exige handlers instalados.
pub unsafe fn set_masks(master: u8, slave: u8) {
    // SAFETY: contrato da função.
    unsafe {
        outb(PIC1_DATA, master);
        outb(PIC2_DATA, slave);
    }
}

/// Lê as máscaras atuais `(mestre, escravo)`.
pub fn masks() -> (u8, u8) {
    // SAFETY: leitura das portas de dados.
    unsafe { (inb(PIC1_DATA), inb(PIC2_DATA)) }
}

/// Sinaliza fim de interrupção para `irq` (0..=15).
///
/// # Safety
/// Deve ser chamado exatamente uma vez por interrupção atendida.
pub unsafe fn end_of_interrupt(irq: u8) {
    // SAFETY: comando EOI não específico.
    unsafe {
        if irq >= 8 {
            outb(PIC2_CMD, 0x20);
        }
        outb(PIC1_CMD, 0x20);
    }
}

/// Mascara tudo (usado ao migrar para APIC).
///
/// # Safety
/// Desliga entrega de IRQs legadas.
pub unsafe fn disable() {
    // SAFETY: contrato da função.
    unsafe { set_masks(0xff, 0xff) };
}

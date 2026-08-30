//! Programmable Interval Timer (8253/8254), canal 0.

/// Frequência de entrada do PIT em Hz.
pub const INPUT_FREQUENCY: u32 = 1_193_182;

/// Divisor para obter aproximadamente `hz` interrupções por segundo.
pub const fn divisor_for(hz: u32) -> u16 {
    let d = INPUT_FREQUENCY / hz;
    if d == 0 {
        1
    } else if d > 0xffff {
        0xffff
    } else {
        d as u16
    }
}

/// Frequência real obtida com `divisor`.
pub const fn actual_frequency(divisor: u16) -> u32 {
    INPUT_FREQUENCY / divisor as u32
}

/// Programa o canal 0 em modo 2 (gerador de taxa) com `hz` Hz.
///
/// # Safety
/// Altera o hardware de temporização da plataforma.
#[cfg(target_arch = "x86_64")]
pub unsafe fn configure_periodic(hz: u32) -> u16 {
    use crate::cpu::outb;
    let d = divisor_for(hz);
    // SAFETY: sequência documentada do PIT: comando, byte baixo, byte alto.
    unsafe {
        outb(0x43, 0b0011_0100); // canal 0, lo/hi, modo 2, binário
        outb(0x40, d as u8);
        outb(0x40, (d >> 8) as u8);
    }
    d
}

/// Inicia uma contagem única no canal 2 (gate via porta 0x61), a partir de `count`.
///
/// # Safety
/// Programa o PIT e a porta 0x61 (também controla o alto-falante).
#[cfg(target_arch = "x86_64")]
pub unsafe fn channel2_one_shot(count: u16) {
    use crate::cpu::{inb, outb};
    // SAFETY: sequência documentada; alto-falante fica desligado (bit 1 = 0).
    unsafe {
        let gate = inb(0x61);
        outb(0x61, (gate & !0x03) | 0x00);
        outb(0x43, 0b1011_0000); // canal 2, lo/hi, modo 0, binário
        outb(0x42, count as u8);
        outb(0x42, (count >> 8) as u8);
        outb(0x61, (gate & !0x02) | 0x01); // gate alto inicia a contagem
    }
}

/// Lê a contagem atual do canal 2 (comando de latch).
#[cfg(target_arch = "x86_64")]
pub fn channel2_read() -> u16 {
    use crate::cpu::{inb, outb};
    // SAFETY: latch + duas leituras do canal 2.
    unsafe {
        outb(0x43, 0b1000_0000);
        let lo = inb(0x42) as u16;
        let hi = inb(0x42) as u16;
        (hi << 8) | lo
    }
}

/// Desliga o gate do canal 2.
///
/// # Safety
/// Altera a porta 0x61.
#[cfg(target_arch = "x86_64")]
pub unsafe fn channel2_stop() {
    use crate::cpu::{inb, outb};
    // SAFETY: apenas limpa os bits de gate/alto-falante.
    unsafe { outb(0x61, inb(0x61) & !0x03) };
}

/// Para o canal 0 (modo 0 com contagem 0: uma única expiração e silêncio).
///
/// # Safety
/// Reprograma o PIT.
#[cfg(target_arch = "x86_64")]
pub unsafe fn channel0_stop() {
    use crate::cpu::outb;
    // SAFETY: sequência documentada do PIT.
    unsafe {
        outb(0x43, 0b0011_0000);
        outb(0x40, 0);
        outb(0x40, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divisors() {
        assert_eq!(divisor_for(1000), 1193);
        assert!((999..=1001).contains(&actual_frequency(divisor_for(1000))));
        assert_eq!(divisor_for(1), 0xffff);
        assert_eq!(divisor_for(10_000_000), 1);
    }
}

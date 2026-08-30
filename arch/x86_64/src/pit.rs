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

//! UART 16550 em portas de E/S (COM1 = 0x3F8).

use crate::cpu::{inb, outb};
use core::fmt;

/// Porta serial.
#[derive(Clone, Copy)]
pub struct SerialPort {
    base: u16,
}

impl SerialPort {
    /// Base de COM1.
    pub const COM1: u16 = 0x3f8;

    /// Cria um handle (não inicializa).
    pub const fn new(base: u16) -> Self {
        SerialPort { base }
    }

    /// Inicializa 115200 8N1 com FIFO e faz um teste de loopback.
    ///
    /// Devolve `false` se o loopback falhou (UART ausente/emulada de forma
    /// incompleta); a porta ainda pode ser usada — a saída será descartada.
    ///
    /// # Safety
    /// Programa o hardware serial da plataforma.
    pub unsafe fn init(&self) -> bool {
        let b = self.base;
        // SAFETY: sequência documentada do 16550.
        unsafe {
            outb(b + 1, 0x00); // sem interrupções
            outb(b + 3, 0x80); // DLAB
            outb(b, 0x01); // divisor 1 → 115200
            outb(b + 1, 0x00);
            outb(b + 3, 0x03); // 8N1
            outb(b + 2, 0xc7); // FIFO on, limpa, 14 bytes
            outb(b + 4, 0x0b); // DTR, RTS, OUT2
            outb(b + 4, 0x1e); // loopback
            outb(b, 0xae);
            let ok = inb(b) == 0xae;
            outb(b + 4, 0x0f); // modo normal
            ok
        }
    }

    #[inline]
    fn can_send(&self) -> bool {
        // SAFETY: leitura do LSR.
        unsafe { inb(self.base + 5) & 0x20 != 0 }
    }

    /// Envia um byte (bloqueante).
    pub fn write_byte(&self, byte: u8) {
        let mut spins = 0u32;
        while !self.can_send() {
            spins += 1;
            if spins > 1_000_000 {
                return; // UART travada/ausente: não bloquear o kernel
            }
            core::hint::spin_loop();
        }
        // SAFETY: escrita no THR.
        unsafe { outb(self.base, byte) };
    }

    /// Envia bytes, convertendo `\n` em `\r\n`.
    pub fn write_bytes(&self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(b);
        }
    }

    /// Lê um byte se disponível.
    pub fn read_byte(&self) -> Option<u8> {
        // SAFETY: leitura do LSR/RBR.
        unsafe {
            if inb(self.base + 5) & 0x01 != 0 {
                Some(inb(self.base))
            } else {
                None
            }
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_bytes(s.as_bytes());
        Ok(())
    }
}

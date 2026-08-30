//! IDT de 256 entradas com interrupt gates de 64 bits.

use crate::gdt::DescriptorTablePointer;

/// Uma entrada da IDT.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    /// Entrada ausente.
    pub const fn missing() -> Self {
        IdtEntry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            zero: 0,
        }
    }

    /// Configura como interrupt gate presente para `handler` (endereço linear).
    ///
    /// `ist` é 0 (nenhuma) ou 1..=7; `dpl` é o privilégio mínimo para `int n`.
    pub fn set_handler(&mut self, handler: u64, selector: u16, ist: u8, dpl: u8) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = selector;
        self.ist = ist & 0x7;
        self.type_attr = 0x80 | ((dpl & 0x3) << 5) | 0x0e;
        self.zero = 0;
    }

    /// Endereço do handler.
    pub fn handler(&self) -> u64 {
        self.offset_low as u64 | (self.offset_mid as u64) << 16 | (self.offset_high as u64) << 32
    }

    /// `true` se presente.
    pub fn is_present(&self) -> bool {
        self.type_attr & 0x80 != 0
    }
}

/// Tabela de descritores de interrupção.
#[repr(C, align(16))]
pub struct InterruptDescriptorTable {
    /// Entradas 0..=255.
    pub entries: [IdtEntry; 256],
}

impl InterruptDescriptorTable {
    /// IDT vazia.
    pub const fn new() -> Self {
        InterruptDescriptorTable {
            entries: [IdtEntry::missing(); 256],
        }
    }

    /// Ponteiro para `lidt`.
    pub fn pointer(&'static self) -> DescriptorTablePointer {
        DescriptorTablePointer {
            limit: (core::mem::size_of::<InterruptDescriptorTable>() - 1) as u16,
            base: self as *const _ as u64,
        }
    }

    /// Carrega a IDT.
    ///
    /// # Safety
    /// Todos os handlers presentes devem ser stubs válidos.
    #[cfg(target_arch = "x86_64")]
    pub unsafe fn load(&'static self) {
        let ptr = self.pointer();
        // SAFETY: contrato da função.
        unsafe { core::arch::asm!("lidt [{}]", in(reg) &ptr, options(nostack)) };
    }
}

impl Default for InterruptDescriptorTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_encoding() {
        assert_eq!(core::mem::size_of::<IdtEntry>(), 16);
        assert_eq!(core::mem::size_of::<InterruptDescriptorTable>(), 4096);
        let mut e = IdtEntry::missing();
        assert!(!e.is_present());
        e.set_handler(0xffff_ffff_8001_2345, 0x08, 1, 0);
        assert!(e.is_present());
        assert_eq!(e.handler(), 0xffff_ffff_8001_2345);
        assert_eq!(e.type_attr, 0x8e);
        assert_eq!(e.ist, 1);
        e.set_handler(0, 0x08, 0, 3);
        assert_eq!(e.type_attr, 0xee);
    }
}

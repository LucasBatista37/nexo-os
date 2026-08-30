//! Inicialização específica de x86_64: GDT/TSS e IDT/handlers.

pub mod apic;
pub mod gdt;
pub mod percpu;
pub mod smp;
pub mod traps;

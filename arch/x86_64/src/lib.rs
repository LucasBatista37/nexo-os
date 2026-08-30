//! Camada x86_64 do Nexo OS.
//!
//! Estruturas de dados (paginação, GDT, IDT) compilam em qualquer host e são
//! testadas com `cargo test`. Código com `asm!`/`global_asm!` é compilado
//! apenas para `x86_64`; os *stubs* de trap e a troca de contexto apenas para
//! `target_os = "none"` (o kernel), pois usam seções ELF.
//!
//! Política de `unsafe`: cada bloco tem um comentário `SAFETY`; a lista
//! completa de invariantes está em `docs/unsafe-inventory.md`.
#![no_std]

pub mod gdt;
pub mod idt;
pub mod paging;
pub mod pit;

#[cfg(target_arch = "x86_64")]
pub mod apic;
#[cfg(target_arch = "x86_64")]
pub mod cpu;
#[cfg(target_arch = "x86_64")]
pub mod ioapic;
#[cfg(target_arch = "x86_64")]
pub mod pic;
#[cfg(target_arch = "x86_64")]
pub mod qemu;
#[cfg(target_arch = "x86_64")]
pub mod serial;

#[cfg(all(target_arch = "x86_64", target_os = "none"))]
pub mod context;
#[cfg(all(target_arch = "x86_64", target_os = "none"))]
pub mod smp;
#[cfg(all(target_arch = "x86_64", target_os = "none"))]
pub mod syscall;
#[cfg(all(target_arch = "x86_64", target_os = "none"))]
pub mod trap;

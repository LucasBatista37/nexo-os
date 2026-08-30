//! Kernel do Nexo OS — release `0.0.1-boot`.
//!
//! Escopo desta versão (Plano Mestre, seção 8): entrada de 64 bits a partir do
//! loader UEFI, logger serial, GDT/TSS/IDT com tratamento de exceções, panic
//! com backtrace simbolizado, memória física (bitmap), memória virtual (mapa/
//! desmapa/permissões/guard page), heap, timer PIT com contador monotônico,
//! duas tarefas cooperativas e uma bateria de auto-testes verificada por serial.
#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
mod klog;
mod boot;
mod cell;
mod console;
mod mm;
mod panic;
mod selftest;
mod symbols;
mod task;
mod time;
mod x86;

use nexo_arch_x86_64::{cpu, qemu};
use nexo_boot_abi::{BootInfo, cmdline_value};

/// Versão do kernel (casa com a tag de release).
pub const VERSION: &str = "0.0.1-boot";
/// Nome provisório do sistema.
pub const NAME: &str = "Nexo OS";

/// Entrada do kernel. O loader coloca `RDI = &BootInfo` (virtual, no physmap),
/// `RSP` no topo da pilha inicial e `CR3` na PML4 descrita em BootInfo.
///
/// # Safety
/// Só pode ser chamada pelo loader, com o estado de máquina descrito em
/// `docs/spec/boot-abi.md` §2 e `boot_info` apontando para um `BootInfo` válido.
#[unsafe(no_mangle)]
pub unsafe extern "sysv64" fn _start(boot_info: *const BootInfo) -> ! {
    // SAFETY: contrato de boot (docs/spec/boot-abi.md): ponteiro válido e imutável.
    let bi = unsafe { &*boot_info };
    kmain(bi)
}

fn kmain(bi: &'static BootInfo) -> ! {
    klog::init();
    kinfo!("{NAME} kernel {VERSION} (x86_64) iniciando");
    boot::init(bi);
    x86::gdt::init();
    x86::traps::init();
    symbols::init();
    mm::init();
    console::init();
    time::init();
    task::init();
    kinfo!("inicializacao concluida em {} ms", time::uptime_ms());

    let cmdline = boot::cmdline();
    let mut ok = true;
    if cmdline_value(cmdline, "selftest") != Some("0") {
        ok = selftest::run();
    }
    match cmdline_value(cmdline, "test") {
        Some("panic") => {
            kinfo!("cenario: panic deliberado");
            panic!("panic deliberado solicitado por test=panic");
        }
        Some("fault") => {
            kinfo!("cenario: falta de pagina nao tratada");
            selftest::deliberate_fault();
        }
        Some("overflow") => {
            kinfo!("cenario: estouro de pilha (guard page + #DF em IST)");
            selftest::deliberate_stack_overflow();
        }
        Some(other) => kwarn!("cenario desconhecido: {other}"),
        None => {}
    }

    kinfo!("NEXO: boot completo ({} ms)", time::uptime_ms());
    console::status("boot completo");
    if boot::test_mode() {
        kinfo!(
            "modo de teste: encerrando QEMU ({})",
            if ok { "sucesso" } else { "falha" }
        );
        qemu::exit(if ok {
            qemu::EXIT_SUCCESS
        } else {
            qemu::EXIT_FAILURE
        });
    }
    idle()
}

fn idle() -> ! {
    let mut last = u64::MAX;
    loop {
        cpu::halt();
        let period = time::uptime_ms() / 10_000;
        if period != last {
            last = period;
            let f = mm::phys::stats();
            let h = mm::heap::stats();
            kdebug!(
                "idle: uptime {} s, {} quadros livres, heap {} KiB em uso",
                time::uptime_ms() / 1000,
                f.free,
                h.used_bytes / 1024
            );
        }
    }
}

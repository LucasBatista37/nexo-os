//! Panic handler com contexto, backtrace por frame pointers e symbolication.

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, Ordering};
use nexo_arch_x86_64::{cpu, qemu};
use nexo_boot_abi::{KERNEL_STACK_BASE, KERNEL_STACK_TOP};

use crate::symbols::Symbolized;

static PANICKING: AtomicBool = AtomicBool::new(false);

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    cpu::disable_interrupts();
    if PANICKING.swap(true, Ordering::SeqCst) {
        kprint!("\n!!! panic aninhado; parando a CPU !!!\n");
        cpu::halt_forever();
    }
    crate::x86::smp::halt_others();
    // SAFETY: o detentor de qualquer lock de saída não voltará a executar.
    unsafe {
        crate::klog::force_unlock();
        crate::console::force_unlock();
    }
    kprint!("\n==================== KERNEL PANIC ====================\n");
    kprint!(
        "cpu      : {}\n",
        crate::x86::percpu::try_current().map_or(0, |c| c.index)
    );
    kprint!("mensagem : {}\n", info.message());
    if let Some(l) = info.location() {
        kprint!("local    : {}:{}:{}\n", l.file(), l.line(), l.column());
    }
    kprint!("uptime   : {} ms\n", crate::time::uptime_ms());
    kprint!("tarefa   : {}\n", crate::task::current_name());
    backtrace(cpu::read_rbp(), None);
    kprint!("======================================================\n");
    crate::console::status("KERNEL PANIC");
    halt_or_exit()
}

/// Encerra o QEMU com falha em modo de teste; senão para a CPU.
pub fn halt_or_exit() -> ! {
    if crate::boot::test_mode() {
        qemu::exit(qemu::EXIT_FAILURE);
    }
    cpu::halt_forever()
}

/// Limites da pilha que contém `addr`, se conhecida.
fn stack_bounds(addr: u64) -> Option<(u64, u64)> {
    if (KERNEL_STACK_BASE..KERNEL_STACK_TOP).contains(&addr) {
        return Some((KERNEL_STACK_BASE, KERNEL_STACK_TOP));
    }
    let df = crate::x86::gdt::double_fault_stack_bounds();
    if (df.0..df.1).contains(&addr) {
        return Some(df);
    }
    if let Some(b) = crate::x86::percpu::df_stack_bounds_containing(addr) {
        return Some(b);
    }
    if let Some(b) = crate::x86::percpu::stack_bounds_containing(addr) {
        return Some(b);
    }
    crate::task::stack_bounds_containing(addr)
}

/// Imprime o backtrace a partir de `rbp` (e, opcionalmente, de um RIP inicial).
/// Frames consecutivos idênticos (recursão) são agrupados.
pub fn backtrace(mut rbp: u64, rip: Option<u64>) {
    kprint!("backtrace:\n");
    let mut i = 0;
    if let Some(pc) = rip {
        kprint!("  #{i:<2} {}\n", Symbolized::pc(pc));
        i += 1;
    }
    let mut last: Option<u64> = None;
    let mut repeats = 0u32;
    for _ in 0..256 {
        let Some((lo, hi)) = stack_bounds(rbp) else {
            break;
        };
        if !rbp.is_multiple_of(8) || rbp < lo || rbp + 16 > hi {
            break;
        }
        // SAFETY: `rbp` está dentro de uma pilha mapeada e alinhado a 8.
        let (saved_rbp, ret) = unsafe { (*(rbp as *const u64), *((rbp + 8) as *const u64)) };
        if ret == 0 {
            break;
        }
        if last == Some(ret) {
            repeats += 1;
        } else {
            if repeats > 0 {
                kprint!("      ... frame anterior repetido {repeats} vezes\n");
                repeats = 0;
            }
            kprint!("  #{i:<2} {}\n", Symbolized::return_address(ret));
            i += 1;
            last = Some(ret);
        }
        if saved_rbp <= rbp {
            break;
        }
        rbp = saved_rbp;
    }
    if repeats > 0 {
        kprint!("      ... frame anterior repetido {repeats} vezes\n");
    }
    if i == 0 {
        kprint!("  (nenhum frame legivel; rbp={rbp:#x})\n");
    }
}

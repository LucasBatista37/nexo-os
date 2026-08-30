//! SDK mínimo: invoca syscalls do Nexo OS. Só funciona em x86_64 (em outros
//! hosts as funções devolvem `NotSupported`, para permitir compilar/lintar).
#![no_std]

use abi::Status;
pub use nexo_syscall_abi as abi;

/// Invoca a syscall `n` com até três argumentos. Devolve `(status, valor)`.
///
/// # Safety
/// O kernel valida ponteiros, mas argumentos incoerentes podem encerrar o processo.
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn raw(n: u64, a0: u64, a1: u64, a2: u64) -> (Status, u64) {
    let status: u64;
    let value: u64;
    // SAFETY: convenção da ABI v0; `rcx`/`r11` são destruídos por `syscall`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") n => status,
            in("rdi") a0,
            in("rsi") a1,
            inlateout("rdx") a2 => value,
            out("rcx") _,
            out("r11") _,
            options(nostack)
        );
    }
    (Status::from_u64(status), value)
}

/// Versão para outros hosts (compila, mas não faz nada).
///
/// # Safety
/// Nunca é perigosa: não invoca nada.
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub unsafe fn raw(_n: u64, _a0: u64, _a1: u64, _a2: u64) -> (Status, u64) {
    (Status::NotSupported, 0)
}

/// Encerra o processo.
pub fn exit(code: i64) -> ! {
    // SAFETY: syscall sem ponteiros.
    unsafe {
        let _ = raw(abi::SYS_EXIT, code as u64, 0, 0);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Escreve no log do kernel.
pub fn log(s: &str) -> Status {
    // SAFETY: ponteiro e tamanho vêm de um `&str` válido.
    unsafe { raw(abi::SYS_LOG, s.as_ptr() as u64, s.len() as u64, 0).0 }
}

/// Relógio monotônico em nanossegundos.
pub fn time_now() -> u64 {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_TIME_NOW, 0, 0, 0).1 }
}

/// Cede a CPU.
pub fn yield_now() {
    // SAFETY: sem ponteiros.
    unsafe {
        let _ = raw(abi::SYS_YIELD, 0, 0, 0);
    }
}

/// Dorme `ns` nanossegundos.
pub fn sleep_ns(ns: u64) {
    // SAFETY: sem ponteiros.
    unsafe {
        let _ = raw(abi::SYS_SLEEP, ns, 0, 0);
    }
}

/// ID do processo.
pub fn get_pid() -> u64 {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_GET_PID, 0, 0, 0).1 }
}

/// Versão da ABI do kernel.
pub fn abi_version() -> u64 {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_ABI_VERSION, 0, 0, 0).1 }
}

/// Informação de depuração (`sel`: 0 CPUs, 1 uptime ms, 2 syscalls do processo).
pub fn debug_info(sel: u64) -> u64 {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_DEBUG_INFO, sel, 0, 0).1 }
}

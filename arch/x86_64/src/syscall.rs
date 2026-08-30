//! Entrada de `syscall`/`sysret` e transição para o modo usuário.
//!
//! Layout exigido dos dados por CPU (`gs`): `[0]` ponteiro `self`,
//! `[8]` topo da pilha de kernel da thread atual, `[16]` rascunho para o RSP
//! do usuário. O kernel executa com `GS_BASE` apontando para esses dados e
//! `KERNEL_GS_BASE = 0`; `swapgs` troca os dois na fronteira usuário/kernel.

use crate::cpu::{rdmsr, wrmsr};
use crate::gdt::{
    KERNEL_CODE_SELECTOR, SYSRET_SELECTOR_BASE, USER_CODE_SELECTOR, USER_DATA_SELECTOR,
};

/// MSR STAR (seletores de `syscall`/`sysret`).
pub const IA32_STAR: u32 = 0xC000_0081;
/// MSR LSTAR (RIP de entrada em modo longo).
pub const IA32_LSTAR: u32 = 0xC000_0082;
/// MSR SFMASK (bits de RFLAGS limpos na entrada).
pub const IA32_FMASK: u32 = 0xC000_0084;
/// EFER.SCE.
pub const EFER_SCE: u64 = 1;

/// Deslocamento de `gs` do topo da pilha de kernel.
pub const GS_KERNEL_RSP: usize = 8;
/// Deslocamento de `gs` do rascunho do RSP de usuário.
pub const GS_USER_RSP: usize = 16;

core::arch::global_asm!(
    r#"
    .section .text.nexo_syscall, "ax", @progbits
    .global nexo_syscall_entry
    .p2align 4
    nexo_syscall_entry:
        swapgs
        mov gs:[16], rsp
        mov rsp, gs:[8]
        push 0x23
        push gs:[16]
        push r11
        push 0x2b
        push rcx
        push 0
        push 0x80
        push rax
        push rbx
        push rcx
        push rdx
        push rsi
        push rdi
        push rbp
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15
        cld
        mov rdi, rsp
        call nexo_syscall_dispatch
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rbp
        pop rdi
        pop rsi
        pop rdx
        pop rcx
        pop rbx
        pop rax
        add rsp, 16
        cli
        mov rcx, qword ptr [rsp]
        mov r11, qword ptr [rsp + 16]
        mov rsp, qword ptr [rsp + 24]
        swapgs
        sysretq
    "#
);

unsafe extern "C" {
    fn nexo_syscall_entry();
}

/// Programa STAR/LSTAR/SFMASK e liga EFER.SCE nesta CPU.
///
/// # Safety
/// Os dados por CPU devem estar em `GS_BASE` com o layout descrito no módulo.
pub unsafe fn init_msrs() {
    let star = ((SYSRET_SELECTOR_BASE as u64) << 48) | ((KERNEL_CODE_SELECTOR as u64) << 32);
    // SAFETY: contrato da função; MSRs existem em qualquer CPU x86_64.
    unsafe {
        wrmsr(IA32_STAR, star);
        wrmsr(IA32_LSTAR, nexo_syscall_entry as *const () as usize as u64);
        // IF | TF | DF | AC limpos na entrada.
        wrmsr(IA32_FMASK, (1 << 9) | (1 << 8) | (1 << 10) | (1 << 18));
        let efer = rdmsr(crate::cpu::IA32_EFER);
        wrmsr(crate::cpu::IA32_EFER, efer | EFER_SCE);
        wrmsr(crate::cpu::IA32_KERNEL_GS_BASE, 0);
    }
}

/// Entra em ring 3 em `entry` com pilha `user_sp` e `RDI = arg`. Nunca retorna.
///
/// # Safety
/// `entry`/`user_sp` devem estar mapeados com `USER` no espaço atual; `gs:[8]`
/// deve conter o topo da pilha de kernel desta thread.
pub unsafe fn enter_user(entry: u64, user_sp: u64, arg: u64) -> ! {
    // SAFETY: contrato da função; `swapgs` deixa GS_BASE = 0 para o usuário e
    // KERNEL_GS_BASE = dados por CPU.
    unsafe {
        core::arch::asm!(
            "cli",
            "swapgs",
            "push {ss}",
            "push {sp}",
            "push {fl}",
            "push {cs}",
            "push {ip}",
            "xor eax, eax",
            "xor ebx, ebx",
            "xor ecx, ecx",
            "xor edx, edx",
            "xor esi, esi",
            "xor ebp, ebp",
            "xor r8d, r8d",
            "xor r9d, r9d",
            "xor r10d, r10d",
            "xor r11d, r11d",
            "xor r12d, r12d",
            "xor r13d, r13d",
            "xor r14d, r14d",
            "xor r15d, r15d",
            "iretq",
            ss = const USER_DATA_SELECTOR as u64,
            sp = in(reg) user_sp,
            fl = const 0x202u64,
            cs = const USER_CODE_SELECTOR as u64,
            ip = in(reg) entry,
            in("rdi") arg,
            options(noreturn)
        )
    }
}

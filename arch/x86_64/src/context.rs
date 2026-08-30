//! Troca de contexto cooperativa entre pilhas de kernel.
//!
//! `nexo_switch_context(prev_sp, next_sp)` salva os registradores
//! callee-saved na pilha atual, grava RSP em `*prev_sp`, carrega `next_sp` e
//! restaura os registradores de lá. Uma pilha nova é preparada por
//! [`prepare_stack`] para que o primeiro `ret` caia no trampolim.

core::arch::global_asm!(
    r#"
    .section .text.nexo_context, "ax", @progbits
    .global nexo_switch_context
    .p2align 4
    nexo_switch_context:
        push rbp
        push rbx
        push r12
        push r13
        push r14
        push r15
        mov [rdi], rsp
        mov rsp, rsi
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx
        pop rbp
        ret

    .global nexo_task_trampoline
    .p2align 4
    nexo_task_trampoline:
        mov rdi, r13
        call r12
        ud2
    "#
);

unsafe extern "C" {
    /// Troca de `*prev_sp` (salvo) para `next_sp`.
    pub fn nexo_switch_context(prev_sp: *mut u64, next_sp: u64);
    fn nexo_task_trampoline();
}

/// Função de entrada de uma tarefa (nunca retorna).
pub type TaskEntry = extern "C" fn(usize) -> !;

/// Prepara uma pilha nova cujo topo (exclusivo, alinhado a 16) é `stack_top`.
/// Devolve o RSP inicial a passar para [`nexo_switch_context`].
///
/// # Safety
/// `[stack_top - 64, stack_top)` deve ser memória gravável exclusiva da tarefa.
pub unsafe fn prepare_stack(stack_top: u64, entry: TaskEntry, arg: usize) -> u64 {
    debug_assert!(stack_top % 16 == 0);
    let sp = stack_top - 7 * 8;
    let frame = sp as *mut u64;
    // SAFETY: contrato da função; layout casado com os `pop`s do assembly.
    unsafe {
        frame.add(0).write(0); // r15
        frame.add(1).write(0); // r14
        frame.add(2).write(arg as u64); // r13 → rdi
        frame.add(3).write(entry as *const () as usize as u64); // r12 → call
        frame.add(4).write(0); // rbx
        frame.add(5).write(0); // rbp
        frame
            .add(6)
            .write(nexo_task_trampoline as *const () as usize as u64); // ret
    }
    sp
}

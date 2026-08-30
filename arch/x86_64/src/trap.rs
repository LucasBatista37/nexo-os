//! Stubs de exceção/interrupção e despacho para o kernel.
//!
//! Os 256 stubs são gerados em assembly (`.altmacro`/`.rept`). Cada um empilha
//! um código de erro fictício quando a CPU não fornece um, o número do vetor,
//! todos os registradores de propósito geral, e chama [`nexo_trap_dispatch`]
//! com um ponteiro para o [`TrapFrame`]. O kernel registra o handler com
//! [`set_handler`].

use core::sync::atomic::{AtomicUsize, Ordering};

/// Estado salvo na entrada de uma trap (layout casado com o assembly).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TrapFrame {
    /// R15.
    pub r15: u64,
    /// R14.
    pub r14: u64,
    /// R13.
    pub r13: u64,
    /// R12.
    pub r12: u64,
    /// R11.
    pub r11: u64,
    /// R10.
    pub r10: u64,
    /// R9.
    pub r9: u64,
    /// R8.
    pub r8: u64,
    /// RBP.
    pub rbp: u64,
    /// RDI.
    pub rdi: u64,
    /// RSI.
    pub rsi: u64,
    /// RDX.
    pub rdx: u64,
    /// RCX.
    pub rcx: u64,
    /// RBX.
    pub rbx: u64,
    /// RAX.
    pub rax: u64,
    /// Vetor (0..=255).
    pub vector: u64,
    /// Código de erro (0 se a CPU não fornece).
    pub error_code: u64,
    /// RIP interrompido.
    pub rip: u64,
    /// CS.
    pub cs: u64,
    /// RFLAGS.
    pub rflags: u64,
    /// RSP interrompido.
    pub rsp: u64,
    /// SS.
    pub ss: u64,
}

core::arch::global_asm!(
    r#"
    .altmacro
    .macro NEXO_STUB n
        .p2align 4
    nexo_trap_stub_\n:
        .if (\n == 8) || (\n == 10) || (\n == 11) || (\n == 12) || (\n == 13) || (\n == 14) || (\n == 17) || (\n == 21) || (\n == 29) || (\n == 30)
        .else
        push 0
        .endif
        push \n
        jmp nexo_trap_common
    .endm
    .macro NEXO_STUB_ENTRY n
        .quad nexo_trap_stub_\n
    .endm

    .section .text.nexo_trap, "ax", @progbits
    .p2align 4
    nexo_trap_common:
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
        mov rax, qword ptr [rsp + 144]
        test rax, 3
        jz 1f
        swapgs
    1:
        cld
        mov rdi, rsp
        call nexo_trap_dispatch
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
        test qword ptr [rsp + 8], 3
        jz 2f
        swapgs
    2:
        iretq

    .set nexo_i, 0
    .rept 256
        NEXO_STUB %nexo_i
        .set nexo_i, nexo_i + 1
    .endr

    .section .rodata.nexo_trap, "a", @progbits
    .global nexo_trap_stub_table
    .p2align 3
    nexo_trap_stub_table:
    .set nexo_i, 0
    .rept 256
        NEXO_STUB_ENTRY %nexo_i
        .set nexo_i, nexo_i + 1
    .endr
    "#
);

unsafe extern "C" {
    static nexo_trap_stub_table: [u64; 256];
}

/// Endereço do stub do vetor `vector`.
pub fn stub_address(vector: u8) -> u64 {
    // SAFETY: tabela definida no assembly acima, somente leitura.
    unsafe { nexo_trap_stub_table[vector as usize] }
}

static HANDLER: AtomicUsize = AtomicUsize::new(0);

/// Tipo do handler registrado pelo kernel.
pub type TrapHandler = fn(&mut TrapFrame);

/// Registra o handler global de traps.
pub fn set_handler(handler: TrapHandler) {
    HANDLER.store(handler as usize, Ordering::Release);
}

#[unsafe(no_mangle)]
extern "C" fn nexo_trap_dispatch(frame: *mut TrapFrame) {
    let h = HANDLER.load(Ordering::Acquire);
    if h == 0 {
        crate::cpu::halt_forever();
    }
    // SAFETY: `h` foi armazenado a partir de um `fn(&mut TrapFrame)` válido;
    // `frame` aponta para o frame empilhado pelo stub e vive até o `iretq`.
    unsafe {
        let f: TrapHandler = core::mem::transmute::<usize, TrapHandler>(h);
        f(&mut *frame);
    }
}

/// `true` se a CPU empilha código de erro para `vector`.
pub const fn has_error_code(vector: u8) -> bool {
    matches!(vector, 8 | 10..=14 | 17 | 21 | 29 | 30)
}

/// Nome da exceção.
pub const fn exception_name(vector: u8) -> &'static str {
    match vector {
        0 => "Divide Error",
        1 => "Debug",
        2 => "NMI",
        3 => "Breakpoint",
        4 => "Overflow",
        5 => "BOUND Range Exceeded",
        6 => "Invalid Opcode",
        7 => "Device Not Available",
        8 => "Double Fault",
        9 => "Coprocessor Segment Overrun",
        10 => "Invalid TSS",
        11 => "Segment Not Present",
        12 => "Stack-Segment Fault",
        13 => "General Protection Fault",
        14 => "Page Fault",
        16 => "x87 Floating-Point",
        17 => "Alignment Check",
        18 => "Machine Check",
        19 => "SIMD Floating-Point",
        20 => "Virtualization",
        21 => "Control Protection",
        28 => "Hypervisor Injection",
        29 => "VMM Communication",
        30 => "Security",
        _ => "Reserved/Unknown",
    }
}

/// Decodificação do código de erro de #PF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFaultError(pub u64);

impl PageFaultError {
    /// Página presente (violação de proteção) vs. não presente.
    pub const fn present(self) -> bool {
        self.0 & 1 != 0
    }
    /// Acesso de escrita.
    pub const fn write(self) -> bool {
        self.0 & 2 != 0
    }
    /// Acesso em modo usuário.
    pub const fn user(self) -> bool {
        self.0 & 4 != 0
    }
    /// Bit reservado setado nas tabelas.
    pub const fn reserved_bit(self) -> bool {
        self.0 & 8 != 0
    }
    /// Busca de instrução (NX).
    pub const fn instruction_fetch(self) -> bool {
        self.0 & 16 != 0
    }
}

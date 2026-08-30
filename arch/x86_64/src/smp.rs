//! Trampolim de inicialização de APs (modo real → protegido → longo).
//!
//! O bloco entre `nexo_trampoline_start` e `nexo_trampoline_end` é
//! **copiado** pelo kernel para [`TRAMPOLINE_PHYS`] (abaixo de 1 MiB); nunca
//! é executado no lugar. Todas as referências internas são constantes
//! calculadas a partir desse endereço fixo, portanto não há relocações.
//! O kernel preenche os parâmetros (PML4, pilha, entrada, argumento) antes
//! de enviar INIT/SIPI.

/// Endereço físico (e de execução) do trampolim. Página reservada pelo kernel.
pub const TRAMPOLINE_PHYS: u64 = 0x8000;
/// Vetor SIPI correspondente (`TRAMPOLINE_PHYS >> 12`).
pub const SIPI_VECTOR: u8 = (TRAMPOLINE_PHYS >> 12) as u8;

core::arch::global_asm!(
    r#"
    .set NEXO_TRAMP_BASE, 0x8000

    .section .rodata.nexo_trampoline, "a"
    .global nexo_trampoline_start
    .global nexo_trampoline_end
    .global nexo_tramp_param_pml4
    .global nexo_tramp_param_stack
    .global nexo_tramp_param_entry
    .global nexo_tramp_param_arg
    .p2align 12
    .code16
nexo_trampoline_start:
    jmp nexo_tramp_code16

    .p2align 3
nexo_tramp_gdt:
    .quad 0
    .quad 0x00cf9a000000ffff
    .quad 0x00cf92000000ffff
    .quad 0x00af9a000000ffff
nexo_tramp_gdt_end:
nexo_tramp_gdt_ptr:
    .word nexo_tramp_gdt_end - nexo_tramp_gdt - 1
    .long NEXO_TRAMP_BASE + (nexo_tramp_gdt - nexo_trampoline_start)
    .p2align 3
nexo_tramp_param_pml4:
    .quad 0
nexo_tramp_param_stack:
    .quad 0
nexo_tramp_param_entry:
    .quad 0
nexo_tramp_param_arg:
    .quad 0

    // Deslocamentos absolutos (símbolos já definidos acima → constantes).
    .set NEXO_OFF_GDT_PTR, nexo_tramp_gdt_ptr - nexo_trampoline_start
    .set NEXO_OFF_PML4, nexo_tramp_param_pml4 - nexo_trampoline_start
    .set NEXO_OFF_STACK, nexo_tramp_param_stack - nexo_trampoline_start
    .set NEXO_OFF_ENTRY, nexo_tramp_param_entry - nexo_trampoline_start
    .set NEXO_OFF_ARG, nexo_tramp_param_arg - nexo_trampoline_start

    .code16
nexo_tramp_code16:
    cli
    cld
    mov ax, cs
    mov ds, ax
    mov es, ax
    mov ss, ax
    lgdt [NEXO_OFF_GDT_PTR]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    .byte 0x66, 0xea
    .long NEXO_TRAMP_BASE + (nexo_tramp_pm32 - nexo_trampoline_start)
    .word 0x08

    .code32
nexo_tramp_pm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax
    mov eax, cr4
    or eax, (1 << 5)
    mov cr4, eax
    mov eax, dword ptr [NEXO_TRAMP_BASE + NEXO_OFF_PML4]
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8) | (1 << 11)
    wrmsr
    mov eax, cr0
    or eax, 0x80010001
    mov cr0, eax
    .byte 0xea
    .long NEXO_TRAMP_BASE + (nexo_tramp_lm64 - nexo_trampoline_start)
    .word 0x18

    .code64
nexo_tramp_lm64:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax
    // Sintaxe Intel: `[expr]` com expressao constante = endereco absoluto (SIB sem base).
    mov rsp, qword ptr [NEXO_TRAMP_BASE + NEXO_OFF_STACK]
    mov rdi, qword ptr [NEXO_TRAMP_BASE + NEXO_OFF_ARG]
    mov rax, qword ptr [NEXO_TRAMP_BASE + NEXO_OFF_ENTRY]
    xor ebp, ebp
    jmp rax
nexo_trampoline_end:
    "#
);

unsafe extern "C" {
    static nexo_trampoline_start: u8;
    static nexo_trampoline_end: u8;
    static nexo_tramp_param_pml4: u64;
    static nexo_tramp_param_stack: u64;
    static nexo_tramp_param_entry: u64;
    static nexo_tramp_param_arg: u64;
}

/// Parâmetros que o kernel grava no trampolim.
#[derive(Clone, Copy, Debug)]
pub struct TrampolineParams {
    /// Endereço físico da PML4 (32 bits: as tabelas iniciais ficam abaixo de 4 GiB).
    pub pml4: u32,
    /// Topo da pilha do AP (virtual, no espaço do kernel).
    pub stack_top: u64,
    /// Função de entrada (`extern "sysv64" fn(usize) -> !`).
    pub entry: u64,
    /// Argumento entregue em `RDI`.
    pub arg: u64,
}

/// Bytes do trampolim (imagem a copiar).
pub fn trampoline_image() -> &'static [u8] {
    // SAFETY: símbolos delimitam um bloco contíguo e somente leitura definido acima.
    unsafe {
        let start = &raw const nexo_trampoline_start;
        let end = &raw const nexo_trampoline_end;
        core::slice::from_raw_parts(start, end as usize - start as usize)
    }
}

/// Deslocamento de cada parâmetro dentro da imagem: `(pml4, stack, entry, arg)`.
pub fn param_offsets() -> (usize, usize, usize, usize) {
    // Apenas endereços de símbolos; nada é lido.
    let base = &raw const nexo_trampoline_start as usize;
    (
        &raw const nexo_tramp_param_pml4 as usize - base,
        &raw const nexo_tramp_param_stack as usize - base,
        &raw const nexo_tramp_param_entry as usize - base,
        &raw const nexo_tramp_param_arg as usize - base,
    )
}

/// Grava a imagem do trampolim com os parâmetros em `dest` (que deve ser a
/// visão do kernel do endereço físico [`TRAMPOLINE_PHYS`], com 4 KiB).
///
/// # Safety
/// `dest` deve apontar para uma página gravável exclusiva do trampolim.
pub unsafe fn install(dest: *mut u8, params: TrampolineParams) -> usize {
    let img = trampoline_image();
    let (o_pml4, o_stack, o_entry, o_arg) = param_offsets();
    // SAFETY: contrato da função; a imagem cabe em uma página (verificado pelo chamador).
    unsafe {
        core::ptr::copy_nonoverlapping(img.as_ptr(), dest, img.len());
        dest.add(o_pml4)
            .cast::<u64>()
            .write_unaligned(params.pml4 as u64);
        dest.add(o_stack)
            .cast::<u64>()
            .write_unaligned(params.stack_top);
        dest.add(o_entry)
            .cast::<u64>()
            .write_unaligned(params.entry);
        dest.add(o_arg).cast::<u64>().write_unaligned(params.arg);
    }
    img.len()
}

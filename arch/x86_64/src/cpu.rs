//! Acesso a registradores de controle, MSRs, portas de E/S e instruções
//! privilegiadas. Tudo aqui é `x86_64`-only.

use core::arch::asm;

/// MSR EFER.
pub const IA32_EFER: u32 = 0xC000_0080;
/// EFER.NXE — habilita o bit NX nas tabelas de página.
pub const EFER_NXE: u64 = 1 << 11;
/// EFER.LMA — modo longo ativo.
pub const EFER_LMA: u64 = 1 << 10;
/// CR0.WP — protege páginas somente-leitura contra escrita em ring 0.
pub const CR0_WP: u64 = 1 << 16;
/// CR0.PG — paginação.
pub const CR0_PG: u64 = 1 << 31;
/// CR4.PGE — páginas globais.
pub const CR4_PGE: u64 = 1 << 7;
/// MSR com a base do segmento GS (dados por CPU).
pub const IA32_GS_BASE: u32 = 0xC000_0101;
/// MSR com a base de GS alternativa (`swapgs`).
pub const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

/// Escreve um byte em uma porta.
///
/// # Safety
/// E/S de porta pode ter qualquer efeito colateral no hardware.
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    // SAFETY: contrato da função.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags))
    };
}

/// Lê um byte de uma porta.
///
/// # Safety
/// Ver [`outb`].
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    // SAFETY: contrato da função.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") v, options(nomem, nostack, preserves_flags))
    };
    v
}

/// Escreve 32 bits em uma porta.
///
/// # Safety
/// Ver [`outb`].
#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    // SAFETY: contrato da função.
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags))
    };
}

/// Lê 32 bits de uma porta.
///
/// # Safety
/// Ver [`outb`].
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let v: u32;
    // SAFETY: contrato da função.
    unsafe {
        asm!("in eax, dx", in("dx") port, out("eax") v, options(nomem, nostack, preserves_flags))
    };
    v
}

/// Lê um MSR.
///
/// # Safety
/// MSR inexistente causa #GP.
#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let (lo, hi): (u32, u32);
    // SAFETY: contrato da função.
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags))
    };
    ((hi as u64) << 32) | lo as u64
}

/// Escreve um MSR.
///
/// # Safety
/// Pode alterar o modo de operação da CPU.
#[inline]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    // SAFETY: contrato da função.
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nomem, nostack, preserves_flags))
    };
}

/// Lê CR0.
#[inline]
pub fn read_cr0() -> u64 {
    let v: u64;
    // SAFETY: leitura de registrador de controle é livre de efeitos colaterais.
    unsafe { asm!("mov {}, cr0", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// Escreve CR0.
///
/// # Safety
/// Altera paginação/proteção.
#[inline]
pub unsafe fn write_cr0(v: u64) {
    // SAFETY: contrato da função.
    unsafe { asm!("mov cr0, {}", in(reg) v, options(nomem, nostack, preserves_flags)) };
}

/// Lê CR2 (endereço da última falta de página).
#[inline]
pub fn read_cr2() -> u64 {
    let v: u64;
    // SAFETY: leitura sem efeitos colaterais.
    unsafe { asm!("mov {}, cr2", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// Lê CR3 (base física da PML4 + flags PCD/PWT).
#[inline]
pub fn read_cr3() -> u64 {
    let v: u64;
    // SAFETY: leitura sem efeitos colaterais.
    unsafe { asm!("mov {}, cr3", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// Escreve CR3 (troca de espaço de endereçamento; invalida TLB não-global).
///
/// # Safety
/// As novas tabelas devem mapear o código em execução e a pilha.
#[inline]
pub unsafe fn write_cr3(v: u64) {
    // SAFETY: contrato da função.
    unsafe { asm!("mov cr3, {}", in(reg) v, options(nostack, preserves_flags)) };
}

/// Lê CR4.
#[inline]
pub fn read_cr4() -> u64 {
    let v: u64;
    // SAFETY: leitura sem efeitos colaterais.
    unsafe { asm!("mov {}, cr4", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// Escreve CR4.
///
/// # Safety
/// Altera recursos da CPU.
#[inline]
pub unsafe fn write_cr4(v: u64) {
    // SAFETY: contrato da função.
    unsafe { asm!("mov cr4, {}", in(reg) v, options(nomem, nostack, preserves_flags)) };
}

/// Invalida a entrada de TLB de `addr`.
#[inline]
pub fn invlpg(addr: u64) {
    // SAFETY: invalidar TLB é sempre seguro (apenas custo).
    unsafe { asm!("invlpg [{}]", in(reg) addr, options(nostack, preserves_flags)) };
}

/// Invalida todo o TLB (recarrega CR3).
#[inline]
pub fn flush_tlb_all() {
    // SAFETY: reescrever o mesmo CR3 é seguro.
    unsafe { write_cr3(read_cr3()) };
}

/// `hlt` — aguarda a próxima interrupção.
#[inline]
pub fn halt() {
    // SAFETY: instrução sem efeitos além de parar até uma interrupção.
    unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
}

/// Para para sempre com interrupções desabilitadas.
pub fn halt_forever() -> ! {
    loop {
        disable_interrupts();
        halt();
    }
}

/// `cli`.
#[inline]
pub fn disable_interrupts() {
    // SAFETY: apenas mascara interrupções.
    unsafe { asm!("cli", options(nomem, nostack)) };
}

/// `sti`.
///
/// # Safety
/// Uma IDT válida deve estar carregada.
#[inline]
pub unsafe fn enable_interrupts() {
    // SAFETY: contrato da função.
    unsafe { asm!("sti", options(nomem, nostack)) };
}

/// Lê RFLAGS.
#[inline]
pub fn read_rflags() -> u64 {
    let v: u64;
    // SAFETY: pushfq/pop usa a pilha de forma balanceada.
    unsafe { asm!("pushfq", "pop {}", out(reg) v, options(nomem, preserves_flags)) };
    v
}

/// `true` se IF está setado.
#[inline]
pub fn interrupts_enabled() -> bool {
    read_rflags() & (1 << 9) != 0
}

/// Executa `f` com interrupções desabilitadas, restaurando o estado anterior.
pub fn without_interrupts<R>(f: impl FnOnce() -> R) -> R {
    let was_enabled = interrupts_enabled();
    disable_interrupts();
    let r = f();
    if was_enabled {
        // SAFETY: estavam habilitadas antes, logo a IDT é válida.
        unsafe { enable_interrupts() };
    }
    r
}

/// Lê RBP (base do frame atual).
#[inline(always)]
pub fn read_rbp() -> u64 {
    let v: u64;
    // SAFETY: leitura de registrador.
    unsafe { asm!("mov {}, rbp", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// Lê RSP.
#[inline(always)]
pub fn read_rsp() -> u64 {
    let v: u64;
    // SAFETY: leitura de registrador.
    unsafe { asm!("mov {}, rsp", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// Define a base de GS (ponteiro para os dados desta CPU).
///
/// # Safety
/// Código que usa `gs:` passa a ler a partir de `base`.
#[inline]
pub unsafe fn write_gs_base(base: u64) {
    // SAFETY: contrato da função.
    unsafe { wrmsr(IA32_GS_BASE, base) };
}

/// Lê a base de GS.
#[inline]
pub fn read_gs_base() -> u64 {
    // SAFETY: leitura de MSR sempre válida em modo longo.
    unsafe { rdmsr(IA32_GS_BASE) }
}

/// Lê o `u64` em `gs:[0]` (ponteiro `self` da estrutura por CPU).
#[inline(always)]
pub fn read_gs_self() -> u64 {
    let v: u64;
    // SAFETY: leitura relativa a GS; o chamador garante base configurada.
    unsafe { asm!("mov {}, gs:[0]", out(reg) v, options(nostack, preserves_flags, readonly)) };
    v
}

/// Lê o seletor CS atual.
#[inline]
pub fn read_cs() -> u16 {
    let v: u16;
    // SAFETY: leitura de registrador de segmento.
    unsafe { asm!("mov {0:x}, cs", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// Contador de ciclos.
#[inline]
pub fn rdtsc() -> u64 {
    let (lo, hi): (u32, u32);
    // SAFETY: rdtsc não tem efeitos colaterais.
    unsafe {
        asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags))
    };
    ((hi as u64) << 32) | lo as u64
}

/// Resultado de `cpuid`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuidResult {
    /// EAX.
    pub eax: u32,
    /// EBX.
    pub ebx: u32,
    /// ECX.
    pub ecx: u32,
    /// EDX.
    pub edx: u32,
}

/// Executa `cpuid`.
pub fn cpuid(leaf: u32, subleaf: u32) -> CpuidResult {
    let r = core::arch::x86_64::__cpuid_count(leaf, subleaf);
    CpuidResult {
        eax: r.eax,
        ebx: r.ebx,
        ecx: r.ecx,
        edx: r.edx,
    }
}

/// `true` se a CPU suporta o bit NX.
pub fn supports_nx() -> bool {
    cpuid(0x8000_0000, 0).eax >= 0x8000_0001 && cpuid(0x8000_0001, 0).edx & (1 << 20) != 0
}

/// `true` se a CPU suporta páginas de 1 GiB.
pub fn supports_1g_pages() -> bool {
    cpuid(0x8000_0000, 0).eax >= 0x8000_0001 && cpuid(0x8000_0001, 0).edx & (1 << 26) != 0
}

/// String do fabricante (12 bytes ASCII).
pub fn vendor() -> [u8; 12] {
    let r = cpuid(0, 0);
    let mut v = [0u8; 12];
    v[0..4].copy_from_slice(&r.ebx.to_le_bytes());
    v[4..8].copy_from_slice(&r.edx.to_le_bytes());
    v[8..12].copy_from_slice(&r.ecx.to_le_bytes());
    v
}

/// Habilita EFER.NXE (se suportado). Devolve `true` em sucesso.
///
/// # Safety
/// Deve ser chamado antes de instalar tabelas com o bit NX.
pub unsafe fn enable_nx() -> bool {
    if !supports_nx() {
        return false;
    }
    // SAFETY: EFER existe em qualquer CPU de 64 bits; só ligamos NXE.
    unsafe {
        let efer = rdmsr(IA32_EFER);
        if efer & EFER_NXE == 0 {
            wrmsr(IA32_EFER, efer | EFER_NXE);
        }
    }
    true
}

/// `true` se EFER.NXE está ativo.
pub fn nx_enabled() -> bool {
    // SAFETY: leitura de EFER.
    unsafe { rdmsr(IA32_EFER) & EFER_NXE != 0 }
}

/// Liga CR0.WP.
///
/// # Safety
/// Escritas em páginas somente-leitura passam a falhar em ring 0.
pub unsafe fn enable_write_protect() {
    // SAFETY: contrato da função.
    unsafe { write_cr0(read_cr0() | CR0_WP) };
}

/// Liga SSE/FXSR (CR0.EM←0, CR0.MP←1; CR4.OSFXSR | CR4.OSXMMEXCPT): processos de usuário
/// passam a poder usar FPU/XMM. O kernel continua *soft-float* (nunca toca XMM); o estado do
/// usuário é preservado por FXSAVE/FXRSTOR por thread na troca de contexto. Uma vez por CPU.
pub fn enable_sse() {
    // SAFETY: bits arquiteturais padrão do x86_64; idempotente.
    unsafe {
        write_cr0((read_cr0() & !(1 << 2)) | (1 << 1)); // EM = 0, MP = 1
        write_cr4(read_cr4() | (1 << 9) | (1 << 10)); // OSFXSR | OSXMMEXCPT
    }
}

/// Área de FXSAVE64/FXRSTOR64: 512 bytes alinhados a 16.
#[repr(C, align(16))]
pub struct FxArea(pub [u8; 512]);

impl FxArea {
    /// Estado limpo de FPU/SSE (FCW 0x037F, MXCSR 0x1F80, registradores zerados) — o estado
    /// inicial de toda thread, para que nada vaze de um processo para outro.
    pub const fn new() -> Self {
        let mut a = [0u8; 512];
        a[0] = 0x7f; // FCW = 0x037F (precisão dupla estendida, exceções mascaradas)
        a[1] = 0x03;
        a[24] = 0x80; // MXCSR = 0x1F80 (exceções SSE mascaradas)
        a[25] = 0x1f;
        FxArea(a)
    }
}

impl Default for FxArea {
    fn default() -> Self {
        Self::new()
    }
}

/// Salva o estado FPU/SSE corrente em `a`.
pub fn fxsave(a: &mut FxArea) {
    // SAFETY: área de 512 B alinhada a 16 por construção; CR4.OSFXSR ligado no boot.
    unsafe {
        core::arch::asm!("fxsave64 [{}]", in(reg) a.0.as_mut_ptr(), options(nostack, preserves_flags));
    }
}

/// Restaura o estado FPU/SSE a partir de `a`.
pub fn fxrstor(a: &FxArea) {
    // SAFETY: área válida escrita por `fxsave` ou `FxArea::new`; CR4.OSFXSR ligado.
    unsafe {
        core::arch::asm!("fxrstor64 [{}]", in(reg) a.0.as_ptr(), options(nostack, preserves_flags));
    }
}

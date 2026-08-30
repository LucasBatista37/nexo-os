//! IDT, handlers de exceção/IRQ e sondas de falta esperada.
//!
//! - Exceções não esperadas são fatais: registradores, CR2, RIP simbolizado
//!   e backtrace são impressos; em modo de teste o QEMU encerra com falha.
//! - `#BP` incrementa um contador e retorna (usado em auto-teste).
//! - `#PF` consulta a sonda ([`probe`]): se a falta era esperada, o handler
//!   redireciona RIP para o ponto de retomada, registrando CR2 e o código de
//!   erro — a mesma técnica de *exception tables* usada para validar acessos.
//! - `#DF` roda em IST1 (pilha própria), o que permite diagnosticar estouro
//!   de pilha através da guard page.

use core::sync::atomic::{AtomicU64, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::gdt::KERNEL_CODE_SELECTOR;
use nexo_arch_x86_64::idt::InterruptDescriptorTable;
use nexo_arch_x86_64::trap::{self, PageFaultError, TrapFrame, exception_name};

use super::apic::vectors;
use nexo_sync::SpinLock;

use crate::cell::StaticCell;
use crate::symbols::Symbolized;

static IDT: StaticCell<InterruptDescriptorTable> = StaticCell::new(InterruptDescriptorTable::new());
static BREAKPOINTS: AtomicU64 = AtomicU64::new(0);
static EXCEPTIONS: AtomicU64 = AtomicU64::new(0);
static IOAPIC_TEST_IRQS: AtomicU64 = AtomicU64::new(0);
static IPIS: AtomicU64 = AtomicU64::new(0);
static SPURIOUS: AtomicU64 = AtomicU64::new(0);
static TIMER_IRQS: AtomicU64 = AtomicU64::new(0);

/// Instala os 256 vetores e carrega a IDT.
pub fn init() {
    // SAFETY: inicialização única em uma CPU; a tabela nunca mais é escrita.
    let idt = unsafe { &mut *IDT.as_ptr() };
    for v in 0..=255u8 {
        let ist = if v == 8 { 1 } else { 0 };
        idt.entries[v as usize].set_handler(trap::stub_address(v), KERNEL_CODE_SELECTOR, ist, 0);
    }
    trap::set_handler(handle_trap);
    // SAFETY: todos os handlers apontam para stubs válidos gerados em assembly.
    unsafe { (*IDT.as_ptr()).load() };
    kinfo!(
        "idt: 256 vetores instalados; #DF em IST1; timer {:#x}, IPIs {:#x}..{:#x}, espuria {:#x}",
        vectors::TIMER,
        vectors::RESCHED,
        vectors::TLB_FLUSH,
        vectors::SPURIOUS
    );
}

/// Carrega a IDT (compartilhada) na CPU atual — usado pelas APs.
///
/// # Safety
/// `init` deve ter sido executado pela BSP.
pub unsafe fn load_idt() {
    // SAFETY: tabela preenchida por `init`, nunca mais escrita.
    unsafe { (*IDT.as_ptr()).load() };
}

/// Interrupções recebidas pela entrada de teste do I/O APIC.
pub fn ioapic_test_count() -> u64 {
    IOAPIC_TEST_IRQS.load(Ordering::Relaxed)
}

/// IPIs recebidas (RESCHED + TLB_FLUSH).
pub fn ipi_count() -> u64 {
    IPIS.load(Ordering::Relaxed)
}

/// Interrupções espúrias.
pub fn spurious_count() -> u64 {
    SPURIOUS.load(Ordering::Relaxed)
}

/// Interrupções do timer do LAPIC.
pub fn timer_irq_count() -> u64 {
    TIMER_IRQS.load(Ordering::Relaxed)
}

/// Número de `#BP` tratados.
pub fn breakpoint_count() -> u64 {
    BREAKPOINTS.load(Ordering::Relaxed)
}

/// Número total de exceções tratadas (inclui sondas).
pub fn exception_count() -> u64 {
    EXCEPTIONS.load(Ordering::Relaxed)
}

fn handle_trap(frame: &mut TrapFrame) {
    let vector = frame.vector as u8;
    match vector {
        3 => {
            BREAKPOINTS.fetch_add(1, Ordering::Relaxed);
            EXCEPTIONS.fetch_add(1, Ordering::Relaxed);
            kdebug!("#BP em {}", Symbolized::pc(frame.rip));
        }
        14 => page_fault(frame),
        8 => double_fault(frame),
        vectors::TIMER => {
            TIMER_IRQS.fetch_add(1, Ordering::Relaxed);
            match super::percpu::try_current() {
                Some(c) => {
                    c.timer_irqs.fetch_add(1, Ordering::Relaxed);
                    if c.index == 0 {
                        crate::time::tick();
                    }
                }
                None => crate::time::tick(),
            }
            super::apic::eoi();
        }
        vectors::IOAPIC_TEST => {
            IOAPIC_TEST_IRQS.fetch_add(1, Ordering::Relaxed);
            super::apic::eoi();
        }
        vectors::RESCHED => {
            IPIS.fetch_add(1, Ordering::Relaxed);
            if let Some(c) = super::percpu::try_current() {
                c.ipis.fetch_add(1, Ordering::Relaxed);
            }
            super::apic::eoi();
        }
        vectors::TLB_FLUSH => {
            IPIS.fetch_add(1, Ordering::Relaxed);
            if let Some(c) = super::percpu::try_current() {
                c.ipis.fetch_add(1, Ordering::Relaxed);
            }
            cpu::flush_tlb_all();
            super::apic::eoi();
        }
        vectors::HALT => cpu::halt_forever(),
        vectors::APIC_ERROR => {
            kwarn!("apic: erro ESR={:#x}", super::apic::lapic().error_status());
            super::apic::eoi();
        }
        vectors::SPURIOUS => {
            SPURIOUS.fetch_add(1, Ordering::Relaxed);
            // Espúria: sem EOI.
        }
        v if (vectors::PIC_BASE..vectors::PIC_BASE + 16).contains(&v) => {
            // PIC está mascarado; qualquer coisa aqui é espúria (IRQ7/15).
            SPURIOUS.fetch_add(1, Ordering::Relaxed);
            kdebug!("irq legada {} do PIC ignorada", v - vectors::PIC_BASE);
        }
        _ => fatal(frame, "excecao nao tratada"),
    }
}

fn page_fault(frame: &mut TrapFrame) {
    EXCEPTIONS.fetch_add(1, Ordering::Relaxed);
    let cr2 = cpu::read_cr2();
    let err = PageFaultError(frame.error_code);
    let mut probe = PROBE.lock();
    if let Some(p) = probe.as_mut()
        && cr2 & !0xfff == p.page
    {
        p.hit = Some(Hit {
            cr2,
            error: err,
            rip: frame.rip,
        });
        frame.rip = RESUME_RIP.load(Ordering::Relaxed);
        if p.kind == ProbeKind::Exec {
            // `call` empilhou o endereço de retorno; descarta-o.
            frame.rsp += 8;
        }
        return;
    }
    drop(probe);
    cpu::disable_interrupts();
    // SAFETY: caminho fatal; ninguém mais usará os locks de saída.
    unsafe {
        crate::klog::force_unlock();
        crate::console::force_unlock();
    }
    kprint!("\n==================== EXCEPTION ====================\n");
    kprint!(
        "PAGE FAULT em {:#018x}: {} {} {} {}{}\n",
        cr2,
        if err.present() {
            "protecao"
        } else {
            "nao-presente"
        },
        if err.write() { "escrita" } else { "leitura" },
        if err.user() { "usuario" } else { "kernel" },
        if err.instruction_fetch() {
            "busca-de-instrucao "
        } else {
            ""
        },
        if err.reserved_bit() {
            "bit-reservado "
        } else {
            ""
        },
    );
    dump_and_stop(frame);
}

fn double_fault(frame: &mut TrapFrame) {
    EXCEPTIONS.fetch_add(1, Ordering::Relaxed);
    cpu::disable_interrupts();
    // SAFETY: caminho fatal.
    unsafe {
        crate::klog::force_unlock();
        crate::console::force_unlock();
    }
    kprint!("\n==================== EXCEPTION ====================\n");
    let in_guard = frame.rsp < nexo_boot_abi::KERNEL_STACK_BASE
        && frame.rsp >= nexo_boot_abi::KERNEL_STACK_BASE - 0x1000;
    kprint!(
        "DOUBLE FAULT (rsp={:#x}){}\n",
        frame.rsp,
        if in_guard || crate::task::stack_bounds_containing(frame.rsp).is_none() {
            " — provavel estouro de pilha (guard page atingida)"
        } else {
            ""
        }
    );
    dump_and_stop(frame);
}

fn fatal(frame: &mut TrapFrame, why: &str) -> ! {
    EXCEPTIONS.fetch_add(1, Ordering::Relaxed);
    cpu::disable_interrupts();
    // SAFETY: caminho fatal.
    unsafe {
        crate::klog::force_unlock();
        crate::console::force_unlock();
    }
    kprint!("\n==================== EXCEPTION ====================\n");
    kprint!(
        "{}: {} (#{})\n",
        why,
        exception_name(frame.vector as u8),
        frame.vector
    );
    dump_and_stop(frame)
}

fn dump_and_stop(frame: &TrapFrame) -> ! {
    super::smp::halt_others();
    kprint!(
        "cpu      : {}\n",
        super::percpu::try_current().map_or(0, |c| c.index)
    );
    kprint!(
        "vetor    : {} ({})\n",
        frame.vector,
        exception_name(frame.vector as u8)
    );
    kprint!("erro     : {:#x}\n", frame.error_code);
    kprint!("rip      : {}\n", Symbolized::pc(frame.rip));
    kprint!(
        "cs:rflags: {:#x}:{:#x}   ss:rsp: {:#x}:{:#x}\n",
        frame.cs,
        frame.rflags,
        frame.ss,
        frame.rsp
    );
    kprint!(
        "rax {:#018x} rbx {:#018x} rcx {:#018x} rdx {:#018x}\n",
        frame.rax,
        frame.rbx,
        frame.rcx,
        frame.rdx
    );
    kprint!(
        "rsi {:#018x} rdi {:#018x} rbp {:#018x} r8  {:#018x}\n",
        frame.rsi,
        frame.rdi,
        frame.rbp,
        frame.r8
    );
    kprint!(
        "r9  {:#018x} r10 {:#018x} r11 {:#018x} r12 {:#018x}\n",
        frame.r9,
        frame.r10,
        frame.r11,
        frame.r12
    );
    kprint!(
        "r13 {:#018x} r14 {:#018x} r15 {:#018x}\n",
        frame.r13,
        frame.r14,
        frame.r15
    );
    kprint!(
        "cr2 {:#018x} cr3 {:#018x} cr0 {:#018x}\n",
        cpu::read_cr2(),
        cpu::read_cr3(),
        cpu::read_cr0()
    );
    kprint!(
        "uptime   : {} ms   tarefa: {}\n",
        crate::time::uptime_ms(),
        crate::task::current_name()
    );
    crate::panic::backtrace(frame.rbp, Some(frame.rip));
    kprint!("===================================================\n");
    crate::console::status("EXCEPTION");
    crate::panic::halt_or_exit()
}

// ---------------------------------------------------------------------------
// Sondas de falta esperada
// ---------------------------------------------------------------------------

/// Tipo de acesso sondado.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProbeKind {
    /// Leitura de 8 bytes.
    Read,
    /// Escrita (reescreve o valor lido, sem alterar conteúdo).
    Write,
    /// Execução (`call` para o endereço, que deve conter `ret`).
    Exec,
}

#[derive(Clone, Copy)]
struct Hit {
    cr2: u64,
    error: PageFaultError,
    rip: u64,
}

struct Probe {
    page: u64,
    kind: ProbeKind,
    hit: Option<Hit>,
}

static PROBE: SpinLock<Option<Probe>> = SpinLock::new(None);
static RESUME_RIP: AtomicU64 = AtomicU64::new(0);

/// Resultado de uma sonda.
#[derive(Clone, Copy, Debug)]
pub struct ProbeResult {
    /// `true` se houve falta de página no endereço sondado.
    pub faulted: bool,
    /// CR2 observado.
    pub cr2: u64,
    /// Código de erro observado.
    pub error: PageFaultError,
    /// RIP da instrução que falhou.
    pub fault_rip: u64,
}

/// Executa um acesso a `addr` e informa se ele causou `#PF`.
///
/// Uma falta em qualquer *outra* página continua fatal.
pub fn probe(kind: ProbeKind, addr: u64) -> ProbeResult {
    cpu::without_interrupts(|| {
        *PROBE.lock() = Some(Probe {
            page: addr & !0xfff,
            kind,
            hit: None,
        });
        let faulted: u64;
        let slot = RESUME_RIP.as_ptr();
        // SAFETY: o handler de #PF redireciona RIP para o rótulo `2:` quando a
        // falta ocorre na página sondada; nos demais casos o acesso é válido
        // (páginas mapeadas do kernel). Para `Exec`, a página-alvo contém `ret`.
        unsafe {
            match kind {
                ProbeKind::Read => core::arch::asm!(
                    "lea {tmp}, [rip + 2f]",
                    "mov [{slot}], {tmp}",
                    "mov {tmp}, [{addr}]",
                    "xor {res:e}, {res:e}",
                    "jmp 3f",
                    "2:",
                    "mov {res:e}, 1",
                    "3:",
                    addr = in(reg) addr,
                    slot = in(reg) slot,
                    tmp = out(reg) _,
                    res = out(reg) faulted,
                    options(nostack),
                ),
                ProbeKind::Write => core::arch::asm!(
                    "lea {tmp}, [rip + 2f]",
                    "mov [{slot}], {tmp}",
                    "mov {tmp}, [{addr}]",
                    "mov [{addr}], {tmp}",
                    "xor {res:e}, {res:e}",
                    "jmp 3f",
                    "2:",
                    "mov {res:e}, 1",
                    "3:",
                    addr = in(reg) addr,
                    slot = in(reg) slot,
                    tmp = out(reg) _,
                    res = out(reg) faulted,
                    options(nostack),
                ),
                ProbeKind::Exec => core::arch::asm!(
                    "lea {tmp}, [rip + 2f]",
                    "mov [{slot}], {tmp}",
                    "call {addr}",
                    "xor {res:e}, {res:e}",
                    "jmp 3f",
                    "2:",
                    "mov {res:e}, 1",
                    "3:",
                    addr = in(reg) addr,
                    slot = in(reg) slot,
                    tmp = out(reg) _,
                    res = out(reg) faulted,
                ),
            }
        }
        let p = PROBE.lock().take();
        let hit = p.and_then(|p| p.hit);
        ProbeResult {
            faulted: faulted != 0,
            cr2: hit.map_or(0, |h| h.cr2),
            error: hit.map_or(PageFaultError(0), |h| h.error),
            fault_rip: hit.map_or(0, |h| h.rip),
        }
    })
}

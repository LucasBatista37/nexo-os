//! Local APIC em modo xAPIC (registradores MMIO de 32 bits, espaçados de 16 bytes).
//!
//! O chamador mapeia a página física do LAPIC (tipicamente `0xFEE00000`) com
//! cache desabilitado e passa o endereço virtual.

use crate::cpu::{rdmsr, wrmsr};

/// MSR com a base física do LAPIC e o bit de habilitação global.
pub const IA32_APIC_BASE: u32 = 0x1b;
/// Bit "APIC global enable" do MSR.
pub const APIC_BASE_ENABLE: u64 = 1 << 11;
/// Bit "BSP" do MSR.
pub const APIC_BASE_BSP: u64 = 1 << 8;

const REG_ID: u32 = 0x20;
const REG_VERSION: u32 = 0x30;
const REG_TPR: u32 = 0x80;
const REG_EOI: u32 = 0xb0;
const REG_SVR: u32 = 0xf0;
const REG_ESR: u32 = 0x280;
const REG_ICR_LO: u32 = 0x300;
const REG_ICR_HI: u32 = 0x310;
const REG_LVT_TIMER: u32 = 0x320;
const REG_LVT_LINT0: u32 = 0x350;
const REG_LVT_LINT1: u32 = 0x360;
const REG_LVT_ERROR: u32 = 0x370;
const REG_TIMER_INIT: u32 = 0x380;
const REG_TIMER_CUR: u32 = 0x390;
const REG_TIMER_DIV: u32 = 0x3e0;

const LVT_MASKED: u32 = 1 << 16;
const ICR_DELIVERY_PENDING: u32 = 1 << 12;

/// Divisor do timer do LAPIC.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum TimerDivide {
    /// Sem divisão.
    By1 = 0b1011,
    /// ÷2.
    By2 = 0b0000,
    /// ÷4.
    By4 = 0b0001,
    /// ÷8.
    By8 = 0b0010,
    /// ÷16.
    By16 = 0b0011,
    /// ÷32.
    By32 = 0b1000,
    /// ÷64.
    By64 = 0b1001,
    /// ÷128.
    By128 = 0b1010,
}

impl TimerDivide {
    /// Fator numérico.
    pub const fn factor(self) -> u32 {
        match self {
            TimerDivide::By1 => 1,
            TimerDivide::By2 => 2,
            TimerDivide::By4 => 4,
            TimerDivide::By8 => 8,
            TimerDivide::By16 => 16,
            TimerDivide::By32 => 32,
            TimerDivide::By64 => 64,
            TimerDivide::By128 => 128,
        }
    }
}

/// Handle do LAPIC da CPU atual (o mapeamento é o mesmo em todas as CPUs).
#[derive(Clone, Copy)]
pub struct LocalApic {
    base: u64,
}

// SAFETY: registradores MMIO por CPU; cada CPU acessa o próprio LAPIC pelo
// mesmo endereço virtual.
unsafe impl Send for LocalApic {}
unsafe impl Sync for LocalApic {}

impl LocalApic {
    /// Cria o handle para o LAPIC mapeado em `virt_base`.
    ///
    /// # Safety
    /// `virt_base` deve mapear a página do LAPIC sem cache.
    pub const unsafe fn new(virt_base: u64) -> Self {
        LocalApic { base: virt_base }
    }

    #[inline]
    fn read(&self, reg: u32) -> u32 {
        // SAFETY: registrador dentro da página mapeada do LAPIC.
        unsafe { core::ptr::read_volatile((self.base + reg as u64) as *const u32) }
    }

    #[inline]
    fn write(&self, reg: u32, v: u32) {
        // SAFETY: idem.
        unsafe { core::ptr::write_volatile((self.base + reg as u64) as *mut u32, v) }
    }

    /// ID do LAPIC desta CPU.
    pub fn id(&self) -> u32 {
        self.read(REG_ID) >> 24
    }

    /// Registrador de versão (versão nos bits 0..8, LVTs máximas em 16..24).
    pub fn version(&self) -> u32 {
        self.read(REG_VERSION)
    }

    /// Habilita o LAPIC via SVR, zera TPR e mascara LINT0/LINT1/erro.
    pub fn enable(&self, spurious_vector: u8, error_vector: u8) {
        self.write(REG_TPR, 0);
        self.write(REG_LVT_LINT0, LVT_MASKED);
        self.write(REG_LVT_LINT1, LVT_MASKED);
        self.write(REG_LVT_ERROR, error_vector as u32);
        self.write(REG_LVT_TIMER, LVT_MASKED);
        self.write(REG_SVR, 0x100 | spurious_vector as u32);
        self.write(REG_ESR, 0);
    }

    /// Fim de interrupção.
    #[inline]
    pub fn eoi(&self) {
        self.write(REG_EOI, 0);
    }

    /// Lê e limpa o registrador de erro.
    pub fn error_status(&self) -> u32 {
        self.write(REG_ESR, 0);
        self.read(REG_ESR)
    }

    fn wait_icr_idle(&self) {
        let mut spins = 0u32;
        while self.read(REG_ICR_LO) & ICR_DELIVERY_PENDING != 0 {
            spins += 1;
            if spins > 1_000_000 {
                break;
            }
            core::hint::spin_loop();
        }
    }

    fn icr(&self, dest_apic_id: u32, low: u32) {
        self.wait_icr_idle();
        self.write(REG_ICR_HI, dest_apic_id << 24);
        self.write(REG_ICR_LO, low);
        self.wait_icr_idle();
    }

    /// IPI de vetor fixo para `dest_apic_id`.
    pub fn send_ipi(&self, dest_apic_id: u32, vector: u8) {
        self.icr(dest_apic_id, vector as u32 | (1 << 14));
    }

    /// IPI para a própria CPU.
    pub fn send_ipi_self(&self, vector: u8) {
        self.icr(0, vector as u32 | (1 << 14) | (0b01 << 18));
    }

    /// IPI para todas as outras CPUs.
    pub fn send_ipi_all_others(&self, vector: u8) {
        self.icr(0, vector as u32 | (1 << 14) | (0b11 << 18));
    }

    /// INIT (asserção por nível) para `dest_apic_id`.
    pub fn send_init(&self, dest_apic_id: u32) {
        self.icr(dest_apic_id, (5 << 8) | (1 << 14) | (1 << 15));
        self.icr(dest_apic_id, (5 << 8) | (1 << 15)); // deassert
    }

    /// Startup IPI: a CPU começa em `vector << 12` (modo real).
    pub fn send_sipi(&self, dest_apic_id: u32, vector: u8) {
        self.icr(dest_apic_id, (6 << 8) | vector as u32);
    }

    /// Configura o timer: vetor, periódico ou não, divisor. Não inicia.
    pub fn timer_configure(&self, vector: u8, periodic: bool, divide: TimerDivide) {
        self.write(REG_TIMER_DIV, divide as u32);
        self.write(
            REG_LVT_TIMER,
            vector as u32 | if periodic { 1 << 17 } else { 0 },
        );
    }

    /// Inicia/reinicia a contagem.
    pub fn timer_start(&self, initial: u32) {
        self.write(REG_TIMER_INIT, initial);
    }

    /// Contagem atual.
    pub fn timer_current(&self) -> u32 {
        self.read(REG_TIMER_CUR)
    }

    /// Para o timer (mascara e zera).
    pub fn timer_stop(&self) {
        self.write(REG_TIMER_INIT, 0);
        self.write(REG_LVT_TIMER, LVT_MASKED);
    }
}

/// Base física do LAPIC segundo o MSR e se está globalmente habilitado.
pub fn apic_base() -> (u64, bool, bool) {
    // SAFETY: MSR existe em toda CPU com APIC.
    let v = unsafe { rdmsr(IA32_APIC_BASE) };
    (
        v & 0x000f_ffff_ffff_f000,
        v & APIC_BASE_ENABLE != 0,
        v & APIC_BASE_BSP != 0,
    )
}

/// Liga o bit de habilitação global (se estava desligado).
///
/// # Safety
/// Altera o roteamento de interrupções da CPU.
pub unsafe fn enable_global() {
    // SAFETY: contrato da função.
    unsafe {
        let v = rdmsr(IA32_APIC_BASE);
        if v & APIC_BASE_ENABLE == 0 {
            wrmsr(IA32_APIC_BASE, v | APIC_BASE_ENABLE);
        }
    }
}

/// `true` se a CPU anuncia um LAPIC (CPUID.1:EDX[9]).
pub fn cpu_has_apic() -> bool {
    crate::cpu::cpuid(1, 0).edx & (1 << 9) != 0
}

/// `true` se a CPU anuncia TSC invariante (CPUID.80000007:EDX[8]).
pub fn tsc_invariant() -> bool {
    crate::cpu::cpuid(0x8000_0000, 0).eax >= 0x8000_0007
        && crate::cpu::cpuid(0x8000_0007, 0).edx & (1 << 8) != 0
}

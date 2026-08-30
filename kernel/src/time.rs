//! Tempo: TSC calibrado pelo PIT (relógio monotônico em ns) e timer do LAPIC
//! como fonte de tick (1000 Hz) para o escalonador e `sleep`.
//!
//! Calibração: o PIT canal 2 conta 20 ms em modo one-shot enquanto medimos o
//! avanço do TSC e a descida do timer do LAPIC (divisor 1). Não usa IRQs.

use core::sync::atomic::{AtomicU64, Ordering};
use nexo_arch_x86_64::apic::{self, TimerDivide};
use nexo_arch_x86_64::{cpu, pit};

use crate::x86::apic::vectors;

/// Frequência do tick.
pub const HZ: u64 = 1000;
const CALIBRATION_PIT_TICKS: u32 = 23_864; // 20 ms a 1,193182 MHz
const APIC_DIVIDE: TimerDivide = TimerDivide::By16;

static TICKS: AtomicU64 = AtomicU64::new(0);
static TSC_HZ: AtomicU64 = AtomicU64::new(0);
static TSC_BASE: AtomicU64 = AtomicU64::new(0);
static APIC_TIMER_HZ: AtomicU64 = AtomicU64::new(0);
static APIC_INITIAL: AtomicU64 = AtomicU64::new(0);

struct Calibration {
    tsc_hz: u64,
    apic_hz: u64,
}

/// Mede TSC e timer do LAPIC contra o PIT. Deve rodar com interrupções desabilitadas.
fn calibrate() -> Calibration {
    let lapic = crate::x86::apic::lapic();
    lapic.timer_configure(vectors::TIMER, false, TimerDivide::By1);
    // SAFETY: PIT canal 2 é dedicado à calibração; IRQs estão desabilitadas.
    unsafe { pit::channel2_one_shot(0xffff) };
    // Espera o PIT começar a contar (primeira leitura pode ser o valor inicial).
    let mut start = pit::channel2_read();
    let mut spins = 0;
    while start == 0xffff && spins < 100_000 {
        start = pit::channel2_read();
        spins += 1;
    }
    let tsc0 = cpu::rdtsc();
    lapic.timer_start(0xffff_ffff);
    let mut elapsed;
    loop {
        let now = pit::channel2_read();
        elapsed = start.wrapping_sub(now) as u32;
        if elapsed >= CALIBRATION_PIT_TICKS || now > start {
            break;
        }
        core::hint::spin_loop();
    }
    let tsc1 = cpu::rdtsc();
    let apic_cur = lapic.timer_current();
    lapic.timer_stop();
    // SAFETY: encerra a contagem do canal 2.
    unsafe { pit::channel2_stop() };
    let elapsed = elapsed.max(1) as u64;
    let tsc_hz = (tsc1 - tsc0) * pit::INPUT_FREQUENCY as u64 / elapsed;
    let apic_hz = (0xffff_ffffu64 - apic_cur as u64) * pit::INPUT_FREQUENCY as u64 / elapsed;
    Calibration { tsc_hz, apic_hz }
}

/// Calibra o TSC/timer do LAPIC, inicia o tick de 1000 Hz e habilita interrupções.
pub fn init() {
    let c = calibrate();
    TSC_HZ.store(c.tsc_hz, Ordering::Relaxed);
    TSC_BASE.store(cpu::rdtsc(), Ordering::Relaxed);
    APIC_TIMER_HZ.store(c.apic_hz, Ordering::Relaxed);
    let initial = (c.apic_hz / APIC_DIVIDE.factor() as u64 / HZ).max(1);
    APIC_INITIAL.store(initial, Ordering::Relaxed);
    start_local_timer();
    // SAFETY: IDT instalada com handler para o vetor do timer.
    unsafe { cpu::enable_interrupts() };
    kinfo!(
        "time: TSC {}.{:03} MHz ({}), timer LAPIC {}.{:03} MHz, tick {} Hz (contagem {} /{}), IF={}",
        c.tsc_hz / 1_000_000,
        (c.tsc_hz / 1000) % 1000,
        if apic::tsc_invariant() {
            "invariante"
        } else {
            "sem flag invariante"
        },
        c.apic_hz / 1_000_000,
        (c.apic_hz / 1000) % 1000,
        HZ,
        initial,
        APIC_DIVIDE.factor(),
        cpu::interrupts_enabled()
    );
}

/// Programa o timer periódico do LAPIC desta CPU com os parâmetros calibrados.
pub fn start_local_timer() {
    let lapic = crate::x86::apic::lapic();
    lapic.timer_configure(vectors::TIMER, true, APIC_DIVIDE);
    lapic.timer_start(APIC_INITIAL.load(Ordering::Relaxed) as u32);
}

/// Chamado pelo handler do vetor do timer na CPU de boot.
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Ticks desde a habilitação do timer.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Frequência calibrada do TSC (0 antes de `init`).
pub fn tsc_hz() -> u64 {
    TSC_HZ.load(Ordering::Relaxed)
}

/// Frequência calibrada do timer do LAPIC.
pub fn apic_timer_hz() -> u64 {
    APIC_TIMER_HZ.load(Ordering::Relaxed)
}

/// Nanossegundos monotônicos desde a calibração (TSC).
pub fn monotonic_ns() -> u64 {
    let hz = tsc_hz();
    if hz == 0 {
        return ticks() * (1_000_000_000 / HZ);
    }
    let delta = cpu::rdtsc().saturating_sub(TSC_BASE.load(Ordering::Relaxed)) as u128;
    (delta * 1_000_000_000 / hz as u128) as u64
}

/// Milissegundos desde o boot (TSC quando calibrado; senão ticks).
///
/// Em emulação (TCG) as interrupções periódicas do timer coalescem enquanto
/// a CPU está em `hlt`, logo `ticks()` pode ficar atrás do tempo real; o TSC
/// é a referência de tempo, e os ticks servem ao escalonador.
pub fn uptime_ms() -> u64 {
    if tsc_hz() == 0 {
        ticks() * 1000 / HZ
    } else {
        monotonic_ns() / 1_000_000
    }
}

/// Microssegundos desde o boot (TSC quando calibrado).
pub fn uptime_micros() -> u64 {
    if tsc_hz() == 0 {
        ticks() * 1_000_000 / HZ
    } else {
        monotonic_ns() / 1000
    }
}

/// Espera ocupada de `us` microssegundos pelo TSC (requer calibração).
pub fn delay_us(us: u64) {
    let hz = tsc_hz();
    if hz == 0 {
        for _ in 0..us * 100 {
            core::hint::spin_loop();
        }
        return;
    }
    let end = cpu::rdtsc() + us * hz / 1_000_000;
    while cpu::rdtsc() < end {
        core::hint::spin_loop();
    }
}

/// Aguarda `ms` milissegundos de tempo real (TSC); dorme com `hlt` entre ticks
/// quando interrupções estão ativas.
pub fn sleep_ms(ms: u64) {
    if tsc_hz() == 0 {
        let end = ticks() + ms * HZ / 1000;
        while ticks() < end {
            if cpu::interrupts_enabled() {
                cpu::halt()
            } else {
                core::hint::spin_loop()
            }
        }
        return;
    }
    let end = monotonic_ns() + ms * 1_000_000;
    while monotonic_ns() < end {
        if cpu::interrupts_enabled() {
            cpu::halt()
        } else {
            core::hint::spin_loop()
        }
    }
}

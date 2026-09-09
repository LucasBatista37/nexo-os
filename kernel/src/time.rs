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
    match nexo_arch_x86_64::rtc::read_epoch() {
        Some(e) => {
            BOOT_EPOCH.store(e, Ordering::Relaxed);
            kinfo!("time: RTC (UTC): epoch {e} no boot");
        }
        None => kinfo!("time: RTC ilegivel — relogio de parede indisponivel"),
    }
    APIC_TIMER_HZ.store(c.apic_hz, Ordering::Relaxed);
    let initial = (c.apic_hz / APIC_DIVIDE.factor() as u64 / HZ).max(1);
    APIC_INITIAL.store(initial, Ordering::Relaxed);
    start_local_timer();
    // SAFETY: IDT instalada com handler para o vetor do timer.
    unsafe { cpu::enable_interrupts() };
    kinfo!(
        "time: TSC {}.{:03} MHz ({}), timer LAPIC {}.{:03} MHz, tick {} Hz (contagem {} /{}; one-shot dinamico na BSP), IF={}",
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

/// Programa o timer do LAPIC desta CPU com os parâmetros calibrados: **periódico** (1 ms)
/// nas APs; **one-shot** na BSP (tique dinâmico: re-armado a cada disparo para o próximo
/// prazo, no máximo 1 ms à frente — ver [`rearm_bsp`]).
pub fn start_local_timer() {
    let lapic = crate::x86::apic::lapic();
    if is_bsp() {
        lapic.timer_configure(vectors::TIMER, false, APIC_DIVIDE);
        ARMED_DEADLINE.store(monotonic_ns() + TICK_NS, Ordering::Relaxed);
        lapic.timer_start(APIC_INITIAL.load(Ordering::Relaxed) as u32);
    } else {
        lapic.timer_configure(vectors::TIMER, true, APIC_DIVIDE);
        lapic.timer_start(APIC_INITIAL.load(Ordering::Relaxed) as u32);
    }
}

/// Nanossegundos por tique periódico.
const TICK_NS: u64 = 1_000_000_000 / HZ;
/// Menor antecedência armada (evita disparos em cascata por prazos já vencidos).
const MIN_ARM_NS: u64 = 20_000;
/// Prazo (ns monotônicos) para o qual o one-shot da BSP está armado.
static ARMED_DEADLINE: AtomicU64 = AtomicU64::new(0);
/// Re-armamentos antecipados (prazo novo antes do armado): diagnóstico.
static EARLY_REARMS: AtomicU64 = AtomicU64::new(0);

fn is_bsp() -> bool {
    crate::x86::percpu::try_current().is_none_or(|c| c.index == 0)
}

/// Arma o one-shot da BSP para `deadline_ns` (limitado a `[now + MIN_ARM_NS, now + 1 ms]`).
/// Só na BSP.
fn arm_bsp(deadline_ns: u64) {
    let now = monotonic_ns();
    let delta = deadline_ns.saturating_sub(now).clamp(MIN_ARM_NS, TICK_NS);
    let per_tick = APIC_INITIAL.load(Ordering::Relaxed).max(1);
    let count = (per_tick * delta / TICK_NS).max(1) as u32;
    ARMED_DEADLINE.store(now + delta, Ordering::Relaxed);
    crate::x86::apic::lapic().timer_start(count);
}

/// Chamado pelo handler do timer na BSP **antes** de escalonar (o handler pode trocar de
/// thread e só voltar muito depois): re-arma o one-shot para o próximo prazo — a dormida
/// mais próxima, o timer de kernel mais próximo ou o tique regular de 1 ms.
pub fn rearm_bsp() {
    let now = monotonic_ns();
    let mut next = now + TICK_NS;
    // só prazos ainda no futuro: os vencidos são atendidos pelo `on_tick` logo a seguir
    if let Some(w) = crate::sched::next_wake_after(now) {
        next = next.min(w);
    }
    if let Some(t) = crate::timer::next_deadline_after(now) {
        next = next.min(t);
    }
    arm_bsp(next);
}

/// Um prazo novo (dormida ou timer) foi registrado: se vence antes do que está armado,
/// re-arma agora (na BSP) ou pede à BSP que re-arme (IPI do vetor do timer, de outra CPU).
pub fn notify_deadline(deadline_ns: u64) {
    if tsc_hz() == 0 {
        return;
    }
    // Só deixa de armar se o prazo já armado é MAIS CEDO e ainda está no futuro; um prazo
    // armado que já passou (a IRQ está atrasada ou pendente) não pode segurar o novo — sob
    // emulação a IRQ chega centenas de µs depois e o bookkeeping ficaria "no passado".
    let armed = ARMED_DEADLINE.load(Ordering::Relaxed);
    if deadline_ns >= armed && armed > monotonic_ns() {
        return;
    }
    EARLY_REARMS.fetch_add(1, Ordering::Relaxed);
    if is_bsp() {
        arm_bsp(deadline_ns);
    } else if let (Some(bsp), Some(l)) = (crate::x86::percpu::get(0), crate::x86::apic::try_lapic())
    {
        // evita tempestade de IPIs: marca como "já pedido" até o próximo tique da BSP
        ARMED_DEADLINE.store(deadline_ns, Ordering::Relaxed);
        l.send_ipi(bsp.apic_id, vectors::TIMER);
    }
}

/// Teste/diagnóstico: arma o one-shot da BSP para daqui a `ns` (só na BSP) e devolve
/// (LVT do timer, contagem atual logo após o armamento).
pub fn probe_arm(ns: u64) -> (u32, u32) {
    // Sem interrupções: entre armar e ler, o próprio one-shot (ou um prazo mais cedo) podia
    // disparar e re-armar para 1 ms — a leitura via a contagem nova, não a que pedimos.
    cpu::without_interrupts(|| {
        let l = crate::x86::apic::lapic();
        arm_bsp(monotonic_ns() + ns);
        (l.timer_lvt(), l.timer_current())
    })
}

/// Contagem do timer do LAPIC correspondente a `ns` (com o divisor calibrado).
pub fn apic_counts_for_ns(ns: u64) -> u32 {
    (APIC_INITIAL.load(Ordering::Relaxed).max(1) * ns / TICK_NS) as u32
}

/// Re-armamentos antecipados do tique dinâmico (diagnóstico).
pub fn early_rearms() -> u64 {
    EARLY_REARMS.load(Ordering::Relaxed)
}

/// Chamado pelo handler do vetor do timer na CPU de boot. Com o tique dinâmico a BSP é
/// interrompida sempre que há um prazo mais cedo, então **um tique não é uma interrupção**:
/// o contador conta fronteiras de 1 ms (continua a ser uma medida de tempo, como no tique
/// periódico). Devolve `true` quando cruzou uma fronteira — só aí o quantum é debitado.
pub fn tick() -> bool {
    let now = monotonic_ns();
    let next = NEXT_TICK_AT.load(Ordering::Relaxed);
    if now < next {
        return false;
    }
    TICKS.fetch_add(1, Ordering::Relaxed);
    // reancora se ficou muito para trás (o guest pode ter parado: `hlt` longo, host ocupado)
    NEXT_TICK_AT.store(
        if now >= next + TICK_NS {
            now + TICK_NS
        } else {
            next + TICK_NS
        },
        Ordering::Relaxed,
    );
    true
}

/// Instante da próxima fronteira de tique (ns monotônicos).
static NEXT_TICK_AT: AtomicU64 = AtomicU64::new(0);

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
/// Segundos Unix (UTC) no boot, lidos do RTC (0 = RTC indisponível).
static BOOT_EPOCH: AtomicU64 = AtomicU64::new(0);

/// Relógio de parede: segundos Unix (UTC) agora; 0 se o RTC não pôde ser lido no boot.
pub fn wall_epoch() -> u64 {
    let base = BOOT_EPOCH.load(Ordering::Relaxed);
    if base == 0 {
        0
    } else {
        base + uptime_ms() / 1000
    }
}

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

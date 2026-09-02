//! Trace de syscalls (Plano §Fase 6: "criar profiler e visualizador de traces" — a metade do
//! *trace*; o profiler por amostragem fica para depois). Um anel global de eventos
//! `{tsc, pid, nr}` alimentado pelo despachante quando habilitado; leitura não destrutiva por
//! syscall (o leitor recebe os últimos eventos na ordem do anel). Barato por construção:
//! desabilitado é um load relaxado; habilitado é um `fetch_add` + três stores relaxados.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// Entradas do anel (potência de dois).
pub const ENTRIES: usize = 4096;

/// Um evento de trace, como copiado ao usuário (16 bytes, `repr(C)`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Event {
    /// Contador de tempo (TSC) no momento do evento.
    pub tsc: u64,
    /// Processo que fez a syscall.
    pub pid: u32,
    /// Número da syscall.
    pub nr: u16,
    /// Reservado (zero).
    pub reserved: u16,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static HEAD: AtomicUsize = AtomicUsize::new(0);
// Trincas paralelas (evita exigir atomics de 128 bits): um slot pode ser lido em corrida com
// uma gravação — o trace é diagnóstico, não fonte de verdade; leitores toleram um evento
// rasgado no limite do anel.
static TSC: [AtomicU64; ENTRIES] = [const { AtomicU64::new(0) }; ENTRIES];
static PIDNR: [AtomicU64; ENTRIES] = [const { AtomicU64::new(0) }; ENTRIES];

/// Liga/desliga a gravação.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Release);
}

/// `true` se a gravação está ligada.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Total de eventos gravados desde o boot (o anel guarda os últimos [`ENTRIES`]).
pub fn recorded() -> u64 {
    HEAD.load(Ordering::Relaxed) as u64
}

/// Grava um evento (chamado pelo despachante de syscalls quando habilitado).
pub fn record(pid: u64, nr: u64) {
    let i = HEAD.fetch_add(1, Ordering::Relaxed) % ENTRIES;
    TSC[i].store(nexo_arch_x86_64::cpu::rdtsc(), Ordering::Relaxed);
    PIDNR[i].store(pid << 16 | (nr & 0xffff), Ordering::Relaxed);
}

/// Copia até `out.len()` eventos, dos mais antigos disponíveis aos mais novos; devolve quantos.
pub fn snapshot(out: &mut [Event]) -> usize {
    let head = HEAD.load(Ordering::Acquire);
    let avail = head.min(ENTRIES);
    let n = avail.min(out.len());
    for (k, slot) in out[..n].iter_mut().enumerate() {
        let i = (head - n + k) % ENTRIES;
        let pidnr = PIDNR[i].load(Ordering::Relaxed);
        *slot = Event {
            tsc: TSC[i].load(Ordering::Relaxed),
            pid: (pidnr >> 16) as u32,
            nr: (pidnr & 0xffff) as u16,
            reserved: 0,
        };
    }
    n
}

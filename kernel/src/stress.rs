//! Modo de stress (`stress=<segundos>`): threads concorrentes exercitando
//! escalonador, locks, heap, paginação, sleep/join e IPIs em todas as CPUs,
//! com verificação de invariantes a cada segundo. Base do gate F1 (24 h).

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use nexo_arch_x86_64::paging::PageFlags;
use nexo_mm::VirtAddr;
use nexo_sync::SpinLock;

use crate::sched;
use crate::sync::IrqLock;

static STOP: AtomicBool = AtomicBool::new(false);
static LOCKED_COUNTER: IrqLock<u64> = IrqLock::new(0);
static LOCKED_EXPECTED: AtomicU64 = AtomicU64::new(0);
static ATOMIC_COUNTER: AtomicU64 = AtomicU64::new(0);
static ALLOC_OPS: AtomicU64 = AtomicU64::new(0);
static SLEEP_OPS: AtomicU64 = AtomicU64::new(0);
static SPAWN_OPS: AtomicU64 = AtomicU64::new(0);
static PAGE_OPS: AtomicU64 = AtomicU64::new(0);
static YIELD_OPS: AtomicU64 = AtomicU64::new(0);
static ERRORS: AtomicU64 = AtomicU64::new(0);
static CPU_MASK: AtomicU64 = AtomicU64::new(0);
static CHILD_RUNS: AtomicU64 = AtomicU64::new(0);
static PAGE_LOCK: SpinLock<()> = SpinLock::new(());

fn note_cpu() {
    if let Some(c) = crate::x86::percpu::try_current() {
        CPU_MASK.fetch_or(1 << (c.index.min(63)), Ordering::Relaxed);
    }
}

fn fail(msg: &str) {
    ERRORS.fetch_add(1, Ordering::Relaxed);
    kerror!("stress: {msg}");
}

fn worker_lock(_arg: usize) {
    let mut local = 0u64;
    while !STOP.load(Ordering::Relaxed) {
        for _ in 0..1000 {
            *LOCKED_COUNTER.lock() += 1;
            local += 1;
        }
        note_cpu();
        YIELD_OPS.fetch_add(1, Ordering::Relaxed);
        sched::yield_now();
    }
    LOCKED_EXPECTED.fetch_add(local, Ordering::Relaxed);
}

fn worker_atomic(_arg: usize) {
    while !STOP.load(Ordering::Relaxed) {
        for _ in 0..10_000 {
            ATOMIC_COUNTER.fetch_add(1, Ordering::Relaxed);
        }
        note_cpu();
    }
}

fn worker_alloc(seed: usize) {
    let mut x = seed as u64 * 0x9E37_79B9_7F4A_7C15 + 1;
    while !STOP.load(Ordering::Relaxed) {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let n = (x % 4000 + 16) as usize;
        let tag = (x & 0xff) as u8;
        let v = alloc::vec![tag; n];
        let b: Box<[u64]> = (0..(n / 8).max(1)).map(|i| i as u64 ^ x).collect();
        sched::yield_now();
        if v.iter().any(|&c| c != tag) {
            fail("conteudo de Vec corrompido");
        }
        if b.iter().enumerate().any(|(i, &w)| w != i as u64 ^ x) {
            fail("conteudo de Box corrompido");
        }
        ALLOC_OPS.fetch_add(1, Ordering::Relaxed);
        note_cpu();
    }
}

fn worker_sleep(arg: usize) {
    let ms = 1 + (arg as u64 % 5);
    while !STOP.load(Ordering::Relaxed) {
        let t0 = crate::time::monotonic_ns();
        sched::sleep_ms(ms);
        let dt = crate::time::monotonic_ns() - t0;
        if dt < ms * 1_000_000 {
            fail("sleep acordou cedo demais");
        }
        SLEEP_OPS.fetch_add(1, Ordering::Relaxed);
        note_cpu();
    }
}

fn child(arg: usize) {
    CHILD_RUNS.fetch_add(1, Ordering::Relaxed);
    let v = alloc::vec![arg as u8; 256];
    if arg.is_multiple_of(3) {
        sched::yield_now();
    }
    if v[255] != arg as u8 {
        fail("filho: conteudo");
    }
    note_cpu();
}

fn worker_spawn(_arg: usize) {
    let mut i = 0usize;
    while !STOP.load(Ordering::Relaxed) {
        let ids: Vec<_> = (0..4)
            .map(|k| sched::spawn("stress/filho", child, i + k))
            .collect();
        for id in ids {
            if !sched::join(id) {
                fail("join de filho inexistente");
            }
        }
        i += 4;
        SPAWN_OPS.fetch_add(4, Ordering::Relaxed);
        if i.is_multiple_of(64) {
            sched::reap();
        }
        note_cpu();
    }
}

fn worker_pages(arg: usize) {
    let base = 0xffff_ffff_d100_0000u64 + (arg as u64) * 0x10_0000;
    let mut round = 0u64;
    while !STOP.load(Ordering::Relaxed) {
        let v = VirtAddr::new(base + (round % 16) * 0x1000);
        {
            let _g = PAGE_LOCK.lock();
        }
        match crate::mm::virt::alloc_and_map(v, PageFlags::KERNEL_RW) {
            Ok(_) => {
                // SAFETY: página recém-mapeada RW e exclusiva desta thread.
                unsafe {
                    let p = v.as_mut_ptr::<u64>();
                    p.write_volatile(round);
                    if p.read_volatile() != round {
                        fail("pagina nao retem valor");
                    }
                }
                if crate::mm::virt::unmap_and_free(v).is_err() {
                    fail("unmap falhou");
                }
                PAGE_OPS.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => fail("map falhou"),
        }
        round += 1;
        if round.is_multiple_of(8) {
            sched::yield_now();
        }
        note_cpu();
    }
}

/// Executa o stress por `secs` segundos. Devolve `true` se nenhum invariante falhou.
pub fn run(secs: u64) -> bool {
    let cpus = crate::x86::percpu::online_count();
    kprint!("[STRESS] iniciando: {secs} s, {cpus} CPUs\n");
    let frames0 = crate::mm::phys::stats().free;
    let heap0 = crate::mm::heap::stats().used_bytes;
    STOP.store(false, Ordering::Relaxed);
    let mut ids = Vec::new();
    for i in 0..cpus.max(2) {
        ids.push(sched::spawn("stress/lock", worker_lock, i));
        ids.push(sched::spawn("stress/atomic", worker_atomic, i));
        ids.push(sched::spawn("stress/alloc", worker_alloc, i));
        ids.push(sched::spawn("stress/sleep", worker_sleep, i));
    }
    ids.push(sched::spawn("stress/spawn", worker_spawn, 0));
    ids.push(sched::spawn("stress/pages", worker_pages, 0));
    ids.push(sched::spawn("stress/pages", worker_pages, 1));

    let start = crate::time::monotonic_ns();
    let deadline = start + secs * 1_000_000_000;
    let mut next_report = start + 1_000_000_000;
    let mut last_switches = 0;
    while crate::time::monotonic_ns() < deadline {
        sched::sleep_ms(100);
        let now = crate::time::monotonic_ns();
        if now >= next_report {
            next_report += 1_000_000_000;
            let s = sched::stats();
            let f = crate::mm::phys::stats();
            let h = crate::mm::heap::stats();
            kprint!(
                "[STRESS] t={}s trocas={} (+{}) preempcoes={} threads={} lock={} atomic={} alloc={} sleep={} spawn={} pages={} yields={} cpus={:#b} quadros_livres={} heap_uso={} erros={}\n",
                (now - start) / 1_000_000_000,
                s.switches,
                s.switches - last_switches,
                s.preemptions,
                s.alive,
                *LOCKED_COUNTER.lock(),
                ATOMIC_COUNTER.load(Ordering::Relaxed),
                ALLOC_OPS.load(Ordering::Relaxed),
                SLEEP_OPS.load(Ordering::Relaxed),
                SPAWN_OPS.load(Ordering::Relaxed),
                PAGE_OPS.load(Ordering::Relaxed),
                YIELD_OPS.load(Ordering::Relaxed),
                CPU_MASK.load(Ordering::Relaxed),
                f.free,
                h.used_bytes,
                ERRORS.load(Ordering::Relaxed)
            );
            last_switches = s.switches;
            if s.switches == 0 {
                fail("nenhuma troca de contexto");
            }
            if h.used_bytes > heap0 + 32 * 1024 * 1024 {
                fail("heap crescendo sem limite (vazamento?)");
            }
            if f.free + 16 * 1024 < frames0 {
                fail("quadros fisicos vazando");
            }
        }
    }
    STOP.store(true, Ordering::Relaxed);
    for id in ids {
        sched::join(id);
    }
    sched::reap();
    let s = sched::stats();
    let locked = *LOCKED_COUNTER.lock();
    let expected = LOCKED_EXPECTED.load(Ordering::Relaxed);
    if locked != expected {
        fail("contador protegido por lock divergiu");
    }
    let f = crate::mm::phys::stats();
    let h = crate::mm::heap::stats();
    if h.used_bytes > heap0 + 256 * 1024 {
        fail("heap nao voltou ao patamar inicial");
    }
    if f.free + 64 < frames0 {
        fail("quadros nao voltaram ao patamar inicial");
    }
    if cpus > 1 && CPU_MASK.load(Ordering::Relaxed).count_ones() < 2 {
        fail("threads nao usaram mais de uma CPU");
    }
    let errors = ERRORS.load(Ordering::Relaxed);
    kprint!(
        "[STRESS] {} duracao={}s trocas={} preempcoes={} spawned={} reaped={} lock={}/{} atomic={} alloc={} sleep={} spawn={} filhos={} pages={} cpus={:#b} quadros={}->{} heap={}->{} erros={}\n",
        if errors == 0 { "PASS" } else { "FAIL" },
        (crate::time::monotonic_ns() - start) / 1_000_000_000,
        s.switches,
        s.preemptions,
        s.spawned,
        s.reaped,
        locked,
        expected,
        ATOMIC_COUNTER.load(Ordering::Relaxed),
        ALLOC_OPS.load(Ordering::Relaxed),
        SLEEP_OPS.load(Ordering::Relaxed),
        SPAWN_OPS.load(Ordering::Relaxed),
        CHILD_RUNS.load(Ordering::Relaxed),
        PAGE_OPS.load(Ordering::Relaxed),
        CPU_MASK.load(Ordering::Relaxed),
        frames0,
        f.free,
        heap0,
        h.used_bytes,
        errors
    );
    errors == 0
}

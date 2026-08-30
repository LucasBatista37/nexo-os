//! Timers de kernel: callbacks únicos ou periódicos com prazo em nanossegundos.
//!
//! O tick da BSP verifica a lista (ordenada por prazo) e move os vencidos para
//! uma fila de disparo; a thread `ktimer` executa os callbacks fora de
//! contexto de interrupção, portanto eles podem dormir, alocar e criar threads.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::sched;
use crate::sync::IrqLock;

/// Identificador de timer.
pub type TimerId = u64;

/// Callback: recebe o argumento registrado.
pub type Callback = fn(usize);

struct Pending {
    id: TimerId,
    deadline_ns: u64,
    period_ns: u64, // 0 = único
    callback: Callback,
    arg: usize,
}

struct Fired {
    callback: Callback,
    arg: usize,
}

static PENDING: IrqLock<Vec<Pending>> = IrqLock::new(Vec::new());
static FIRED: IrqLock<Vec<Fired>> = IrqLock::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static FIRED_TOTAL: AtomicU64 = AtomicU64::new(0);
static WORKER_READY: AtomicBool = AtomicBool::new(false);

fn insert(mut list: crate::sync::IrqGuard<'_, Vec<Pending>>, p: Pending) {
    let pos = list
        .iter()
        .position(|q| q.deadline_ns > p.deadline_ns)
        .unwrap_or(list.len());
    list.insert(pos, p);
}

/// Agenda `callback(arg)` para daqui a `after_ns` nanossegundos.
pub fn after_ns(after_ns: u64, callback: Callback, arg: usize) -> TimerId {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let deadline = crate::time::monotonic_ns() + after_ns;
    insert(
        PENDING.lock(),
        Pending {
            id,
            deadline_ns: deadline,
            period_ns: 0,
            callback,
            arg,
        },
    );
    id
}

/// Agenda `callback(arg)` a cada `period_ns` (primeiro disparo após um período).
pub fn periodic_ns(period_ns: u64, callback: Callback, arg: usize) -> TimerId {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let deadline = crate::time::monotonic_ns() + period_ns.max(1);
    insert(
        PENDING.lock(),
        Pending {
            id,
            deadline_ns: deadline,
            period_ns: period_ns.max(1),
            callback,
            arg,
        },
    );
    id
}

/// Cancela um timer pendente. Devolve `true` se ele ainda não tinha disparado (ou era periódico e foi removido).
pub fn cancel(id: TimerId) -> bool {
    let mut list = PENDING.lock();
    let before = list.len();
    list.retain(|p| p.id != id);
    list.len() != before
}

/// Timers pendentes.
pub fn pending_count() -> usize {
    PENDING.lock().len()
}

/// Total de disparos executados.
pub fn fired_total() -> u64 {
    FIRED_TOTAL.load(Ordering::Relaxed)
}

/// Chamado pelo tick da BSP (interrupções desabilitadas). Move vencidos para a fila de disparo.
pub fn on_tick() {
    let now = crate::time::monotonic_ns();
    let mut pending = PENDING.lock();
    if pending.first().is_none_or(|p| p.deadline_ns > now) {
        return;
    }
    let mut fired = FIRED.lock();
    let mut requeue = Vec::new();
    while let Some(p) = pending.first() {
        if p.deadline_ns > now {
            break;
        }
        let p = pending.remove(0);
        fired.push(Fired {
            callback: p.callback,
            arg: p.arg,
        });
        if p.period_ns != 0 {
            requeue.push(Pending {
                deadline_ns: p.deadline_ns + p.period_ns,
                ..p
            });
        }
    }
    drop(fired);
    for p in requeue {
        let pos = pending
            .iter()
            .position(|q| q.deadline_ns > p.deadline_ns)
            .unwrap_or(pending.len());
        pending.insert(pos, p);
    }
}

fn worker(_: usize) {
    WORKER_READY.store(true, Ordering::Release);
    loop {
        let batch: Vec<Fired> = core::mem::take(&mut *FIRED.lock());
        if batch.is_empty() {
            sched::sleep_ms(1);
            continue;
        }
        for f in batch {
            (f.callback)(f.arg);
            FIRED_TOTAL.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Inicia a thread `ktimer`.
pub fn init() {
    sched::spawn("ktimer", worker, 0);
    while !WORKER_READY.load(Ordering::Acquire) {
        sched::yield_now();
    }
    kinfo!(
        "timer: thread ktimer ativa; resolucao de despacho {} ms",
        1000 / crate::time::HZ
    );
}

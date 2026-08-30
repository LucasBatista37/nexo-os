//! Vetores de interrupção entregues a processos de usuário (MSI/MSI-X).
//!
//! O kernel reserva vetores em `USER_VECTOR_BASE..`, conta disparos e acorda
//! quem espera em [`wait`]; o driver programa o dispositivo com o endereço e
//! os dados MSI devolvidos por [`alloc`].

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::sched::{self, ThreadId};
use crate::sync::IrqLock;

/// Primeiro vetor de usuário.
pub const USER_VECTOR_BASE: u8 = 0x50;
/// Número de vetores de usuário.
pub const USER_VECTORS: usize = 32;

static COUNTS: [AtomicU64; USER_VECTORS] = [const { AtomicU64::new(0) }; USER_VECTORS];
static ALLOCATED: IrqLock<[bool; USER_VECTORS]> = IrqLock::new([false; USER_VECTORS]);
static WAITERS: IrqLock<Vec<(u8, ThreadId)>> = IrqLock::new(Vec::new());

/// `true` se `vector` é um vetor de usuário.
pub fn is_user_vector(vector: u8) -> bool {
    (USER_VECTOR_BASE..USER_VECTOR_BASE + USER_VECTORS as u8).contains(&vector)
}

/// Total de interrupções de usuário entregues (todos os vetores).
pub fn total() -> u64 {
    COUNTS.iter().map(|c| c.load(Ordering::Relaxed)).sum()
}

/// Devolve um vetor ao pool (o contador permanece; quem espera é acordado).
pub fn free(vector: u8) {
    if !is_user_vector(vector) {
        return;
    }
    ALLOCATED.lock()[(vector - USER_VECTOR_BASE) as usize] = false;
    on_interrupt_wake_only(vector);
}

fn on_interrupt_wake_only(vector: u8) {
    let woken: Vec<ThreadId> = {
        let mut w = WAITERS.lock();
        let (mine, rest): (Vec<_>, Vec<_>) = w.drain(..).partition(|(v, _)| *v == vector);
        *w = rest;
        mine.into_iter().map(|(_, t)| t).collect()
    };
    for t in woken {
        sched::unpark(t);
    }
}

/// Reserva um vetor livre.
pub fn alloc() -> Option<u8> {
    let mut a = ALLOCATED.lock();
    let i = a.iter().position(|used| !*used)?;
    a[i] = true;
    Some(USER_VECTOR_BASE + i as u8)
}

/// Chamado pelo handler de trap (após o EOI): conta e acorda.
pub fn on_interrupt(vector: u8) {
    let i = (vector - USER_VECTOR_BASE) as usize;
    COUNTS[i].fetch_add(1, Ordering::Release);
    on_interrupt_wake_only(vector);
}

/// Contagem de disparos do vetor.
pub fn count(vector: u8) -> u64 {
    COUNTS[(vector - USER_VECTOR_BASE) as usize].load(Ordering::Acquire)
}

/// Bloqueia até `count(vector) > seen`; devolve a contagem atual.
pub fn wait(vector: u8, seen: u64) -> u64 {
    loop {
        let w = WAITERS.lock();
        let c = count(vector);
        if c > seen {
            return c;
        }
        let Some(me) = sched::current().map(|t| t.id) else {
            return c;
        };
        let mut w = w;
        w.push((vector, me));
        sched::park_with(w);
    }
}

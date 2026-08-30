//! Primitivas de sincronização mínimas, sem dependências, testáveis no host.
//!
//! Nesta fase o kernel executa em uma única CPU; os locks existem para tornar
//! a disciplina de acesso explícita e para que o código já esteja correto
//! quando o SMP chegar (Fase 1).
#![no_std]

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Spinlock simples (test-and-set com `spin_loop`).
pub struct SpinLock<T: ?Sized> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

// SAFETY: o acesso a `data` é serializado pelo flag `locked`.
unsafe impl<T: ?Sized + Send> Sync for SpinLock<T> {}
// SAFETY: mover o lock move o dado junto; nada é compartilhado sem o lock.
unsafe impl<T: ?Sized + Send> Send for SpinLock<T> {}

/// Guard que libera o lock ao sair de escopo.
pub struct SpinLockGuard<'a, T: ?Sized> {
    lock: &'a SpinLock<T>,
}

impl<T> SpinLock<T> {
    /// Cria um lock destravado.
    pub const fn new(value: T) -> Self {
        SpinLock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }

    /// Consome o lock e devolve o valor.
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> SpinLock<T> {
    /// Adquire o lock, girando até conseguir.
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        loop {
            if let Some(g) = self.try_lock() {
                return g;
            }
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
    }

    /// Tenta adquirir sem girar.
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinLockGuard { lock: self })
        } else {
            None
        }
    }

    /// `true` se alguém segura o lock neste instante.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    /// Libera o lock à força. Usado apenas no caminho de panic, quando o
    /// detentor pode ter sido interrompido no meio de uma operação.
    ///
    /// # Safety
    /// O chamador garante que nenhum guard vivo continuará a usar o dado.
    pub unsafe fn force_unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }

    /// Acesso mutável sem lock (exclusividade garantida pelo borrow checker).
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    /// Ponteiro cru para o dado, para quem já detém o lock por outros meios
    /// (ex.: guard esquecido através de uma troca de contexto).
    pub fn as_ptr(&self) -> *mut T {
        self.data.get()
    }
}

impl<T: ?Sized> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: o guard existe apenas enquanto o lock está adquirido.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: idem; acesso exclusivo garantido pelo lock.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

const ONCE_EMPTY: u8 = 0;
const ONCE_BUSY: u8 = 1;
const ONCE_READY: u8 = 2;

/// Célula inicializada exatamente uma vez (para globais do kernel).
pub struct Once<T> {
    state: AtomicU8,
    value: UnsafeCell<Option<T>>,
}

// SAFETY: escrita ocorre uma única vez, protegida pelo estado atômico; leituras
// só acontecem após `ONCE_READY` (Acquire).
unsafe impl<T: Send + Sync> Sync for Once<T> {}
// SAFETY: mover a célula move o valor junto.
unsafe impl<T: Send> Send for Once<T> {}

impl<T> Once<T> {
    /// Cria uma célula vazia.
    pub const fn new() -> Self {
        Once {
            state: AtomicU8::new(ONCE_EMPTY),
            value: UnsafeCell::new(None),
        }
    }

    /// Inicializa com `value`. Devolve `Err(value)` se já estava inicializada.
    pub fn set(&self, value: T) -> Result<(), T> {
        match self.state.compare_exchange(
            ONCE_EMPTY,
            ONCE_BUSY,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                // SAFETY: somos o único escritor (estado BUSY) e ninguém lê antes de READY.
                unsafe { *self.value.get() = Some(value) };
                self.state.store(ONCE_READY, Ordering::Release);
                Ok(())
            }
            Err(_) => Err(value),
        }
    }

    /// Inicializa com `f` se ainda vazia e devolve a referência.
    pub fn call_once<F: FnOnce() -> T>(&self, f: F) -> &T {
        if self.state.load(Ordering::Acquire) != ONCE_READY {
            let _ = self.set(f());
            while self.state.load(Ordering::Acquire) != ONCE_READY {
                core::hint::spin_loop();
            }
        }
        self.get().expect("Once: estado READY sem valor")
    }

    /// Devolve o valor, se inicializado.
    pub fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == ONCE_READY {
            // SAFETY: após READY o valor nunca mais é escrito.
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }

    /// `true` se já inicializada.
    pub fn is_ready(&self) -> bool {
        self.state.load(Ordering::Acquire) == ONCE_READY
    }
}

impl<T> Default for Once<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::sync::Arc;
    use std::vec::Vec;

    #[test]
    fn spinlock_serializes_increments() {
        let lock = Arc::new(SpinLock::new(0u64));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let l = Arc::clone(&lock);
                std::thread::spawn(move || {
                    for _ in 0..10_000 {
                        *l.lock() += 1;
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(*lock.lock(), 80_000);
        assert!(!lock.is_locked());
    }

    #[test]
    fn try_lock_fails_while_held() {
        let lock = SpinLock::new(1);
        let g = lock.lock();
        assert!(lock.try_lock().is_none());
        drop(g);
        assert!(lock.try_lock().is_some());
    }

    #[test]
    fn once_initializes_exactly_once() {
        let once: Once<u32> = Once::new();
        assert!(once.get().is_none());
        assert_eq!(once.set(7), Ok(()));
        assert_eq!(once.set(8), Err(8));
        assert_eq!(once.get(), Some(&7));
        assert_eq!(*once.call_once(|| 9), 7);
        assert!(once.is_ready());
    }
}

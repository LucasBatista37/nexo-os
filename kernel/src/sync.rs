//! Locks do kernel que desabilitam interrupções enquanto detidos.
//!
//! Regra: nenhum spinlock do kernel pode ser detido com interrupções
//! habilitadas — senão uma preempção pelo timer pode escalonar, na mesma CPU,
//! uma thread que gira esperando o lock que a thread interrompida detém.

use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
use nexo_arch_x86_64::cpu;
use nexo_sync::{SpinLock, SpinLockGuard};

/// Spinlock que desabilita interrupções ao adquirir e restaura ao soltar.
pub struct IrqLock<T> {
    inner: SpinLock<T>,
}

/// Guard de [`IrqLock`].
pub struct IrqGuard<'a, T> {
    guard: ManuallyDrop<SpinLockGuard<'a, T>>,
    was_enabled: bool,
}

impl<T> IrqLock<T> {
    /// Cria o lock.
    pub const fn new(value: T) -> Self {
        IrqLock {
            inner: SpinLock::new(value),
        }
    }

    /// Adquire (interrupções ficam desabilitadas até o guard ser solto).
    pub fn lock(&self) -> IrqGuard<'_, T> {
        let was_enabled = cpu::interrupts_enabled();
        cpu::disable_interrupts();
        IrqGuard {
            guard: ManuallyDrop::new(self.inner.lock()),
            was_enabled,
        }
    }

    /// Tenta adquirir sem girar.
    pub fn try_lock(&self) -> Option<IrqGuard<'_, T>> {
        let was_enabled = cpu::interrupts_enabled();
        cpu::disable_interrupts();
        match self.inner.try_lock() {
            Some(g) => Some(IrqGuard {
                guard: ManuallyDrop::new(g),
                was_enabled,
            }),
            None => {
                if was_enabled {
                    // SAFETY: estavam habilitadas antes.
                    unsafe { cpu::enable_interrupts() };
                }
                None
            }
        }
    }

    /// Libera à força (caminho de panic).
    ///
    /// # Safety
    /// Ver [`SpinLock::force_unlock`].
    pub unsafe fn force_unlock(&self) {
        // SAFETY: contrato da função.
        unsafe { self.inner.force_unlock() };
    }
}

impl<T> IrqGuard<'_, T> {
    /// Solta o lock **sem** restaurar o estado de interrupções. Devolve se
    /// elas estavam habilitadas ao adquirir, para o chamador restaurar depois.
    pub fn unlock_keep_irqs_disabled(mut self) -> bool {
        // SAFETY: solta o guard interno exatamente uma vez.
        unsafe { ManuallyDrop::drop(&mut self.guard) };
        let was = self.was_enabled;
        core::mem::forget(self);
        was
    }
}

impl<T> Deref for IrqGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> DerefMut for IrqGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T> Drop for IrqGuard<'_, T> {
    fn drop(&mut self) {
        // SAFETY: o guard interno é solto exatamente uma vez, antes de reabilitar interrupções.
        unsafe { ManuallyDrop::drop(&mut self.guard) };
        if self.was_enabled {
            // SAFETY: estavam habilitadas quando o lock foi adquirido.
            unsafe { cpu::enable_interrupts() };
        }
    }
}

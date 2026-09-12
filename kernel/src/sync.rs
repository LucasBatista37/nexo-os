//! Locks do kernel que desabilitam interrupções enquanto detidos.
//!
//! Regra: nenhum spinlock do kernel pode ser detido com interrupções
//! habilitadas — senão uma preempção pelo timer pode escalonar, na mesma CPU,
//! uma thread que gira esperando o lock que a thread interrompida detém.

use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_sync::{SpinLock, SpinLockGuard};

/// Spinlock que desabilita interrupções ao adquirir e restaura ao soltar.
pub struct IrqLock<T> {
    inner: SpinLock<T>,
    /// Última CPU que adquiriu o lock (`usize::MAX` = nenhuma): só para o diagnóstico de um
    /// lock preso — num deadlock é quem o detém.
    dono: AtomicUsize,
}

/// Quanto tempo uma CPU gira num lock antes de o declarar preso (3 s de TSC). Um deadlock
/// de spinlock com interrupções desligadas é silêncio total — nenhum log, nenhum batimento,
/// nada; assim vira um pânico com o dono e a pilha de quem esperava (2026-09-12).
const PRESO_APOS_S: u64 = 3;

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
            dono: AtomicUsize::new(usize::MAX),
        }
    }

    fn cpu_atual() -> usize {
        crate::x86::percpu::try_current().map_or(usize::MAX - 1, |c| c.index)
    }

    /// Adquire (interrupções ficam desabilitadas até o guard ser solto). Gira no máximo
    /// [`PRESO_APOS_S`] segundos: depois disso entra em pânico dizendo quem detém o lock.
    pub fn lock(&self) -> IrqGuard<'_, T> {
        let was_enabled = cpu::interrupts_enabled();
        cpu::disable_interrupts();
        let hz = crate::time::tsc_hz();
        let inicio = cpu::rdtsc();
        let guard = loop {
            if let Some(g) = self.inner.try_lock() {
                break g;
            }
            let mut voltas = 0u32;
            while self.inner.is_locked() {
                core::hint::spin_loop();
                voltas = voltas.wrapping_add(1);
                if voltas.is_multiple_of(4096)
                    && hz != 0
                    && cpu::rdtsc().wrapping_sub(inicio) > hz.saturating_mul(PRESO_APOS_S)
                {
                    let dono = self.dono.load(Ordering::Relaxed);
                    panic!(
                        "lock preso: cpu{} espera ha {} s pelo lock em {:p}, detido pela cpu{}",
                        Self::cpu_atual(),
                        PRESO_APOS_S,
                        self as *const Self,
                        dono
                    );
                }
            }
        };
        self.dono.store(Self::cpu_atual(), Ordering::Relaxed);
        IrqGuard {
            guard: ManuallyDrop::new(guard),
            was_enabled,
        }
    }

    /// Tenta adquirir sem girar.
    pub fn try_lock(&self) -> Option<IrqGuard<'_, T>> {
        let was_enabled = cpu::interrupts_enabled();
        cpu::disable_interrupts();
        match self.inner.try_lock() {
            Some(g) => {
                self.dono.store(Self::cpu_atual(), Ordering::Relaxed);
                Some(IrqGuard {
                    guard: ManuallyDrop::new(g),
                    was_enabled,
                })
            }
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

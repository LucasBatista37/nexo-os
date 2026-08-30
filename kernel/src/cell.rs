//! Célula para globais inicializados uma vez durante o boot.

use core::cell::UnsafeCell;

/// Contêiner `Sync` para dados globais que precisam de `&'static` com
/// mutabilidade interior controlada manualmente (GDT, IDT, pilhas).
///
/// Acesso é `unsafe`: o chamador garante ausência de aliasing mutável.
pub struct StaticCell<T>(UnsafeCell<T>);

// SAFETY: a disciplina de acesso é responsabilidade dos chamadores (`as_ptr`
// é usado apenas em inicialização single-core ou em leitura após publicação).
unsafe impl<T> Sync for StaticCell<T> {}

impl<T> StaticCell<T> {
    /// Cria a célula.
    pub const fn new(v: T) -> Self {
        StaticCell(UnsafeCell::new(v))
    }
    /// Ponteiro cru para o conteúdo.
    pub const fn as_ptr(&self) -> *mut T {
        self.0.get()
    }
}

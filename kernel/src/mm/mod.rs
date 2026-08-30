//! Gerenciamento de memória do kernel: física (bitmap), virtual (mapper sobre
//! o physmap) e heap.

pub mod heap;
pub mod phys;
pub mod virt;

/// Inicializa memória física, virtual e heap, nesta ordem.
pub fn init() {
    phys::init();
    virt::init();
    heap::init();
}

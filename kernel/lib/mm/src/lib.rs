//! Memória física independente de arquitetura: endereços, normalização do mapa
//! de memória e alocador de quadros por bitmap.
//!
//! Todo o código deste crate é seguro e testável no host (`cargo test`). O
//! kernel fornece armazenamento e o loader/kernel fornecem o mapa.
#![no_std]
#![deny(unsafe_code)]

pub mod addr;
pub mod bitmap;
pub mod map;

pub use addr::{PAGE_SHIFT, PAGE_SIZE, PhysAddr, VirtAddr, align_down, align_up};
pub use bitmap::{BitmapFrameAllocator, FrameError, FrameStats};
pub use map::{MapError, MapSummary, normalize, summarize};

/// Fonte de quadros físicos de 4 KiB.
pub trait FrameAllocator {
    /// Devolve um quadro livre (zerado ou não; o chamador decide).
    fn allocate_frame(&mut self) -> Option<PhysAddr>;
    /// Devolve um quadro ao alocador.
    fn deallocate_frame(&mut self, frame: PhysAddr);
}

/// Tradução de endereço físico para um ponteiro acessível.
///
/// No loader UEFI é a identidade; no kernel é o physmap; nos testes de host é
/// um arena em memória.
pub trait PhysToVirt {
    /// Ponteiro através do qual `phys` pode ser lido/escrito.
    fn phys_to_virt(&self, phys: PhysAddr) -> *mut u8;
}

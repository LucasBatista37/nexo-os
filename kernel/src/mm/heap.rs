//! Heap do kernel em `KERNEL_HEAP_BASE`, com guard pages e crescimento sob demanda.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicU64, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::paging::PageFlags;
use nexo_boot_abi::{KERNEL_HEAP_BASE, KERNEL_HEAP_MAX_SIZE};
use nexo_heap::{Heap, HeapStats};
use nexo_mm::{PAGE_SIZE, VirtAddr, align_up};
use nexo_sync::SpinLock;

const INITIAL_SIZE: u64 = 1024 * 1024;
const GROW_STEP: u64 = 1024 * 1024;

struct KernelHeap {
    heap: SpinLock<Heap>,
    mapped_end: AtomicU64,
    /// Alocações que falharam mesmo após tentar crescer (OOM real).
    oom: AtomicU64,
}

// SAFETY: todas as operações são serializadas pelo spinlock com interrupções
// desabilitadas; o crescimento só toca páginas exclusivas do heap.
unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        cpu::without_interrupts(|| {
            let mut h = self.heap.lock();
            if let Some(p) = h.allocate(layout) {
                return p.as_ptr();
            }
            if self.grow(&mut h, layout.size() as u64 + layout.align() as u64 + 64)
                && let Some(p) = h.allocate(layout)
            {
                return p.as_ptr();
            }
            self.oom.fetch_add(1, Ordering::Relaxed);
            kerror!(
                "heap: sem memoria para {} bytes (align {})",
                layout.size(),
                layout.align()
            );
            core::ptr::null_mut()
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        cpu::without_interrupts(|| {
            // SAFETY: `ptr` veio de `alloc` deste heap (contrato de GlobalAlloc).
            unsafe {
                self.heap
                    .lock()
                    .deallocate(core::ptr::NonNull::new_unchecked(ptr), layout)
            }
        })
    }
}

impl KernelHeap {
    /// Mapeia mais páginas e estende o heap. Chamado com o lock já adquirido.
    fn grow(&self, heap: &mut Heap, min: u64) -> bool {
        let end = self.mapped_end.load(Ordering::Relaxed);
        if end == 0 {
            return false;
        }
        let step = align_up(min.max(GROW_STEP), PAGE_SIZE);
        if end + step > KERNEL_HEAP_BASE + KERNEL_HEAP_MAX_SIZE {
            kwarn!(
                "heap: limite de {} MiB atingido",
                KERNEL_HEAP_MAX_SIZE >> 20
            );
            return false;
        }
        if !map_range(end, step) {
            return false;
        }
        // SAFETY: páginas recém-mapeadas, exclusivas do heap.
        unsafe { heap.extend(end as usize, step as usize) };
        self.mapped_end.store(end + step, Ordering::Relaxed);
        kdebug!(
            "heap: crescido para {} KiB",
            (end + step - KERNEL_HEAP_BASE) >> 10
        );
        true
    }
}

fn map_range(start: u64, len: u64) -> bool {
    let mut off = 0;
    while off < len {
        if let Err(e) = super::virt::alloc_and_map(VirtAddr::new(start + off), PageFlags::KERNEL_RW)
        {
            kerror!("heap: falha ao mapear {:#x}: {e}", start + off);
            return false;
        }
        off += PAGE_SIZE;
    }
    true
}

#[global_allocator]
static HEAP: KernelHeap = KernelHeap {
    heap: SpinLock::new(Heap::new()),
    mapped_end: AtomicU64::new(0),
    oom: AtomicU64::new(0),
};

/// Mapeia o heap inicial. Guard pages: `KERNEL_HEAP_BASE - 4K` e o fim
/// mapeado nunca recebem mapeamento.
pub fn init() {
    if !map_range(KERNEL_HEAP_BASE, INITIAL_SIZE) {
        kerror!("heap: sem memoria para o heap inicial");
        cpu::halt_forever();
    }
    cpu::without_interrupts(|| {
        // SAFETY: região recém-mapeada, exclusiva do heap.
        unsafe {
            HEAP.heap
                .lock()
                .extend(KERNEL_HEAP_BASE as usize, INITIAL_SIZE as usize)
        };
    });
    HEAP.mapped_end
        .store(KERNEL_HEAP_BASE + INITIAL_SIZE, Ordering::Relaxed);
    kinfo!(
        "heap: {} KiB em {:#x} (max {} MiB, guard pages nas bordas)",
        INITIAL_SIZE >> 10,
        KERNEL_HEAP_BASE,
        KERNEL_HEAP_MAX_SIZE >> 20
    );
}

/// Estatísticas do heap.
pub fn stats() -> HeapStats {
    cpu::without_interrupts(|| HEAP.heap.lock().stats())
}

/// Extensão das estatísticas com dados que exigem o lock do heap.
pub trait HeapStatsExt {
    /// Maior bloco livre contíguo, em KiB.
    fn largest_free_kib(&self) -> usize;
}

impl HeapStatsExt for HeapStats {
    fn largest_free_kib(&self) -> usize {
        cpu::without_interrupts(|| HEAP.heap.lock().largest_free_block()) / 1024
    }
}

/// Alocações que falharam definitivamente (após tentar crescer).
pub fn oom_count() -> u64 {
    HEAP.oom.load(Ordering::Relaxed)
}

/// Bytes mapeados para o heap.
pub fn mapped_bytes() -> u64 {
    HEAP.mapped_end
        .load(Ordering::Relaxed)
        .saturating_sub(KERNEL_HEAP_BASE)
}

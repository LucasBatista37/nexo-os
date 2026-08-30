//! Memória física: normalização do mapa de boot e alocador de quadros global.

use core::sync::atomic::{AtomicUsize, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_boot_abi::{MemoryKind, MemoryRegion};
use nexo_mm::{
    BitmapFrameAllocator, FrameAllocator, FrameError, FrameStats, MapSummary, PAGE_SIZE, PhysAddr,
    normalize, summarize,
};
use nexo_sync::SpinLock;

use crate::cell::StaticCell;

const MAX_REGIONS: usize = 600;

static ALLOCATOR: SpinLock<Option<BitmapFrameAllocator<'static>>> = SpinLock::new(None);
static REGIONS: StaticCell<[MemoryRegion; MAX_REGIONS]> =
    StaticCell::new([MemoryRegion::EMPTY; MAX_REGIONS]);
static REGION_COUNT: AtomicUsize = AtomicUsize::new(0);
static SUMMARY: SpinLock<MapSummary> = SpinLock::new(MapSummary {
    usable_bytes: 0,
    kernel_bytes: 0,
    reserved_bytes: 0,
    max_addr: 0,
    max_usable_addr: 0,
    regions: 0,
});

/// Constrói o mapa normalizado e o alocador de quadros.
pub fn init() {
    let bi = crate::boot::info();
    let raw = crate::boot::memory_map();

    // Entrada = mapa do loader + reservas sintéticas do kernel.
    let mut input = [MemoryRegion::EMPTY; MAX_REGIONS];
    let mut n = 0;
    for r in raw.iter().take(MAX_REGIONS - 2) {
        input[n] = *r;
        n += 1;
    }
    // Primeiro MiB: BIOS/legado; reservado para futuro trampolim SMP.
    input[n] = MemoryRegion::new(0, 0x10_0000, MemoryKind::Reserved);
    n += 1;
    if bi.framebuffer.is_present() {
        input[n] = MemoryRegion::new(
            bi.framebuffer.base,
            bi.framebuffer.base + bi.framebuffer.size,
            MemoryKind::Framebuffer,
        );
        n += 1;
    }

    // SAFETY: inicialização única em uma CPU; depois só há leituras.
    let regions = unsafe { &mut *REGIONS.as_ptr() };
    let count = match normalize(&input[..n], regions) {
        Ok(c) => c,
        Err(e) => {
            kerror!("mm: mapa de memoria invalido: {e:?}");
            cpu::halt_forever();
        }
    };
    REGION_COUNT.store(count, Ordering::Release);
    let regions = &regions[..count];
    let summary = summarize(regions);
    *SUMMARY.lock() = summary;

    kinfo!(
        "mm: mapa de memoria normalizado ({} regioes brutas -> {}):",
        raw.len(),
        count
    );
    for r in regions {
        kinfo!("mm:   {r}");
    }
    kinfo!(
        "mm: {} MiB utilizaveis, {} KiB do kernel, {} MiB reservados, maior endereco {:#x}",
        summary.usable_bytes >> 20,
        summary.kernel_bytes >> 10,
        summary.reserved_bytes >> 20,
        summary.max_addr
    );

    // Bitmap: precisa de memória antes de existir um alocador — usa o início
    // da primeira região utilizável grande o bastante (acessada pelo physmap).
    let frames = summary.max_usable_addr / PAGE_SIZE;
    let words = BitmapFrameAllocator::words_for(frames);
    let bytes = nexo_mm::align_up((words * 8) as u64, PAGE_SIZE);
    let Some(home) = regions
        .iter()
        .find(|r| r.kind() == MemoryKind::Usable && r.len() >= bytes)
    else {
        kerror!("mm: nenhuma regiao utilizavel comporta o bitmap ({bytes} bytes)");
        cpu::halt_forever();
    };
    let storage_phys = PhysAddr::new(home.start);
    let storage_ptr = crate::mm::virt::phys_to_virt(storage_phys).as_mut_ptr::<u64>();
    // SAFETY: região utilizável, dentro do physmap, exclusiva do bitmap a partir daqui.
    let storage: &'static mut [u64] =
        unsafe { core::slice::from_raw_parts_mut(storage_ptr, words) };
    let mut alloc = BitmapFrameAllocator::new(frames, storage);
    for r in regions.iter().filter(|r| r.kind().is_usable_after_boot()) {
        if let Err(e) = alloc.mark_usable(PhysAddr::new(r.start), PhysAddr::new(r.end)) {
            kwarn!("mm: regiao {r} ignorada: {e:?}");
        }
    }
    alloc
        .mark_used(storage_phys, storage_phys.add(bytes))
        .expect("bitmap alinhado");
    let st = alloc.stats();
    kinfo!(
        "mm: frame allocator: {} quadros gerenciados, {} livres ({} MiB); bitmap de {} KiB em {:#x}",
        st.capacity,
        st.free,
        (st.free * PAGE_SIZE) >> 20,
        bytes >> 10,
        storage_phys
    );
    *ALLOCATOR.lock() = Some(alloc);
}

/// Mapa normalizado.
pub fn regions() -> &'static [MemoryRegion] {
    let n = REGION_COUNT.load(Ordering::Acquire);
    // SAFETY: após `init`, o array é imutável.
    let all: &'static [MemoryRegion; MAX_REGIONS] = unsafe { &*REGIONS.as_ptr() };
    &all[..n]
}

/// Resumo do mapa.
pub fn summary() -> MapSummary {
    *SUMMARY.lock()
}

/// Aloca um quadro de 4 KiB (não zerado).
pub fn allocate_frame() -> Option<PhysAddr> {
    cpu::without_interrupts(|| ALLOCATOR.lock().as_mut()?.allocate())
}

/// Aloca um quadro zerado.
pub fn allocate_zeroed_frame() -> Option<PhysAddr> {
    let f = allocate_frame()?;
    let p = crate::mm::virt::phys_to_virt(f).as_mut_ptr::<u8>();
    // SAFETY: quadro recém-alocado, dentro do physmap.
    unsafe { core::ptr::write_bytes(p, 0, PAGE_SIZE as usize) };
    Some(f)
}

/// Libera um quadro.
pub fn free_frame(f: PhysAddr) -> Result<(), FrameError> {
    cpu::without_interrupts(|| match ALLOCATOR.lock().as_mut() {
        Some(a) => a.free(f),
        None => Err(FrameError::OutOfRange(f)),
    })
}

/// Estatísticas do alocador.
pub fn stats() -> FrameStats {
    cpu::without_interrupts(|| {
        ALLOCATOR
            .lock()
            .as_ref()
            .map(|a| a.stats())
            .unwrap_or_default()
    })
}

/// Adaptador para o `Mapper` (tabelas intermediárias vêm daqui).
pub struct GlobalFrameAllocator;

impl FrameAllocator for GlobalFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysAddr> {
        allocate_frame()
    }
    fn deallocate_frame(&mut self, frame: PhysAddr) {
        let _ = free_frame(frame);
    }
}

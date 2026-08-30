//! Memória virtual do kernel: mapper sobre a PML4 ativa, acessada pelo physmap.

use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::paging::{MapError, Mapper, PageFlags, PageTableEntry, Translation};
use nexo_boot_abi::PHYS_MAP_OFFSET;
use nexo_mm::{PhysAddr, PhysToVirt, VirtAddr};
use nexo_sync::SpinLock;

use super::phys::GlobalFrameAllocator;

/// Tradução física → virtual pelo mapeamento linear.
#[derive(Clone, Copy)]
pub struct PhysMap;

impl PhysToVirt for PhysMap {
    fn phys_to_virt(&self, p: PhysAddr) -> *mut u8 {
        (PHYS_MAP_OFFSET + p.as_u64()) as *mut u8
    }
}

/// Endereço virtual (physmap) de um endereço físico.
pub fn phys_to_virt(p: PhysAddr) -> VirtAddr {
    VirtAddr::new(PHYS_MAP_OFFSET + p.as_u64())
}

/// Serializa edições das tabelas de página.
static LOCK: SpinLock<()> = SpinLock::new(());

fn kernel_mapper() -> Mapper<PhysMap> {
    let root = PhysAddr::new(cpu::read_cr3() & 0x000f_ffff_ffff_f000);
    // SAFETY: CR3 aponta para a PML4 construída pelo loader, coberta pelo physmap.
    unsafe { Mapper::new(root, PhysMap) }
}

unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __data_start: u8;
    static __bss_end: u8;
}

/// Limites de uma seção do kernel (símbolos do linker script).
pub fn section(name: &str) -> Option<(u64, u64)> {
    // Apenas endereços dos símbolos do linker; nada é lido.
    Some(match name {
        "text" => (&raw const __text_start as u64, &raw const __text_end as u64),
        "rodata" => (
            &raw const __rodata_start as u64,
            &raw const __rodata_end as u64,
        ),
        "data" => (&raw const __data_start as u64, &raw const __bss_end as u64),
        _ => return None,
    })
}

/// Remove o alias identidade do loader, liga CR0.WP e valida W^X das seções.
pub fn init() {
    cpu::without_interrupts(|| {
        let _g = LOCK.lock();
        let mut m = kernel_mapper();
        if m.pml4_entry(0).is_present() {
            // SAFETY: o kernel executa e usa pilha apenas na metade superior.
            unsafe { m.set_pml4_entry(0, PageTableEntry::empty()) };
            cpu::flush_tlb_all();
            kinfo!("virt: alias identidade do loader (PML4[0]) removido");
        }
    });
    // SAFETY: seções somente-leitura do kernel foram mapeadas sem WRITABLE.
    unsafe { cpu::enable_write_protect() };
    kinfo!(
        "virt: CR0.WP ativo, EFER.NXE={}, CR3={:#x}",
        cpu::nx_enabled(),
        cpu::read_cr3()
    );

    for name in ["text", "rodata", "data"] {
        let (s, e) = section(name).unwrap();
        let t = translate(VirtAddr::new(s)).expect("secao mapeada");
        kinfo!(
            "virt: .{name:<6} {s:#x}..{e:#x} ({} KiB) flags {:?}",
            (e - s) >> 10,
            t.flags
        );
    }
}

/// Mapeia `virt` → `phys` (4 KiB) e invalida o TLB.
pub fn map_page(virt: VirtAddr, phys: PhysAddr, flags: PageFlags) -> Result<(), MapError> {
    cpu::without_interrupts(|| {
        let _g = LOCK.lock();
        kernel_mapper().map_4k(virt, phys, flags, &mut GlobalFrameAllocator)?;
        cpu::invlpg(virt.as_u64());
        Ok(())
    })
}

/// Desmapeia e devolve o quadro (não o libera).
pub fn unmap_page(virt: VirtAddr) -> Result<PhysAddr, MapError> {
    cpu::without_interrupts(|| {
        let _g = LOCK.lock();
        let p = kernel_mapper().unmap_4k(virt)?;
        cpu::invlpg(virt.as_u64());
        Ok(p)
    })
}

/// Altera flags de uma página mapeada.
pub fn update_flags(virt: VirtAddr, flags: PageFlags) -> Result<(), MapError> {
    cpu::without_interrupts(|| {
        let _g = LOCK.lock();
        kernel_mapper().update_flags_4k(virt, flags)?;
        cpu::invlpg(virt.as_u64());
        Ok(())
    })
}

/// Traduz um endereço virtual.
pub fn translate(virt: VirtAddr) -> Option<Translation> {
    cpu::without_interrupts(|| {
        let _g = LOCK.lock();
        kernel_mapper().translate(virt)
    })
}

/// Aloca um quadro zerado e o mapeia em `virt`.
pub fn alloc_and_map(virt: VirtAddr, flags: PageFlags) -> Result<PhysAddr, MapError> {
    let frame = super::phys::allocate_zeroed_frame().ok_or(MapError::OutOfFrames)?;
    match map_page(virt, frame, flags) {
        Ok(()) => Ok(frame),
        Err(e) => {
            let _ = super::phys::free_frame(frame);
            Err(e)
        }
    }
}

/// Desmapeia `virt` e libera o quadro.
pub fn unmap_and_free(virt: VirtAddr) -> Result<(), MapError> {
    let frame = unmap_page(virt)?;
    let _ = super::phys::free_frame(frame);
    Ok(())
}

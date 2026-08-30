//! Acesso ao `BootInfo` entregue pelo loader.

use core::sync::atomic::{AtomicBool, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_boot_abi::{BootInfo, MemoryRegion, cmdline_flag, cmdline_value};
use nexo_mm::PhysAddr;
use nexo_sync::Once;

static INFO: Once<&'static BootInfo> = Once::new();
static TEST_MODE: AtomicBool = AtomicBool::new(false);

/// Valida e publica o `BootInfo`; aplica `loglevel=` e detecta modo de teste.
pub fn init(bi: &'static BootInfo) {
    if let Err(e) = bi.validate() {
        kerror!("BootInfo invalido: {e}");
        cpu::halt_forever();
    }
    let _ = INFO.set(bi);
    let cl = bi.cmdline();
    if let Some(l) = cmdline_value(cl, "loglevel").and_then(crate::klog::Level::parse) {
        crate::klog::set_level(l);
    }
    TEST_MODE.store(
        cmdline_value(cl, "test").is_some() || cmdline_flag(cl, "exit"),
        Ordering::Relaxed,
    );
    kinfo!("boot: ABI v{}, cmdline=\"{}\"", bi.version, cl);
    kinfo!(
        "boot: kernel fis {:#x} ({} KiB), ELF {} KiB, {} regioes, physmap {} MiB, RSDP {:#x}",
        bi.kernel_phys_base,
        bi.kernel_size >> 10,
        bi.kernel_file_len >> 10,
        bi.memory_map_len,
        bi.phys_map_size >> 20,
        bi.rsdp_addr
    );
    let fb = &bi.framebuffer;
    if fb.is_present() {
        kinfo!(
            "boot: framebuffer {}x{} stride {} {:?} em {:#x} ({} KiB)",
            fb.width,
            fb.height,
            fb.stride,
            fb.pixel_format(),
            fb.base,
            fb.size >> 10
        );
    } else {
        kinfo!("boot: sem framebuffer");
    }
}

/// `BootInfo` publicado.
pub fn info() -> &'static BootInfo {
    INFO.get().expect("boot::init nao foi chamado")
}

/// Mapa de memória cru entregue pelo loader.
pub fn memory_map() -> &'static [MemoryRegion] {
    let bi = info();
    let ptr = crate::mm::virt::phys_to_virt(PhysAddr::new(bi.memory_map_addr)).as_ptr();
    // SAFETY: o loader gravou `memory_map_len` regiões nesse endereço, em
    // páginas reservadas (BootInfo) que nunca são reutilizadas.
    unsafe { core::slice::from_raw_parts(ptr, bi.memory_map_len as usize) }
}

/// Linha de comando.
pub fn cmdline() -> &'static str {
    info().cmdline()
}

/// `true` quando o kernel deve encerrar o QEMU ao final (CI).
pub fn test_mode() -> bool {
    TEST_MODE.load(Ordering::Relaxed)
}

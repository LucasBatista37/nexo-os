//! nexo-loader — aplicação UEFI que prepara o ambiente do kernel.
//!
//! Sequência (ver docs/spec/boot-abi.md):
//! 1. inicializa serial e console UEFI;
//! 2. lê `\nexo\kernel.elf` e `\nexo\boot.cfg` da partição EFI;
//! 3. obtém framebuffer (GOP) e RSDP (ACPI);
//! 4. constrói tabelas de página: physmap em `PHYS_MAP_OFFSET` (2 MiB),
//!    alias identidade temporário, segmentos do kernel com W^X, pilha com guard page;
//! 5. reserva `BootInfo`, vetor de regiões e cópia do ELF (para símbolos);
//! 6. sai dos boot services, converte o mapa de memória e salta para o kernel.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write;
use log::{error, info, warn};
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::paging::{Mapper, PAGE_2M, PageFlags};
use nexo_arch_x86_64::serial::SerialPort;
use nexo_boot_abi::{
    BootInfo, FramebufferInfo, KERNEL_STACK_BASE, KERNEL_STACK_SIZE, KERNEL_STACK_TOP,
    KERNEL_VIRT_BASE, MAX_MEMORY_REGIONS, MemoryKind, MemoryRegion, PHYS_MAP_MIN_SIZE,
    PHYS_MAP_OFFSET, PixelFormat,
};
use nexo_elf::ElfFile;
use nexo_mm::{FrameAllocator, PAGE_SIZE, PhysAddr, PhysToVirt, VirtAddr, align_down, align_up};
use uefi::boot::{self, AllocateType, MemoryType, OpenProtocolAttributes, OpenProtocolParams};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat as GopPixelFormat};
use uefi::table::cfg::{ACPI_GUID, ACPI2_GUID};
use uefi::{CStr16, cstr16};

const MT_KERNEL_IMAGE: MemoryType = MemoryType::custom(0x8000_0000);
const MT_PAGE_TABLES: MemoryType = MemoryType::custom(0x8000_0001);
const MT_KERNEL_STACK: MemoryType = MemoryType::custom(0x8000_0002);
const MT_BOOT_INFO: MemoryType = MemoryType::custom(0x8000_0003);
const MT_KERNEL_FILE: MemoryType = MemoryType::custom(0x8000_0004);
const MT_INITRD: MemoryType = MemoryType::custom(0x8000_0005);

const KERNEL_PATH: &CStr16 = cstr16!("\\nexo\\kernel.elf");
const CONFIG_PATH: &CStr16 = cstr16!("\\nexo\\boot.cfg");
const INIT_PATH: &CStr16 = cstr16!("\\nexo\\init.elf");
const LOADER_VERSION: &str = env!("CARGO_PKG_VERSION");

static SERIAL: SerialPort = SerialPort::new(SerialPort::COM1);

/// Escreve na serial (funciona antes e depois de ExitBootServices).
macro_rules! sprintln {
    ($($arg:tt)*) => {{
        let mut s = SERIAL;
        let _ = writeln!(s, "nexo-loader: {}", format_args!($($arg)*));
    }};
}

/// Alocador de quadros que pede páginas ao firmware (tipo `MT_PAGE_TABLES`).
struct UefiFrameAllocator {
    pages: usize,
}

impl FrameAllocator for UefiFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysAddr> {
        let p = boot::allocate_pages(AllocateType::AnyPages, MT_PAGE_TABLES, 1).ok()?;
        self.pages += 1;
        Some(PhysAddr::new(p.as_ptr() as u64))
    }
    fn deallocate_frame(&mut self, _frame: PhysAddr) {}
}

/// Antes de ExitBootServices o firmware mantém identidade física = virtual.
#[derive(Clone, Copy)]
struct Identity;

impl PhysToVirt for Identity {
    fn phys_to_virt(&self, p: PhysAddr) -> *mut u8 {
        p.as_u64() as *mut u8
    }
}

fn alloc_pages(ty: MemoryType, count: usize) -> uefi::Result<PhysAddr> {
    let p = boot::allocate_pages(AllocateType::AnyPages, ty, count)?;
    // SAFETY: páginas recém-alocadas, identidade-mapeadas, de nossa propriedade.
    unsafe { core::ptr::write_bytes(p.as_ptr(), 0, count * PAGE_SIZE as usize) };
    Ok(PhysAddr::new(p.as_ptr() as u64))
}

fn read_file(path: &CStr16) -> uefi::Result<Vec<u8>> {
    use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
    let mut sfs = boot::get_image_file_system(boot::image_handle())?;
    let mut root = sfs.open_volume()?;
    let handle = root.open(path, FileMode::Read, FileAttribute::empty())?;
    let mut file = handle.into_regular_file().ok_or(uefi::Status::NOT_FOUND)?;
    let info = file.get_boxed_info::<FileInfo>()?;
    let size = info.file_size() as usize;
    let mut buf = alloc::vec![0u8; size];
    let mut read = 0;
    while read < size {
        let n = file.read(&mut buf[read..])?;
        if n == 0 {
            break;
        }
        read += n;
    }
    buf.truncate(read);
    Ok(buf)
}

fn framebuffer_info() -> FramebufferInfo {
    let Ok(handle) = boot::get_handle_for_protocol::<GraphicsOutput>() else {
        warn!("GOP ausente: sem framebuffer");
        return FramebufferInfo::default();
    };
    let params = OpenProtocolParams {
        handle,
        agent: boot::image_handle(),
        controller: None,
    };
    // SAFETY: GetProtocol não toma posse exclusiva; o console do firmware pode
    // continuar desenhando até ExitBootServices — aceitável para o loader.
    let gop = unsafe {
        boot::open_protocol::<GraphicsOutput>(params, OpenProtocolAttributes::GetProtocol)
    };
    let mut gop = match gop {
        Ok(g) => g,
        Err(e) => {
            warn!("GOP nao pode ser aberto: {e:?}");
            return FramebufferInfo::default();
        }
    };
    let mode = gop.current_mode_info();
    let (w, h) = mode.resolution();
    let format = match mode.pixel_format() {
        GopPixelFormat::Rgb => PixelFormat::Rgbx8888,
        GopPixelFormat::Bgr => PixelFormat::Bgrx8888,
        _ => PixelFormat::Unknown,
    };
    let mut fb = gop.frame_buffer();
    FramebufferInfo {
        base: fb.as_mut_ptr() as u64,
        size: fb.size() as u64,
        width: w as u32,
        height: h as u32,
        stride: mode.stride() as u32,
        format: format as u32,
        bytes_per_pixel: 4,
        reserved: 0,
    }
}

/// Pinta uma faixa no topo do framebuffer: prova visual de que o loader rodou.
fn paint_banner(fb: &FramebufferInfo, progress: u32) {
    if !fb.is_present() || fb.pixel_format() == PixelFormat::Unknown {
        return;
    }
    let (r, g, b) = (0x1eu32, 0x3au32, 0x5fu32);
    let px = match fb.pixel_format() {
        PixelFormat::Rgbx8888 => r | (g << 8) | (b << 16),
        _ => b | (g << 8) | (r << 16),
    };
    let bar_h = 24.min(fb.height);
    let width = fb.width * progress / 100;
    for y in 0..bar_h {
        for x in 0..width {
            let off = (y as u64 * fb.stride as u64 + x as u64) * 4;
            // SAFETY: dentro do framebuffer linear (stride * height * 4 <= size).
            unsafe { ((fb.base + off) as *mut u32).write_volatile(px) };
        }
    }
}

fn find_rsdp() -> u64 {
    uefi::system::with_config_table(|entries| {
        let mut acpi1 = 0u64;
        let mut acpi2 = 0u64;
        for e in entries {
            if e.guid == ACPI2_GUID {
                acpi2 = e.address as u64;
            } else if e.guid == ACPI_GUID {
                acpi1 = e.address as u64;
            }
        }
        if acpi2 != 0 { acpi2 } else { acpi1 }
    })
}

fn kind_for(ty: MemoryType) -> MemoryKind {
    match ty {
        MemoryType::CONVENTIONAL => MemoryKind::Usable,
        MemoryType::LOADER_CODE
        | MemoryType::LOADER_DATA
        | MemoryType::BOOT_SERVICES_CODE
        | MemoryType::BOOT_SERVICES_DATA => MemoryKind::LoaderReclaimable,
        MemoryType::RUNTIME_SERVICES_CODE | MemoryType::RUNTIME_SERVICES_DATA => {
            MemoryKind::UefiRuntime
        }
        MemoryType::ACPI_RECLAIM => MemoryKind::AcpiReclaimable,
        MemoryType::ACPI_NON_VOLATILE => MemoryKind::AcpiNvs,
        MemoryType::MMIO | MemoryType::MMIO_PORT_SPACE => MemoryKind::Mmio,
        MT_KERNEL_IMAGE => MemoryKind::KernelImage,
        MT_PAGE_TABLES => MemoryKind::KernelPageTables,
        MT_KERNEL_STACK => MemoryKind::KernelStack,
        MT_BOOT_INFO => MemoryKind::BootInfo,
        MT_KERNEL_FILE => MemoryKind::KernelFile,
        MT_INITRD => MemoryKind::Initrd,
        _ => MemoryKind::Reserved,
    }
}

/// Primeira linha não vazia e não comentada do boot.cfg.
fn parse_cmdline(cfg: &[u8]) -> &str {
    core::str::from_utf8(cfg)
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("")
}

struct LoadedKernel {
    entry: u64,
    phys_base: u64,
    size: u64,
}

fn load_kernel(
    elf: &ElfFile<'_>,
    mapper: &mut Mapper<Identity>,
    alloc: &mut UefiFrameAllocator,
) -> Result<LoadedKernel, &'static str> {
    let mut phys_base = u64::MAX;
    let mut size = 0u64;
    for ph in elf.load_segments() {
        if ph.p_vaddr < KERNEL_VIRT_BASE {
            return Err("segmento abaixo de KERNEL_VIRT_BASE");
        }
        if ph.writable() && ph.executable() {
            return Err("segmento W+X viola W^X");
        }
        let data = elf
            .segment_data(&ph)
            .map_err(|_| "segmento fora do arquivo")?;
        let vstart = align_down(ph.p_vaddr, PAGE_SIZE);
        let vend = align_up(ph.p_vaddr + ph.p_memsz, PAGE_SIZE);
        let pages = ((vend - vstart) / PAGE_SIZE) as usize;
        let phys = alloc_pages(MT_KERNEL_IMAGE, pages).map_err(|_| "sem memoria para segmento")?;
        // SAFETY: destino tem `pages` páginas zeradas; deslocamento dentro do segmento.
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                (phys.as_u64() + (ph.p_vaddr - vstart)) as *mut u8,
                data.len(),
            );
        }
        let mut flags = PageFlags::PRESENT;
        if ph.writable() {
            flags |= PageFlags::WRITABLE;
        }
        if !ph.executable() {
            flags |= PageFlags::NO_EXECUTE;
        }
        mapper
            .map_range_4k(VirtAddr::new(vstart), phys, vend - vstart, flags, alloc)
            .map_err(|_| "falha ao mapear segmento (segmentos sobrepostos?)")?;
        sprintln!(
            "segmento {:#x}..{:#x} -> fis {:#x} ({} paginas, r{}{})",
            vstart,
            vend,
            phys.as_u64(),
            pages,
            if ph.writable() { "w" } else { "-" },
            if ph.executable() { "x" } else { "-" },
        );
        phys_base = phys_base.min(phys.as_u64());
        size += vend - vstart;
    }
    if size == 0 {
        return Err("nenhum segmento PT_LOAD");
    }
    Ok(LoadedKernel {
        entry: elf.entry,
        phys_base,
        size,
    })
}

/// Copia `data` para páginas do tipo `ty`. Devolve o endereço físico.
fn copy_to_pages(ty: MemoryType, data: &[u8]) -> uefi::Result<PhysAddr> {
    let pages = data.len().div_ceil(PAGE_SIZE as usize).max(1);
    let phys = alloc_pages(ty, pages)?;
    // SAFETY: destino tem `pages` páginas zeradas.
    unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), phys.as_u64() as *mut u8, data.len()) };
    Ok(phys)
}

/// Salta para o kernel com a nova PML4, pilha e `RDI = boot_info`.
///
/// # Safety
/// As tabelas devem mapear este código (alias identidade), a pilha e o kernel.
unsafe fn jump_to_kernel(entry: u64, pml4: u64, stack_top: u64, boot_info: u64) -> ! {
    // SAFETY: contrato da função; registradores explícitos evitam conflitos.
    unsafe {
        core::arch::asm!(
            "cli",
            "mov cr3, rax",
            "mov rsp, rcx",
            "xor ebp, ebp",
            "jmp rdx",
            in("rax") pml4,
            in("rcx") stack_top,
            in("rdi") boot_info,
            in("rdx") entry,
            options(noreturn)
        )
    }
}

fn run() -> Result<(), &'static str> {
    // SAFETY: COM1 é a porta serial padrão do PC; inicializar é idempotente.
    let serial_ok = unsafe { SERIAL.init() };
    sprintln!(
        "v{LOADER_VERSION} iniciando (serial loopback: {})",
        if serial_ok { "ok" } else { "falhou" }
    );
    info!("Nexo OS loader v{LOADER_VERSION}");

    if !cpu::supports_nx() {
        return Err("CPU sem suporte a NX");
    }

    // Arquivos.
    let kernel_bytes =
        read_file(KERNEL_PATH).map_err(|_| "nao foi possivel ler \\nexo\\kernel.elf")?;
    info!("kernel.elf: {} bytes", kernel_bytes.len());
    let cfg = read_file(CONFIG_PATH).unwrap_or_default();
    let cmdline = parse_cmdline(&cfg);
    info!("cmdline: \"{cmdline}\"");
    let elf = ElfFile::parse(&kernel_bytes).map_err(|_| "kernel.elf invalido")?;
    let init_bytes = read_file(INIT_PATH).unwrap_or_default();
    if init_bytes.is_empty() {
        info!("init.elf ausente: kernel sem espaco de usuario inicial");
    } else {
        info!("init.elf: {} bytes", init_bytes.len());
    }

    // Plataforma.
    let fb = framebuffer_info();
    if fb.is_present() {
        info!(
            "framebuffer: {}x{} stride {} fmt {:?} @ {:#x}",
            fb.width,
            fb.height,
            fb.stride,
            fb.pixel_format(),
            fb.base
        );
    }
    paint_banner(&fb, 25);
    let rsdp = find_rsdp();
    info!("RSDP: {rsdp:#x}");

    // Estende o physmap até cobrir toda a memória reportada + framebuffer.
    // Só RAM conta: janelas MMIO de 64 bits (PCIe) ficam fora e serão mapeadas sob demanda.
    let mut max_phys = PHYS_MAP_MIN_SIZE;
    {
        let mm = boot::memory_map(MemoryType::LOADER_DATA).map_err(|_| "memory map")?;
        for d in mm.entries() {
            if kind_for(d.ty).is_usable_after_boot() || d.ty == MemoryType::ACPI_RECLAIM {
                max_phys = max_phys.max(d.phys_start + d.page_count * PAGE_SIZE);
            }
        }
    }
    if fb.is_present() {
        max_phys = max_phys.max(fb.base + fb.size);
    }
    let phys_map_size = align_up(max_phys, PAGE_2M * 512);
    info!("physmap: {} MiB", phys_map_size >> 20);

    // Tabelas de página.
    let mut alloc = UefiFrameAllocator { pages: 0 };
    let root = alloc.allocate_frame().ok_or("sem memoria para PML4")?;
    // SAFETY: `root` é uma página zerada; antes de ExitBootServices o mapeamento é identidade.
    unsafe { core::ptr::write_bytes(root.as_u64() as *mut u8, 0, PAGE_SIZE as usize) };
    // SAFETY: idem.
    let mut mapper = unsafe { Mapper::new(root, Identity) };
    let mut off = 0;
    while off < phys_map_size {
        mapper
            .map_2m(
                VirtAddr::new(PHYS_MAP_OFFSET + off),
                PhysAddr::new(off),
                PageFlags::KERNEL_RW,
                &mut alloc,
            )
            .map_err(|_| "falha ao mapear physmap")?;
        off += PAGE_2M;
    }
    // Mapeamento identidade temporário (RX, 4 KiB) apenas da imagem do loader: é de
    // onde executa o `mov cr3` e o `jmp`. O physmap é NX, logo não serve para isso.
    // O kernel remove a entrada PML4[0] logo no início (mm::virt::init).
    {
        let img = boot::open_protocol_exclusive::<uefi::proto::loaded_image::LoadedImage>(
            boot::image_handle(),
        )
        .map_err(|_| "LoadedImage")?;
        let (base, size) = img.info();
        let start = align_down(base as u64, PAGE_SIZE);
        let end = align_up(base as u64 + size, PAGE_SIZE);
        mapper
            .map_range_4k(
                VirtAddr::new(start),
                PhysAddr::new(start),
                end - start,
                PageFlags::KERNEL_RX,
                &mut alloc,
            )
            .map_err(|_| "falha ao mapear identidade do loader")?;
        info!("loader em {start:#x}..{end:#x} mapeado em identidade (RX) para o salto");
    }
    paint_banner(&fb, 50);

    // Kernel.
    let kernel = load_kernel(&elf, &mut mapper, &mut alloc)?;
    info!(
        "kernel: entry {:#x}, {} KiB em {:#x}",
        kernel.entry,
        kernel.size >> 10,
        kernel.phys_base
    );

    // Pilha com guard page abaixo (KERNEL_STACK_BASE - 4K fica sem mapeamento).
    let stack_pages = (KERNEL_STACK_SIZE / PAGE_SIZE) as usize;
    let stack_phys =
        alloc_pages(MT_KERNEL_STACK, stack_pages).map_err(|_| "sem memoria para pilha")?;
    mapper
        .map_range_4k(
            VirtAddr::new(KERNEL_STACK_BASE),
            stack_phys,
            KERNEL_STACK_SIZE,
            PageFlags::KERNEL_RW,
            &mut alloc,
        )
        .map_err(|_| "falha ao mapear pilha")?;

    // Cópia do ELF para símbolos e initrd (init.elf).
    let file_phys = copy_to_pages(MT_KERNEL_FILE, &kernel_bytes)
        .map_err(|_| "sem memoria para copia do ELF")?;
    let initrd_phys = if init_bytes.is_empty() {
        PhysAddr::new(0)
    } else {
        copy_to_pages(MT_INITRD, &init_bytes).map_err(|_| "sem memoria para o initrd")?
    };

    // BootInfo + vetor de regiões (4 páginas: 1 + 3).
    let regions_bytes = MAX_MEMORY_REGIONS * core::mem::size_of::<MemoryRegion>();
    let bi_pages = 1 + regions_bytes.div_ceil(PAGE_SIZE as usize);
    let bi_phys = alloc_pages(MT_BOOT_INFO, bi_pages).map_err(|_| "sem memoria para BootInfo")?;
    let regions_phys = bi_phys.add(PAGE_SIZE);
    let bi_ptr = bi_phys.as_u64() as *mut BootInfo;
    let mut bi = BootInfo::empty();
    bi.memory_map_addr = regions_phys.as_u64();
    bi.memory_map_capacity = MAX_MEMORY_REGIONS as u32;
    bi.phys_map_size = phys_map_size;
    bi.kernel_phys_base = kernel.phys_base;
    bi.kernel_size = kernel.size;
    bi.kernel_file_addr = file_phys.as_u64();
    bi.kernel_file_len = kernel_bytes.len() as u64;
    bi.initrd_addr = initrd_phys.as_u64();
    bi.initrd_len = init_bytes.len() as u64;
    bi.page_table_root = root.as_u64();
    bi.rsdp_addr = rsdp;
    bi.framebuffer = fb;
    bi.set_cmdline(cmdline);
    info!(
        "tabelas de pagina: {} quadros; BootInfo em {:#x}",
        alloc.pages,
        bi_phys.as_u64()
    );
    paint_banner(&fb, 75);
    sprintln!("saindo dos boot services");

    // A partir daqui: sem alocação, sem log UEFI, sem serviços de boot.
    // SAFETY: nenhum handle/protocolo de boot services é usado depois deste ponto.
    let mm = unsafe { boot::exit_boot_services(Some(MT_BOOT_INFO)) };
    let regions = regions_phys.as_u64() as *mut MemoryRegion;
    let mut n = 0usize;
    for d in mm.entries() {
        if n >= MAX_MEMORY_REGIONS {
            break;
        }
        let start = d.phys_start;
        let end = start + d.page_count * PAGE_SIZE;
        // SAFETY: `regions` tem capacidade MAX_MEMORY_REGIONS.
        unsafe {
            regions
                .add(n)
                .write(MemoryRegion::new(start, end, kind_for(d.ty)))
        };
        n += 1;
    }
    bi.memory_map_len = n as u32;
    // SAFETY: página de BootInfo alocada e zerada acima.
    unsafe { bi_ptr.write(bi) };

    // SAFETY: CPU suporta NX (verificado); tabelas usam o bit NX.
    unsafe { cpu::enable_nx() };
    paint_banner(&fb, 100);
    sprintln!(
        "{} regioes de memoria; saltando para o kernel em {:#x}",
        n,
        kernel.entry
    );

    let boot_info_virt = PHYS_MAP_OFFSET + bi_phys.as_u64();
    // SAFETY: PML4 mapeia physmap (+ alias identidade), kernel e pilha.
    unsafe {
        jump_to_kernel(
            kernel.entry,
            root.as_u64(),
            KERNEL_STACK_TOP,
            boot_info_virt,
        )
    }
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().expect("helpers");
    match run() {
        Ok(()) => Status::SUCCESS,
        Err(e) => {
            error!("falha: {e}");
            sprintln!("FALHA: {e}");
            boot::stall(5_000_000);
            Status::LOAD_ERROR
        }
    }
}

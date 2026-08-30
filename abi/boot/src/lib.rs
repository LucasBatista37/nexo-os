//! ABI de boot do Nexo OS.
//!
//! Este crate define o contrato binário entre o loader UEFI (`boot/loader`) e o
//! kernel (`kernel/`). A especificação em prosa está em `docs/spec/boot-abi.md`;
//! qualquer mudança aqui exige incrementar [`BOOT_ABI_VERSION`] e atualizar o
//! documento.
//!
//! Regras:
//! - todas as estruturas são `#[repr(C)]` e independentes de alocação;
//! - todos os endereços são **físicos**, exceto onde indicado;
//! - o kernel acessa memória física através do mapeamento linear em
//!   [`PHYS_MAP_OFFSET`], construído pelo loader antes do salto.
#![no_std]
#![deny(unsafe_code)]

use core::fmt;

/// Assinatura colocada em `BootInfo::magic` (bytes ASCII "NEXOBOOT" em little-endian).
pub const BOOT_INFO_MAGIC: u64 = 0x544f_4f42_4f58_454e;

/// Versão do contrato de boot. Incrementar em qualquer mudança incompatível.
pub const BOOT_ABI_VERSION: u32 = 1;

/// Tamanho de página base da arquitetura.
pub const PAGE_SIZE: u64 = 4096;

/// Base do mapeamento linear de toda a memória física (physmap) no espaço do kernel.
pub const PHYS_MAP_OFFSET: u64 = 0xffff_8000_0000_0000;

/// Tamanho mínimo coberto pelo physmap (garante MMIO clássico < 4 GiB).
pub const PHYS_MAP_MIN_SIZE: u64 = 4 * 1024 * 1024 * 1024;

/// Endereço virtual onde o kernel é carregado (topo de 2 GiB — code model `kernel`).
pub const KERNEL_VIRT_BASE: u64 = 0xffff_ffff_8000_0000;

/// Base da pilha inicial do kernel. A página imediatamente abaixo é uma guard page.
pub const KERNEL_STACK_BASE: u64 = 0xffff_ffff_7fe0_0000;

/// Tamanho da pilha inicial do kernel.
pub const KERNEL_STACK_SIZE: u64 = 64 * 1024;

/// Topo (exclusivo) da pilha inicial do kernel.
pub const KERNEL_STACK_TOP: u64 = KERNEL_STACK_BASE + KERNEL_STACK_SIZE;

/// Base do heap do kernel (mapeado pelo próprio kernel; guard pages nas bordas).
pub const KERNEL_HEAP_BASE: u64 = 0xffff_ffff_c000_0000;

/// Tamanho máximo do heap do kernel nesta versão.
pub const KERNEL_HEAP_MAX_SIZE: u64 = 64 * 1024 * 1024;

/// Tamanho máximo da linha de comando.
pub const MAX_CMDLINE_LEN: usize = 256;

/// Número máximo de regiões que o loader entrega ao kernel.
pub const MAX_MEMORY_REGIONS: usize = 512;

/// Classificação de uma região de memória física.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum MemoryKind {
    /// Valor inválido/desconhecido (nunca deve aparecer em um mapa válido).
    Unknown = 0,
    /// RAM livre para uso do kernel.
    Usable = 1,
    /// Reservado pelo firmware/plataforma; nunca tocar.
    Reserved = 2,
    /// Tabelas ACPI; pode ser recuperado após o kernel copiá-las.
    AcpiReclaimable = 3,
    /// ACPI NVS; deve ser preservado (suspensão).
    AcpiNvs = 4,
    /// Registradores de dispositivos mapeados em memória.
    Mmio = 5,
    /// Código/dados dos runtime services UEFI; preservar.
    UefiRuntime = 6,
    /// Boot services e imagem do loader; livre após o kernel assumir.
    LoaderReclaimable = 7,
    /// Segmentos do kernel carregados.
    KernelImage = 8,
    /// Tabelas de página construídas pelo loader.
    KernelPageTables = 9,
    /// Pilha inicial do kernel.
    KernelStack = 10,
    /// Estrutura [`BootInfo`] e o vetor de regiões.
    BootInfo = 11,
    /// Cópia do arquivo ELF do kernel (usada para símbolos/backtrace).
    KernelFile = 12,
    /// Memória do framebuffer (quando presente no mapa).
    Framebuffer = 13,
}

impl MemoryKind {
    /// Converte um `u32` cru do mapa de boot.
    pub const fn from_u32(v: u32) -> MemoryKind {
        match v {
            1 => MemoryKind::Usable,
            2 => MemoryKind::Reserved,
            3 => MemoryKind::AcpiReclaimable,
            4 => MemoryKind::AcpiNvs,
            5 => MemoryKind::Mmio,
            6 => MemoryKind::UefiRuntime,
            7 => MemoryKind::LoaderReclaimable,
            8 => MemoryKind::KernelImage,
            9 => MemoryKind::KernelPageTables,
            10 => MemoryKind::KernelStack,
            11 => MemoryKind::BootInfo,
            12 => MemoryKind::KernelFile,
            13 => MemoryKind::Framebuffer,
            _ => MemoryKind::Unknown,
        }
    }

    /// `true` se o kernel pode alocar quadros desta região depois de assumir o controle.
    pub const fn is_usable_after_boot(self) -> bool {
        matches!(self, MemoryKind::Usable | MemoryKind::LoaderReclaimable)
    }

    /// Prioridade ao resolver sobreposições: o maior valor vence (mais restritivo).
    pub const fn priority(self) -> u8 {
        match self {
            MemoryKind::Unknown => 0,
            MemoryKind::Usable => 1,
            MemoryKind::LoaderReclaimable => 2,
            MemoryKind::AcpiReclaimable => 3,
            MemoryKind::Framebuffer => 4,
            MemoryKind::KernelFile => 5,
            MemoryKind::BootInfo => 6,
            MemoryKind::KernelStack => 7,
            MemoryKind::KernelPageTables => 8,
            MemoryKind::KernelImage => 9,
            MemoryKind::UefiRuntime => 10,
            MemoryKind::AcpiNvs => 11,
            MemoryKind::Mmio => 12,
            MemoryKind::Reserved => 13,
        }
    }

    /// Nome legível, para logs.
    pub const fn name(self) -> &'static str {
        match self {
            MemoryKind::Unknown => "unknown",
            MemoryKind::Usable => "usable",
            MemoryKind::Reserved => "reserved",
            MemoryKind::AcpiReclaimable => "acpi-reclaim",
            MemoryKind::AcpiNvs => "acpi-nvs",
            MemoryKind::Mmio => "mmio",
            MemoryKind::UefiRuntime => "uefi-runtime",
            MemoryKind::LoaderReclaimable => "loader-reclaim",
            MemoryKind::KernelImage => "kernel-image",
            MemoryKind::KernelPageTables => "kernel-pagetables",
            MemoryKind::KernelStack => "kernel-stack",
            MemoryKind::BootInfo => "boot-info",
            MemoryKind::KernelFile => "kernel-file",
            MemoryKind::Framebuffer => "framebuffer",
        }
    }
}

/// Uma região física `[start, end)`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MemoryRegion {
    /// Início (inclusivo), endereço físico.
    pub start: u64,
    /// Fim (exclusivo), endereço físico.
    pub end: u64,
    /// [`MemoryKind`] como `u32`.
    pub kind: u32,
    /// Reservado; deve ser zero.
    pub reserved: u32,
}

impl MemoryRegion {
    /// Região vazia.
    pub const EMPTY: MemoryRegion = MemoryRegion {
        start: 0,
        end: 0,
        kind: 0,
        reserved: 0,
    };

    /// Cria uma região.
    pub const fn new(start: u64, end: u64, kind: MemoryKind) -> MemoryRegion {
        MemoryRegion {
            start,
            end,
            kind: kind as u32,
            reserved: 0,
        }
    }

    /// Tipo da região.
    pub const fn kind(&self) -> MemoryKind {
        MemoryKind::from_u32(self.kind)
    }

    /// Tamanho em bytes.
    pub const fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    /// `true` se a região não cobre nenhum byte.
    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// `true` se `addr` está dentro da região.
    pub const fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }
}

impl fmt::Display for MemoryRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:#018x}-{:#018x} {:>10} KiB {}",
            self.start,
            self.end,
            self.len() / 1024,
            self.kind().name()
        )
    }
}

/// Formato de pixel do framebuffer (ordem dos bytes em memória).
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    /// Formato desconhecido; o kernel não deve desenhar.
    Unknown = 0,
    /// Bytes `R, G, B, x` — 32 bits por pixel.
    Rgbx8888 = 1,
    /// Bytes `B, G, R, x` — 32 bits por pixel.
    Bgrx8888 = 2,
}

impl PixelFormat {
    /// Converte um `u32` cru.
    pub const fn from_u32(v: u32) -> PixelFormat {
        match v {
            1 => PixelFormat::Rgbx8888,
            2 => PixelFormat::Bgrx8888,
            _ => PixelFormat::Unknown,
        }
    }
}

/// Descrição do framebuffer linear obtido via GOP.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FramebufferInfo {
    /// Endereço físico do início do framebuffer (0 = ausente).
    pub base: u64,
    /// Tamanho em bytes.
    pub size: u64,
    /// Largura em pixels.
    pub width: u32,
    /// Altura em pixels.
    pub height: u32,
    /// Pixels por linha (pode ser maior que `width`).
    pub stride: u32,
    /// [`PixelFormat`] como `u32`.
    pub format: u32,
    /// Bytes por pixel.
    pub bytes_per_pixel: u32,
    /// Reservado; deve ser zero.
    pub reserved: u32,
}

impl FramebufferInfo {
    /// `true` se há um framebuffer utilizável.
    pub const fn is_present(&self) -> bool {
        self.base != 0 && self.width != 0 && self.height != 0
    }

    /// Formato de pixel.
    pub const fn pixel_format(&self) -> PixelFormat {
        PixelFormat::from_u32(self.format)
    }
}

/// Estrutura entregue ao kernel em `RDI` (ponteiro **virtual**, dentro do physmap).
#[repr(C)]
#[derive(Debug)]
pub struct BootInfo {
    /// [`BOOT_INFO_MAGIC`].
    pub magic: u64,
    /// [`BOOT_ABI_VERSION`].
    pub version: u32,
    /// `size_of::<BootInfo>()` no momento da escrita, para detectar incompatibilidade.
    pub size: u32,

    /// Endereço físico do vetor de [`MemoryRegion`].
    pub memory_map_addr: u64,
    /// Número de regiões válidas.
    pub memory_map_len: u32,
    /// Capacidade do vetor.
    pub memory_map_capacity: u32,

    /// Base virtual do physmap (igual a [`PHYS_MAP_OFFSET`]).
    pub phys_map_offset: u64,
    /// Quantidade de bytes físicos cobertos pelo physmap a partir de 0.
    pub phys_map_size: u64,

    /// Endereço físico do primeiro segmento do kernel.
    pub kernel_phys_base: u64,
    /// Endereço virtual do kernel (igual a [`KERNEL_VIRT_BASE`]).
    pub kernel_virt_base: u64,
    /// Tamanho total (bytes) ocupado pelos segmentos do kernel.
    pub kernel_size: u64,

    /// Endereço físico da cópia do arquivo ELF do kernel.
    pub kernel_file_addr: u64,
    /// Tamanho em bytes do arquivo ELF.
    pub kernel_file_len: u64,

    /// Base virtual da pilha inicial do kernel.
    pub stack_base: u64,
    /// Tamanho da pilha inicial.
    pub stack_size: u64,

    /// Endereço físico da PML4 ativa no salto.
    pub page_table_root: u64,
    /// Endereço físico da tabela RSDP (ACPI), ou 0.
    pub rsdp_addr: u64,

    /// Framebuffer.
    pub framebuffer: FramebufferInfo,

    /// Bytes válidos em `cmdline`.
    pub cmdline_len: u32,
    /// Reservado; deve ser zero.
    pub reserved: u32,
    /// Linha de comando (UTF-8, sem terminador).
    pub cmdline: [u8; MAX_CMDLINE_LEN],
}

/// Erros de validação de [`BootInfo`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BootInfoError {
    /// `magic` incorreto.
    BadMagic(u64),
    /// Versão incompatível.
    BadVersion(u32),
    /// Tamanho inconsistente.
    BadSize(u32),
    /// Mapa de memória vazio ou maior que a capacidade.
    BadMemoryMap,
    /// Physmap não configurado conforme a especificação.
    BadPhysMap,
    /// Linha de comando inválida.
    BadCmdline,
}

impl fmt::Display for BootInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootInfoError::BadMagic(m) => write!(f, "magic invalido: {m:#x}"),
            BootInfoError::BadVersion(v) => write!(f, "versao de ABI nao suportada: {v}"),
            BootInfoError::BadSize(s) => write!(f, "tamanho de BootInfo inconsistente: {s}"),
            BootInfoError::BadMemoryMap => write!(f, "mapa de memoria invalido"),
            BootInfoError::BadPhysMap => write!(f, "physmap invalido"),
            BootInfoError::BadCmdline => write!(f, "linha de comando invalida"),
        }
    }
}

impl BootInfo {
    /// Cria uma estrutura zerada com magic e versão preenchidos.
    pub const fn empty() -> BootInfo {
        BootInfo {
            magic: BOOT_INFO_MAGIC,
            version: BOOT_ABI_VERSION,
            size: core::mem::size_of::<BootInfo>() as u32,
            memory_map_addr: 0,
            memory_map_len: 0,
            memory_map_capacity: 0,
            phys_map_offset: PHYS_MAP_OFFSET,
            phys_map_size: 0,
            kernel_phys_base: 0,
            kernel_virt_base: KERNEL_VIRT_BASE,
            kernel_size: 0,
            kernel_file_addr: 0,
            kernel_file_len: 0,
            stack_base: KERNEL_STACK_BASE,
            stack_size: KERNEL_STACK_SIZE,
            page_table_root: 0,
            rsdp_addr: 0,
            framebuffer: FramebufferInfo {
                base: 0,
                size: 0,
                width: 0,
                height: 0,
                stride: 0,
                format: 0,
                bytes_per_pixel: 0,
                reserved: 0,
            },
            cmdline_len: 0,
            reserved: 0,
            cmdline: [0; MAX_CMDLINE_LEN],
        }
    }

    /// Valida os campos invariantes do contrato.
    pub fn validate(&self) -> Result<(), BootInfoError> {
        if self.magic != BOOT_INFO_MAGIC {
            return Err(BootInfoError::BadMagic(self.magic));
        }
        if self.version != BOOT_ABI_VERSION {
            return Err(BootInfoError::BadVersion(self.version));
        }
        if self.size as usize != core::mem::size_of::<BootInfo>() {
            return Err(BootInfoError::BadSize(self.size));
        }
        if self.memory_map_len == 0
            || self.memory_map_len > self.memory_map_capacity
            || self.memory_map_addr == 0
        {
            return Err(BootInfoError::BadMemoryMap);
        }
        if self.phys_map_offset != PHYS_MAP_OFFSET || self.phys_map_size < PHYS_MAP_MIN_SIZE {
            return Err(BootInfoError::BadPhysMap);
        }
        if self.cmdline_len as usize > MAX_CMDLINE_LEN
            || core::str::from_utf8(&self.cmdline[..self.cmdline_len as usize]).is_err()
        {
            return Err(BootInfoError::BadCmdline);
        }
        Ok(())
    }

    /// Linha de comando como `&str` (vazia se inválida).
    pub fn cmdline(&self) -> &str {
        let len = (self.cmdline_len as usize).min(MAX_CMDLINE_LEN);
        core::str::from_utf8(&self.cmdline[..len]).unwrap_or("")
    }

    /// Copia uma linha de comando para a estrutura (trunca se necessário).
    pub fn set_cmdline(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let mut len = bytes.len().min(MAX_CMDLINE_LEN);
        // Não cortar no meio de um caractere UTF-8.
        while len > 0 && !s.is_char_boundary(len) {
            len -= 1;
        }
        self.cmdline[..len].copy_from_slice(&bytes[..len]);
        self.cmdline_len = len as u32;
    }

    /// Procura `key=value` na linha de comando e devolve `value`.
    pub fn cmdline_value<'a>(&'a self, key: &str) -> Option<&'a str> {
        cmdline_value(self.cmdline(), key)
    }
}

/// Procura `key=value` em uma linha de comando separada por espaços.
pub fn cmdline_value<'a>(cmdline: &'a str, key: &str) -> Option<&'a str> {
    cmdline.split_ascii_whitespace().find_map(|tok| {
        let (k, v) = tok.split_once('=')?;
        (k == key).then_some(v)
    })
}

/// `true` se a flag `key` (sem `=`) aparece na linha de comando.
pub fn cmdline_flag(cmdline: &str, key: &str) -> bool {
    cmdline.split_ascii_whitespace().any(|tok| tok == key)
}

/// Assinatura da função de entrada do kernel (System V AMD64: argumento em `RDI`).
#[cfg(target_arch = "x86_64")]
pub type KernelEntry = unsafe extern "sysv64" fn(*const BootInfo) -> !;

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn layout_is_stable() {
        // Estes números fazem parte do contrato; mudar exige nova versão de ABI.
        assert_eq!(core::mem::size_of::<MemoryRegion>(), 24);
        assert_eq!(core::mem::size_of::<FramebufferInfo>(), 40);
        assert_eq!(core::mem::size_of::<BootInfo>(), 424);
        assert_eq!(core::mem::align_of::<BootInfo>(), 8);
    }

    #[test]
    fn magic_spells_nexoboot() {
        assert_eq!(&BOOT_INFO_MAGIC.to_le_bytes(), b"NEXOBOOT");
    }

    #[test]
    fn validate_rejects_bad_fields() {
        let mut bi = BootInfo::empty();
        assert_eq!(bi.validate(), Err(BootInfoError::BadMemoryMap));
        bi.memory_map_addr = 0x1000;
        bi.memory_map_len = 1;
        bi.memory_map_capacity = 1;
        assert_eq!(bi.validate(), Err(BootInfoError::BadPhysMap));
        bi.phys_map_size = PHYS_MAP_MIN_SIZE;
        assert_eq!(bi.validate(), Ok(()));
        bi.magic = 0;
        assert_eq!(bi.validate(), Err(BootInfoError::BadMagic(0)));
    }

    #[test]
    fn cmdline_roundtrip() {
        let mut bi = BootInfo::empty();
        bi.set_cmdline("test=panic loglevel=debug  exit");
        assert_eq!(bi.cmdline_value("test"), Some("panic"));
        assert_eq!(bi.cmdline_value("loglevel"), Some("debug"));
        assert_eq!(bi.cmdline_value("nope"), None);
        assert!(cmdline_flag(bi.cmdline(), "exit"));
        assert!(!cmdline_flag(bi.cmdline(), "test"));
    }

    #[test]
    fn cmdline_truncates_on_char_boundary() {
        let mut bi = BootInfo::empty();
        let long = alloc_string(300);
        bi.set_cmdline(&long);
        assert!(bi.cmdline_len as usize <= MAX_CMDLINE_LEN);
        assert!(core::str::from_utf8(&bi.cmdline[..bi.cmdline_len as usize]).is_ok());
    }

    fn alloc_string(n: usize) -> std::string::String {
        core::iter::repeat_n('é', n).collect()
    }

    #[test]
    fn region_display_and_priority() {
        let r = MemoryRegion::new(0x1000, 0x3000, MemoryKind::Usable);
        assert_eq!(r.len(), 0x2000);
        assert!(r.contains(0x2fff));
        assert!(!r.contains(0x3000));
        assert!(MemoryKind::Reserved.priority() > MemoryKind::Usable.priority());
        assert!(MemoryKind::KernelImage.priority() > MemoryKind::LoaderReclaimable.priority());
        assert_eq!(MemoryKind::from_u32(99), MemoryKind::Unknown);
    }
}

//! Paginação de 4 níveis (PML4 → PDPT → PD → PT) com páginas de 4 KiB e 2 MiB.
//!
//! [`Mapper`] opera sobre tabelas físicas através de um [`PhysToVirt`], por
//! isso é usado pelo loader (identidade), pelo kernel (physmap) e pelos
//! testes no host (arena). Ele nunca invalida TLB: o chamador faz isso.

use core::fmt;
use core::ops::{BitOr, BitOrAssign};
use nexo_mm::{FrameAllocator, PAGE_SIZE, PhysAddr, PhysToVirt, VirtAddr};

/// Entradas por tabela.
pub const ENTRIES: usize = 512;
/// Tamanho de página grande (nível 2).
pub const PAGE_2M: u64 = 2 * 1024 * 1024;
/// Tamanho de página enorme (nível 3).
pub const PAGE_1G: u64 = 1024 * 1024 * 1024;

const ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;

/// Bits de flags de uma entrada de tabela de páginas.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct PageFlags(u64);

impl PageFlags {
    /// Sem flags.
    pub const NONE: PageFlags = PageFlags(0);
    /// Presente.
    pub const PRESENT: PageFlags = PageFlags(1 << 0);
    /// Gravável.
    pub const WRITABLE: PageFlags = PageFlags(1 << 1);
    /// Acessível em ring 3.
    pub const USER: PageFlags = PageFlags(1 << 2);
    /// Write-through.
    pub const WRITE_THROUGH: PageFlags = PageFlags(1 << 3);
    /// Cache desabilitado.
    pub const NO_CACHE: PageFlags = PageFlags(1 << 4);
    /// Acessada (setada pela CPU).
    pub const ACCESSED: PageFlags = PageFlags(1 << 5);
    /// Suja (setada pela CPU).
    pub const DIRTY: PageFlags = PageFlags(1 << 6);
    /// Página grande (níveis 2 e 3).
    pub const HUGE: PageFlags = PageFlags(1 << 7);
    /// Global (não invalidada em troca de CR3).
    pub const GLOBAL: PageFlags = PageFlags(1 << 8);
    /// Não executável (exige EFER.NXE).
    pub const NO_EXECUTE: PageFlags = PageFlags(1 << 63);

    /// Kernel, somente leitura, sem execução.
    pub const KERNEL_RO: PageFlags = PageFlags(Self::PRESENT.0 | Self::NO_EXECUTE.0);
    /// Kernel, leitura e escrita, sem execução.
    pub const KERNEL_RW: PageFlags =
        PageFlags(Self::PRESENT.0 | Self::WRITABLE.0 | Self::NO_EXECUTE.0);
    /// Kernel, leitura e execução.
    pub const KERNEL_RX: PageFlags = PageFlags(Self::PRESENT.0);

    /// Sem flags.
    pub const fn empty() -> Self {
        PageFlags(0)
    }
    /// Valor cru (apenas bits de flag).
    pub const fn bits(self) -> u64 {
        self.0 & !ADDR_MASK
    }
    /// Constrói a partir de bits crus (bits de endereço são descartados).
    pub const fn from_bits(bits: u64) -> Self {
        PageFlags(bits & !ADDR_MASK)
    }
    /// `true` se todos os bits de `other` estão presentes.
    pub const fn contains(self, other: PageFlags) -> bool {
        self.0 & other.0 == other.0
    }
    /// União.
    pub const fn union(self, other: PageFlags) -> Self {
        PageFlags(self.0 | other.0)
    }
    /// Remove os bits de `other`.
    pub const fn without(self, other: PageFlags) -> Self {
        PageFlags(self.0 & !other.0)
    }
}

impl BitOr for PageFlags {
    type Output = PageFlags;
    fn bitor(self, rhs: PageFlags) -> PageFlags {
        self.union(rhs)
    }
}
impl BitOrAssign for PageFlags {
    fn bitor_assign(&mut self, rhs: PageFlags) {
        self.0 |= rhs.0;
    }
}
impl fmt::Debug for PageFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = [
            (Self::PRESENT, "P"),
            (Self::WRITABLE, "W"),
            (Self::USER, "U"),
            (Self::WRITE_THROUGH, "WT"),
            (Self::NO_CACHE, "NC"),
            (Self::ACCESSED, "A"),
            (Self::DIRTY, "D"),
            (Self::HUGE, "H"),
            (Self::GLOBAL, "G"),
            (Self::NO_EXECUTE, "NX"),
        ];
        let mut first = true;
        f.write_str("[")?;
        for (flag, name) in names {
            if self.contains(flag) {
                if !first {
                    f.write_str("|")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        f.write_str("]")
    }
}

/// Entrada de tabela de páginas.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    /// Entrada vazia (não presente).
    pub const fn empty() -> Self {
        PageTableEntry(0)
    }
    /// Constrói com endereço e flags.
    pub const fn new(addr: PhysAddr, flags: PageFlags) -> Self {
        PageTableEntry((addr.as_u64() & ADDR_MASK) | flags.bits())
    }
    /// Valor cru.
    pub const fn raw(self) -> u64 {
        self.0
    }
    /// `true` se presente.
    pub const fn is_present(self) -> bool {
        self.0 & 1 != 0
    }
    /// `true` se é uma página grande/enorme.
    pub const fn is_huge(self) -> bool {
        self.0 & (1 << 7) != 0
    }
    /// Endereço físico apontado.
    pub const fn addr(self) -> PhysAddr {
        PhysAddr::new(self.0 & ADDR_MASK)
    }
    /// Flags.
    pub const fn flags(self) -> PageFlags {
        PageFlags::from_bits(self.0)
    }
}

impl fmt::Debug for PageTableEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PTE({} {:?})", self.addr(), self.flags())
    }
}

/// Uma tabela de páginas (4 KiB, alinhada).
#[repr(C, align(4096))]
pub struct PageTable {
    /// Entradas.
    pub entries: [PageTableEntry; ENTRIES],
}

impl PageTable {
    /// Tabela vazia.
    pub const fn new() -> Self {
        PageTable {
            entries: [PageTableEntry::empty(); ENTRIES],
        }
    }
    /// Zera todas as entradas.
    pub fn zero(&mut self) {
        self.entries = [PageTableEntry::empty(); ENTRIES];
    }
}

impl Default for PageTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Erros de mapeamento.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapError {
    /// Já existe mapeamento no endereço.
    AlreadyMapped(VirtAddr),
    /// Não há mapeamento no endereço.
    NotMapped(VirtAddr),
    /// O alocador de quadros esgotou.
    OutOfFrames,
    /// Uma página grande cobre o caminho.
    HugePageInWay(VirtAddr),
    /// Endereço não alinhado ao tamanho da página.
    Unaligned(VirtAddr),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::AlreadyMapped(v) => write!(f, "{v} ja mapeado"),
            MapError::NotMapped(v) => write!(f, "{v} nao mapeado"),
            MapError::OutOfFrames => write!(f, "sem quadros livres"),
            MapError::HugePageInWay(v) => write!(f, "pagina grande cobre {v}"),
            MapError::Unaligned(v) => write!(f, "{v} nao alinhado"),
        }
    }
}

/// Resultado de uma tradução.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Translation {
    /// Endereço físico correspondente.
    pub phys: PhysAddr,
    /// Flags da entrada folha.
    pub flags: PageFlags,
    /// Tamanho da página (4 KiB, 2 MiB ou 1 GiB).
    pub page_size: u64,
}

/// Manipulador de uma hierarquia de tabelas cuja raiz é `root`.
pub struct Mapper<T: PhysToVirt> {
    root: PhysAddr,
    translate: T,
}

impl<T: PhysToVirt> Mapper<T> {
    /// Cria um mapper sobre a PML4 em `root`.
    ///
    /// # Safety
    /// `root` deve apontar para uma PML4 válida (ou zerada) e `translate`
    /// deve dar acesso a toda memória física usada pelas tabelas.
    pub unsafe fn new(root: PhysAddr, translate: T) -> Self {
        Mapper { root, translate }
    }

    /// Endereço físico da PML4.
    pub fn root(&self) -> PhysAddr {
        self.root
    }

    fn entry_ptr(&self, table: PhysAddr, index: usize) -> *mut PageTableEntry {
        debug_assert!(index < ENTRIES);
        let base = self.translate.phys_to_virt(table) as *mut PageTableEntry;
        // SAFETY: `index < 512` mantém o ponteiro dentro da tabela de 4 KiB.
        unsafe { base.add(index) }
    }

    fn read(&self, table: PhysAddr, index: usize) -> PageTableEntry {
        // SAFETY: tabela válida por invariante do Mapper; leitura volátil evita
        // que o compilador fund leituras de memória que a CPU também altera (A/D).
        unsafe { core::ptr::read_volatile(self.entry_ptr(table, index)) }
    }

    fn write(&mut self, table: PhysAddr, index: usize, e: PageTableEntry) {
        // SAFETY: idem; escrita alinhada em entrada da tabela.
        unsafe { core::ptr::write_volatile(self.entry_ptr(table, index), e) }
    }

    /// Lê a entrada `index` da PML4.
    pub fn pml4_entry(&self, index: usize) -> PageTableEntry {
        self.read(self.root, index)
    }

    /// Escreve a entrada `index` da PML4 (usado para alias/remoção de mapeamentos inteiros).
    ///
    /// # Safety
    /// Alterar entradas de topo pode desmapear código em execução.
    pub unsafe fn set_pml4_entry(&mut self, index: usize, e: PageTableEntry) {
        self.write(self.root, index, e);
    }

    /// Desce até a tabela do nível `target`, criando tabelas intermediárias.
    fn walk_create(
        &mut self,
        virt: VirtAddr,
        target: u8,
        alloc: &mut dyn FrameAllocator,
        user: bool,
    ) -> Result<PhysAddr, MapError> {
        let inter = if user {
            PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER
        } else {
            PageFlags::PRESENT | PageFlags::WRITABLE
        };
        let mut table = self.root;
        let mut level = 4u8;
        while level > target {
            let idx = virt.table_index(level);
            let e = self.read(table, idx);
            if e.is_present() {
                if e.is_huge() {
                    return Err(MapError::HugePageInWay(virt));
                }
                table = e.addr();
            } else {
                let frame = alloc.allocate_frame().ok_or(MapError::OutOfFrames)?;
                let p = self.translate.phys_to_virt(frame);
                // SAFETY: quadro recém-alocado, exclusivo, de 4 KiB.
                unsafe { core::ptr::write_bytes(p, 0, PAGE_SIZE as usize) };
                self.write(table, idx, PageTableEntry::new(frame, inter));
                table = frame;
            }
            level -= 1;
        }
        Ok(table)
    }

    /// Desce até a tabela do nível `target` sem criar nada.
    fn walk(&self, virt: VirtAddr, target: u8) -> Result<PhysAddr, MapError> {
        let mut table = self.root;
        let mut level = 4u8;
        while level > target {
            let e = self.read(table, virt.table_index(level));
            if !e.is_present() {
                return Err(MapError::NotMapped(virt));
            }
            if e.is_huge() {
                return Err(MapError::HugePageInWay(virt));
            }
            table = e.addr();
            level -= 1;
        }
        Ok(table)
    }

    /// Mapeia uma página de 4 KiB.
    pub fn map_4k(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageFlags,
        alloc: &mut dyn FrameAllocator,
    ) -> Result<(), MapError> {
        if !virt.is_aligned(PAGE_SIZE) || !phys.is_aligned(PAGE_SIZE) {
            return Err(MapError::Unaligned(virt));
        }
        let table = self.walk_create(virt, 1, alloc, flags.contains(PageFlags::USER))?;
        let idx = virt.table_index(1);
        if self.read(table, idx).is_present() {
            return Err(MapError::AlreadyMapped(virt));
        }
        self.write(
            table,
            idx,
            PageTableEntry::new(phys, flags | PageFlags::PRESENT),
        );
        Ok(())
    }

    /// Mapeia uma página de 2 MiB.
    pub fn map_2m(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        flags: PageFlags,
        alloc: &mut dyn FrameAllocator,
    ) -> Result<(), MapError> {
        if !virt.is_aligned(PAGE_2M) || !phys.is_aligned(PAGE_2M) {
            return Err(MapError::Unaligned(virt));
        }
        let table = self.walk_create(virt, 2, alloc, flags.contains(PageFlags::USER))?;
        let idx = virt.table_index(2);
        if self.read(table, idx).is_present() {
            return Err(MapError::AlreadyMapped(virt));
        }
        self.write(
            table,
            idx,
            PageTableEntry::new(phys, flags | PageFlags::PRESENT | PageFlags::HUGE),
        );
        Ok(())
    }

    /// Mapeia `[virt, virt+len)` em `[phys, phys+len)` com páginas de 4 KiB.
    pub fn map_range_4k(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        len: u64,
        flags: PageFlags,
        alloc: &mut dyn FrameAllocator,
    ) -> Result<(), MapError> {
        let mut off = 0;
        while off < len {
            self.map_4k(virt.add(off), phys.add(off), flags, alloc)?;
            off += PAGE_SIZE;
        }
        Ok(())
    }

    /// Remove o mapeamento de 4 KiB em `virt` e devolve o quadro (não o libera).
    pub fn unmap_4k(&mut self, virt: VirtAddr) -> Result<PhysAddr, MapError> {
        if !virt.is_aligned(PAGE_SIZE) {
            return Err(MapError::Unaligned(virt));
        }
        let table = self.walk(virt, 1)?;
        let idx = virt.table_index(1);
        let e = self.read(table, idx);
        if !e.is_present() {
            return Err(MapError::NotMapped(virt));
        }
        self.write(table, idx, PageTableEntry::empty());
        Ok(e.addr())
    }

    /// Altera as flags de uma página de 4 KiB mapeada.
    pub fn update_flags_4k(&mut self, virt: VirtAddr, flags: PageFlags) -> Result<(), MapError> {
        if !virt.is_aligned(PAGE_SIZE) {
            return Err(MapError::Unaligned(virt));
        }
        let table = self.walk(virt, 1)?;
        let idx = virt.table_index(1);
        let e = self.read(table, idx);
        if !e.is_present() {
            return Err(MapError::NotMapped(virt));
        }
        self.write(
            table,
            idx,
            PageTableEntry::new(e.addr(), flags | PageFlags::PRESENT),
        );
        Ok(())
    }

    /// Traduz `virt` (qualquer tamanho de página).
    pub fn translate(&self, virt: VirtAddr) -> Option<Translation> {
        let mut table = self.root;
        let mut level = 4u8;
        loop {
            let e = self.read(table, virt.table_index(level));
            if !e.is_present() {
                return None;
            }
            if level == 1 {
                return Some(Translation {
                    phys: e.addr().add(virt.page_offset()),
                    flags: e.flags(),
                    page_size: PAGE_SIZE,
                });
            }
            if e.is_huge() && (level == 2 || level == 3) {
                let size = if level == 2 { PAGE_2M } else { PAGE_1G };
                let base = e.addr().as_u64() & !(size - 1);
                return Some(Translation {
                    phys: PhysAddr::new(base + (virt.as_u64() & (size - 1))),
                    flags: e.flags().without(PageFlags::HUGE),
                    page_size: size,
                });
            }
            table = e.addr();
            level -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    /// Arena: memória "física" simulada + alocador bump.
    struct Arena {
        buf: Vec<u8>,
        base: usize,
        next: u64,
        limit: u64,
    }
    impl Arena {
        fn new(frames: u64) -> Self {
            let buf = vec![0u8; (frames as usize + 1) * 4096];
            let base = (buf.as_ptr() as usize + 4095) & !4095;
            Arena {
                buf,
                base,
                next: 0x1000,
                limit: frames * 4096,
            }
        }
    }
    #[derive(Clone, Copy)]
    struct Translate(usize);
    impl PhysToVirt for Translate {
        fn phys_to_virt(&self, p: PhysAddr) -> *mut u8 {
            (self.0 + p.as_u64() as usize) as *mut u8
        }
    }
    impl FrameAllocator for Arena {
        fn allocate_frame(&mut self) -> Option<PhysAddr> {
            if self.next >= self.limit {
                return None;
            }
            let f = self.next;
            self.next += 4096;
            Some(PhysAddr::new(f))
        }
        fn deallocate_frame(&mut self, _f: PhysAddr) {}
    }

    fn setup(frames: u64) -> (Arena, Mapper<Translate>) {
        let mut arena = Arena::new(frames);
        let _ = &arena.buf;
        let root = arena.allocate_frame().unwrap();
        // SAFETY: quadro zerado pela arena.
        let mapper = unsafe { Mapper::new(root, Translate(arena.base)) };
        (arena, mapper)
    }

    #[test]
    fn map_translate_unmap_4k() {
        let (mut a, mut m) = setup(64);
        let v = VirtAddr::new(0xffff_ffff_8000_0000);
        let p = PhysAddr::new(0x20_0000);
        m.map_4k(v, p, PageFlags::KERNEL_RW, &mut a).unwrap();
        let t = m.translate(v.add(0x123)).unwrap();
        assert_eq!(t.phys, p.add(0x123));
        assert_eq!(t.page_size, 4096);
        assert!(
            t.flags
                .contains(PageFlags::WRITABLE | PageFlags::NO_EXECUTE)
        );
        assert_eq!(
            m.map_4k(v, p, PageFlags::KERNEL_RW, &mut a),
            Err(MapError::AlreadyMapped(v))
        );
        assert_eq!(m.unmap_4k(v), Ok(p));
        assert!(m.translate(v).is_none());
        assert_eq!(m.unmap_4k(v), Err(MapError::NotMapped(v)));
        assert_eq!(
            m.map_4k(v.add(1), p, PageFlags::KERNEL_RW, &mut a),
            Err(MapError::Unaligned(v.add(1)))
        );
        // 3 tabelas intermediárias criadas (PDPT, PD, PT)
        assert_eq!(a.next, 0x1000 * 5);
    }

    #[test]
    fn huge_pages_and_conflicts() {
        let (mut a, mut m) = setup(64);
        let v = VirtAddr::new(0xffff_8000_0000_0000);
        m.map_2m(v, PhysAddr::new(0), PageFlags::KERNEL_RW, &mut a)
            .unwrap();
        let t = m.translate(v.add(0x12_3456)).unwrap();
        assert_eq!(t.phys, PhysAddr::new(0x12_3456));
        assert_eq!(t.page_size, PAGE_2M);
        assert!(!t.flags.contains(PageFlags::HUGE));
        assert_eq!(
            m.map_4k(
                v.add(0x1000),
                PhysAddr::new(0x9000),
                PageFlags::KERNEL_RW,
                &mut a
            ),
            Err(MapError::HugePageInWay(v.add(0x1000)))
        );
        assert_eq!(m.unmap_4k(v), Err(MapError::HugePageInWay(v)));
        assert_eq!(
            m.map_2m(v.add(0x1000), PhysAddr::new(0), PageFlags::NONE, &mut a),
            Err(MapError::Unaligned(v.add(0x1000)))
        );
    }

    #[test]
    fn update_flags_and_pml4_alias() {
        let (mut a, mut m) = setup(64);
        let v = VirtAddr::new(0xffff_8000_0000_0000);
        m.map_4k(v, PhysAddr::new(0x5000), PageFlags::KERNEL_RW, &mut a)
            .unwrap();
        m.update_flags_4k(v, PageFlags::KERNEL_RO).unwrap();
        let t = m.translate(v).unwrap();
        assert!(!t.flags.contains(PageFlags::WRITABLE));
        assert!(t.flags.contains(PageFlags::NO_EXECUTE));
        // Alias da entrada 256 na entrada 0 → identidade
        let e = m.pml4_entry(256);
        // SAFETY: teste em arena.
        unsafe { m.set_pml4_entry(0, e) };
        assert_eq!(
            m.translate(VirtAddr::new(0)).unwrap().phys,
            PhysAddr::new(0x5000)
        );
        // SAFETY: idem.
        unsafe { m.set_pml4_entry(0, PageTableEntry::empty()) };
        assert!(m.translate(VirtAddr::new(0)).is_none());
    }

    #[test]
    fn user_flag_propagates_to_tables() {
        let (mut a, mut m) = setup(64);
        let v = VirtAddr::new(0x40_0000);
        m.map_4k(
            v,
            PhysAddr::new(0x9000),
            PageFlags::PRESENT | PageFlags::USER,
            &mut a,
        )
        .unwrap();
        let e = m.pml4_entry(0);
        assert!(
            e.flags().contains(PageFlags::USER),
            "PML4 sem USER: {:?}",
            e.flags()
        );
        let k = VirtAddr::new(0xffff_ffff_8000_0000);
        m.map_4k(k, PhysAddr::new(0xa000), PageFlags::KERNEL_RW, &mut a)
            .unwrap();
        assert!(!m.pml4_entry(511).flags().contains(PageFlags::USER));
    }

    #[test]
    fn out_of_frames() {
        let (mut a, mut m) = setup(2); // raiz + 1 quadro
        let v = VirtAddr::new(0x1000);
        assert_eq!(
            m.map_4k(v, PhysAddr::new(0), PageFlags::KERNEL_RW, &mut a),
            Err(MapError::OutOfFrames)
        );
    }

    #[test]
    fn map_range() {
        let (mut a, mut m) = setup(64);
        let v = VirtAddr::new(0x4000_0000);
        m.map_range_4k(
            v,
            PhysAddr::new(0x10_0000),
            0x4000,
            PageFlags::KERNEL_RX,
            &mut a,
        )
        .unwrap();
        for i in 0..4 {
            let t = m.translate(v.add(i * 0x1000)).unwrap();
            assert_eq!(t.phys, PhysAddr::new(0x10_0000 + i * 0x1000));
            assert!(!t.flags.contains(PageFlags::NO_EXECUTE));
        }
        assert!(m.translate(v.add(4 * 0x1000)).is_none());
    }

    #[test]
    fn entry_encoding() {
        let e = PageTableEntry::new(PhysAddr::new(0x1234_5000), PageFlags::KERNEL_RW);
        assert_eq!(e.addr(), PhysAddr::new(0x1234_5000));
        assert_eq!(e.raw() & 1, 1);
        assert_eq!(e.raw() >> 63, 1);
        assert_eq!(std::format!("{:?}", e.flags()), "[P|W|NX]");
        assert_eq!(core::mem::size_of::<PageTable>(), 4096);
        assert_eq!(core::mem::align_of::<PageTable>(), 4096);
    }
}

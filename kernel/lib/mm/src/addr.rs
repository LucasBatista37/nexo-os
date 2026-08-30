//! Tipos de endereço.

use core::fmt;

/// Deslocamento de página (log2 de [`PAGE_SIZE`]).
pub const PAGE_SHIFT: u64 = 12;
/// Tamanho de página base.
pub const PAGE_SIZE: u64 = 1 << PAGE_SHIFT;

/// Arredonda `v` para baixo para múltiplo de `align` (potência de dois).
pub const fn align_down(v: u64, align: u64) -> u64 {
    debug_assert!(align.is_power_of_two());
    v & !(align - 1)
}

/// Arredonda `v` para cima para múltiplo de `align` (potência de dois); satura.
pub const fn align_up(v: u64, align: u64) -> u64 {
    debug_assert!(align.is_power_of_two());
    match v.checked_add(align - 1) {
        Some(x) => x & !(align - 1),
        None => !(align - 1),
    }
}

/// Endereço físico.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PhysAddr(u64);

impl PhysAddr {
    /// Endereço zero.
    pub const ZERO: PhysAddr = PhysAddr(0);

    /// Cria a partir de um `u64`.
    pub const fn new(v: u64) -> Self {
        PhysAddr(v)
    }
    /// Valor cru.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
    /// `true` se múltiplo de `align`.
    pub const fn is_aligned(self, align: u64) -> bool {
        self.0 & (align - 1) == 0
    }
    /// Arredonda para baixo.
    pub const fn align_down(self, align: u64) -> Self {
        PhysAddr(align_down(self.0, align))
    }
    /// Arredonda para cima.
    pub const fn align_up(self, align: u64) -> Self {
        PhysAddr(align_up(self.0, align))
    }
    /// Índice do quadro de 4 KiB.
    pub const fn frame_index(self) -> u64 {
        self.0 >> PAGE_SHIFT
    }
    /// Soma com verificação de overflow.
    pub const fn checked_add(self, off: u64) -> Option<Self> {
        match self.0.checked_add(off) {
            Some(v) => Some(PhysAddr(v)),
            None => None,
        }
    }
    /// Soma sem verificação (para deslocamentos pequenos e conhecidos).
    pub const fn add(self, off: u64) -> Self {
        PhysAddr(self.0 + off)
    }
}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#x})", self.0)
    }
}
impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}
impl fmt::LowerHex for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

/// Endereço virtual (canônico em x86_64: bits 48..64 iguais ao bit 47).
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VirtAddr(u64);

impl VirtAddr {
    /// Endereço zero.
    pub const ZERO: VirtAddr = VirtAddr(0);

    /// Cria a partir de um `u64` (sem verificar canonicidade).
    pub const fn new(v: u64) -> Self {
        VirtAddr(v)
    }
    /// Cria a partir de um ponteiro.
    pub fn from_ptr<T: ?Sized>(p: *const T) -> Self {
        VirtAddr(p as *const u8 as usize as u64)
    }
    /// Valor cru.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
    /// Como ponteiro mutável.
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as usize as *mut T
    }
    /// Como ponteiro constante.
    pub const fn as_ptr<T>(self) -> *const T {
        self.0 as usize as *const T
    }
    /// `true` se canônico para 48 bits.
    pub const fn is_canonical(self) -> bool {
        let top = self.0 >> 47;
        top == 0 || top == 0x1_ffff
    }
    /// `true` se múltiplo de `align`.
    pub const fn is_aligned(self, align: u64) -> bool {
        self.0 & (align - 1) == 0
    }
    /// Arredonda para baixo.
    pub const fn align_down(self, align: u64) -> Self {
        VirtAddr(align_down(self.0, align))
    }
    /// Arredonda para cima.
    pub const fn align_up(self, align: u64) -> Self {
        VirtAddr(align_up(self.0, align))
    }
    /// Soma sem verificação.
    pub const fn add(self, off: u64) -> Self {
        VirtAddr(self.0.wrapping_add(off))
    }
    /// Deslocamento dentro da página de 4 KiB.
    pub const fn page_offset(self) -> u64 {
        self.0 & (PAGE_SIZE - 1)
    }
    /// Índice de 9 bits do nível `level` (4 = PML4, 3 = PDPT, 2 = PD, 1 = PT).
    pub const fn table_index(self, level: u8) -> usize {
        ((self.0 >> (12 + 9 * (level as u64 - 1))) & 0x1ff) as usize
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#x})", self.0)
    }
}
impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}
impl fmt::LowerHex for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_helpers() {
        assert_eq!(align_down(0x1fff, 0x1000), 0x1000);
        assert_eq!(align_up(0x1001, 0x1000), 0x2000);
        assert_eq!(align_up(0x1000, 0x1000), 0x1000);
        assert_eq!(align_up(u64::MAX - 5, 0x1000), !0xfff);
        assert!(PhysAddr::new(0x2000).is_aligned(0x1000));
        assert!(!PhysAddr::new(0x2001).is_aligned(0x1000));
        assert_eq!(PhysAddr::new(0x5432).frame_index(), 5);
    }

    #[test]
    fn virt_indices() {
        let v = VirtAddr::new(0xffff_ffff_8000_1234);
        assert!(v.is_canonical());
        assert_eq!(v.table_index(4), 511);
        assert_eq!(v.table_index(3), 510);
        assert_eq!(v.table_index(2), 0);
        assert_eq!(v.table_index(1), 1);
        assert_eq!(v.page_offset(), 0x234);
        assert!(!VirtAddr::new(0x0000_8000_0000_0000).is_canonical());
        assert!(VirtAddr::new(0xffff_8000_0000_0000).is_canonical());
    }
}

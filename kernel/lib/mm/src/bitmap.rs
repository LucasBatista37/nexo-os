//! Alocador de quadros por bitmap (1 bit por quadro de 4 KiB; 1 = ocupado).

use crate::FrameAllocator;
use crate::addr::{PAGE_SIZE, PhysAddr};

/// Erros do alocador.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameError {
    /// Endereço não alinhado a página.
    Unaligned(PhysAddr),
    /// Quadro fora da faixa gerenciada.
    OutOfRange(PhysAddr),
    /// Liberação de quadro que já estava livre.
    DoubleFree(PhysAddr),
    /// Liberação de quadro que nunca foi marcado como utilizável (reservado).
    NotUsable(PhysAddr),
}

/// Estatísticas instantâneas.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FrameStats {
    /// Quadros gerenciáveis (tamanho do bitmap).
    pub capacity: u64,
    /// Quadros marcados como utilizáveis em algum momento.
    pub total_usable: u64,
    /// Quadros livres agora.
    pub free: u64,
    /// Alocações bem-sucedidas acumuladas.
    pub allocations: u64,
    /// Liberações acumuladas.
    pub frees: u64,
    /// Alocações que falharam por exaustão.
    pub failures: u64,
}

impl FrameStats {
    /// Quadros em uso (dentre os utilizáveis).
    pub fn used(&self) -> u64 {
        self.total_usable - self.free
    }
}

/// Alocador de quadros por bitmap sobre armazenamento fornecido pelo chamador.
///
/// Mantém dois bitmaps: `bits` (1 = ocupado/indisponível) e `usable`
/// (1 = RAM que o kernel pode usar). O segundo impede que uma liberação
/// errada devolva memória reservada ao pool.
pub struct BitmapFrameAllocator<'a> {
    bits: &'a mut [u64],
    usable: &'a mut [u64],
    capacity: u64,
    stats: FrameStats,
    hint: usize,
}

impl<'a> BitmapFrameAllocator<'a> {
    /// Palavras `u64` de armazenamento necessárias para `frames` quadros.
    pub const fn words_for(frames: u64) -> usize {
        2 * frames.div_ceil(64) as usize
    }

    /// Cria o alocador com todos os quadros marcados como indisponíveis.
    pub fn new(frames: u64, storage: &'a mut [u64]) -> Self {
        let words = Self::words_for(frames);
        assert!(storage.len() >= words, "bitmap: armazenamento insuficiente");
        let (bits, rest) = storage.split_at_mut(words / 2);
        let usable = &mut rest[..words / 2];
        bits.fill(u64::MAX);
        usable.fill(0);
        BitmapFrameAllocator {
            bits,
            usable,
            capacity: frames,
            stats: FrameStats {
                capacity: frames,
                ..Default::default()
            },
            hint: 0,
        }
    }

    /// Torna utilizáveis os quadros em `[start, end)` (alinhados a página).
    pub fn mark_usable(&mut self, start: PhysAddr, end: PhysAddr) -> Result<(), FrameError> {
        for f in self.frame_range(start, end)? {
            if !self.is_usable_frame(f) {
                self.usable[(f / 64) as usize] |= 1 << (f % 64);
                self.clear(f);
                self.stats.total_usable += 1;
                self.stats.free += 1;
            }
        }
        Ok(())
    }

    /// Marca os quadros em `[start, end)` como ocupados (ex.: bitmap, kernel).
    pub fn mark_used(&mut self, start: PhysAddr, end: PhysAddr) -> Result<(), FrameError> {
        for f in self.frame_range(start, end)? {
            if !self.test(f) {
                self.set(f);
                self.stats.free -= 1;
            }
        }
        Ok(())
    }

    fn frame_range(
        &self,
        start: PhysAddr,
        end: PhysAddr,
    ) -> Result<core::ops::Range<u64>, FrameError> {
        if !start.is_aligned(PAGE_SIZE) {
            return Err(FrameError::Unaligned(start));
        }
        if !end.is_aligned(PAGE_SIZE) {
            return Err(FrameError::Unaligned(end));
        }
        let (s, e) = (start.frame_index(), end.frame_index().min(self.capacity));
        Ok(s..e.max(s))
    }

    #[inline]
    fn is_usable_frame(&self, frame: u64) -> bool {
        self.usable[(frame / 64) as usize] & (1 << (frame % 64)) != 0
    }
    #[inline]
    fn test(&self, frame: u64) -> bool {
        self.bits[(frame / 64) as usize] & (1 << (frame % 64)) != 0
    }
    #[inline]
    fn set(&mut self, frame: u64) {
        self.bits[(frame / 64) as usize] |= 1 << (frame % 64);
    }
    #[inline]
    fn clear(&mut self, frame: u64) {
        self.bits[(frame / 64) as usize] &= !(1 << (frame % 64));
    }

    /// `true` se o quadro está livre.
    pub fn is_free(&self, frame: PhysAddr) -> bool {
        let idx = frame.frame_index();
        idx < self.capacity && !self.test(idx)
    }

    /// Aloca um quadro.
    pub fn allocate(&mut self) -> Option<PhysAddr> {
        let words = self.bits.len();
        for step in 0..words {
            let w = (self.hint + step) % words;
            let word = self.bits[w];
            if word != u64::MAX {
                let bit = word.trailing_ones() as u64;
                let frame = w as u64 * 64 + bit;
                if frame >= self.capacity {
                    continue;
                }
                self.set(frame);
                self.hint = w;
                self.stats.free -= 1;
                self.stats.allocations += 1;
                return Some(PhysAddr::new(frame * PAGE_SIZE));
            }
        }
        self.stats.failures += 1;
        None
    }

    /// Libera um quadro.
    pub fn free(&mut self, frame: PhysAddr) -> Result<(), FrameError> {
        if !frame.is_aligned(PAGE_SIZE) {
            return Err(FrameError::Unaligned(frame));
        }
        let idx = frame.frame_index();
        if idx >= self.capacity {
            return Err(FrameError::OutOfRange(frame));
        }
        if !self.is_usable_frame(idx) {
            return Err(FrameError::NotUsable(frame));
        }
        if !self.test(idx) {
            return Err(FrameError::DoubleFree(frame));
        }
        self.clear(idx);
        self.hint = self.hint.min((idx / 64) as usize);
        self.stats.free += 1;
        self.stats.frees += 1;
        Ok(())
    }

    /// Estatísticas.
    pub fn stats(&self) -> FrameStats {
        self.stats
    }
}

impl FrameAllocator for BitmapFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysAddr> {
        self.allocate()
    }
    fn deallocate_frame(&mut self, frame: PhysAddr) {
        if let Err(e) = self.free(frame) {
            panic!("frame allocator: liberacao invalida: {e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    fn p(v: u64) -> PhysAddr {
        PhysAddr::new(v)
    }

    #[test]
    fn allocates_all_then_exhausts_and_recovers() {
        let mut storage = vec![0u64; BitmapFrameAllocator::words_for(1024)];
        let mut a = BitmapFrameAllocator::new(1024, &mut storage);
        a.mark_usable(p(0x1000), p(1024 * 0x1000)).unwrap(); // quadro 0 fica reservado
        assert_eq!(a.stats().free, 1023);

        let mut got = Vec::new();
        while let Some(f) = a.allocate() {
            assert!(f.is_aligned(PAGE_SIZE));
            assert_ne!(f, p(0));
            got.push(f);
        }
        assert_eq!(got.len(), 1023);
        got.sort();
        got.dedup();
        assert_eq!(got.len(), 1023, "quadros duplicados");
        assert_eq!(a.stats().free, 0);
        assert_eq!(a.stats().failures, 1);

        a.free(p(0x5000)).unwrap();
        assert_eq!(a.allocate(), Some(p(0x5000)));
        assert_eq!(a.free(p(0x5000)), Ok(()));
        assert_eq!(a.free(p(0x5000)), Err(FrameError::DoubleFree(p(0x5000))));
        assert_eq!(a.free(p(0x5001)), Err(FrameError::Unaligned(p(0x5001))));
        assert_eq!(
            a.free(p(0x10_0000_0000)),
            Err(FrameError::OutOfRange(p(0x10_0000_0000)))
        );
        assert_eq!(a.free(p(0)), Err(FrameError::NotUsable(p(0)))); // reservado
    }

    #[test]
    fn mark_used_reserves_ranges() {
        let mut storage = vec![0u64; 4];
        let mut a = BitmapFrameAllocator::new(128, &mut storage);
        a.mark_usable(p(0), p(128 * 0x1000)).unwrap();
        a.mark_used(p(0), p(0x10_000)).unwrap(); // 16 quadros
        assert_eq!(a.stats().free, 112);
        assert!(!a.is_free(p(0xf000)));
        assert!(a.is_free(p(0x10_000)));
        assert_eq!(a.allocate(), Some(p(0x10_000)));
        assert_eq!(a.stats().used(), 17);
    }

    #[test]
    fn capacity_not_multiple_of_64() {
        let mut storage = vec![0u64; 4];
        let mut a = BitmapFrameAllocator::new(70, &mut storage);
        a.mark_usable(p(0), p(200 * 0x1000)).unwrap(); // além da capacidade é ignorado
        assert_eq!(a.stats().total_usable, 70);
        let mut n = 0;
        while a.allocate().is_some() {
            n += 1;
        }
        assert_eq!(n, 70);
    }

    #[test]
    fn trait_object_usage() {
        let mut storage = vec![0u64; 2];
        let mut a = BitmapFrameAllocator::new(64, &mut storage);
        a.mark_usable(p(0), p(64 * 0x1000)).unwrap();
        let alloc: &mut dyn FrameAllocator = &mut a;
        let f = alloc.allocate_frame().unwrap();
        alloc.deallocate_frame(f);
        assert_eq!(a.stats().frees, 1);
    }
}

//! Heap do kernel: lista de blocos livres ordenada por endereço, first-fit,
//! coalescência na liberação e cabeçalho por alocação.
//!
//! Cada alocação recebe um [`AllocHeader`] imediatamente antes do ponteiro
//! devolvido, registrando o bloco real consumido. Isso permite alinhamentos
//! arbitrários sem perder memória e detecta *double free* com um marcador.
//!
//! O crate não conhece páginas nem locks: o kernel envolve [`Heap`] em um
//! spinlock e chama [`Heap::extend`] quando mapeia mais memória.
#![no_std]

use core::alloc::Layout;
use core::ptr::NonNull;

/// Alinhamento mínimo e granularidade de todos os blocos.
pub const MIN_ALIGN: usize = 16;
const HEADER_SIZE: usize = core::mem::size_of::<AllocHeader>();
const NODE_SIZE: usize = core::mem::size_of::<FreeNode>();
const MAGIC_LIVE: usize = 0xA110_C8ED_0000_0001;
const MAGIC_FREED: usize = 0xDEAD_F4EE_0000_0002;

/// Nó de bloco livre (reside no início do próprio bloco).
#[repr(C)]
struct FreeNode {
    size: usize,
    next: Option<NonNull<FreeNode>>,
}

/// Cabeçalho gravado antes de cada alocação.
#[repr(C)]
struct AllocHeader {
    block_start: usize,
    block_size: usize,
    magic: usize,
    _pad: usize,
}

/// Estatísticas do heap.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HeapStats {
    /// Bytes totais administrados (soma das regiões).
    pub total_bytes: usize,
    /// Bytes ocupados por blocos alocados (inclui cabeçalhos e preenchimento).
    pub used_bytes: usize,
    /// Pico de `used_bytes`.
    pub peak_bytes: usize,
    /// Alocações bem-sucedidas.
    pub allocations: u64,
    /// Liberações.
    pub frees: u64,
    /// Falhas por falta de espaço.
    pub failures: u64,
    /// Número de blocos livres (fragmentação).
    pub free_blocks: usize,
}

/// Heap de lista encadeada.
pub struct Heap {
    head: Option<NonNull<FreeNode>>,
    stats: HeapStats,
}

// SAFETY: o Heap só é usado atrás de um lock; os ponteiros internos apontam
// para memória de sua propriedade exclusiva.
unsafe impl Send for Heap {}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

impl Heap {
    /// Heap vazio (sem memória).
    pub const fn new() -> Self {
        Heap {
            head: None,
            stats: HeapStats {
                total_bytes: 0,
                used_bytes: 0,
                peak_bytes: 0,
                allocations: 0,
                frees: 0,
                failures: 0,
                free_blocks: 0,
            },
        }
    }

    /// Adiciona a região `[start, start+size)` ao heap.
    ///
    /// # Safety
    /// A região deve ser válida, exclusiva do heap e permanecer mapeada.
    pub unsafe fn extend(&mut self, start: usize, size: usize) {
        let s = align_up(start, MIN_ALIGN);
        let e = (start + size) & !(MIN_ALIGN - 1);
        if e <= s || e - s < NODE_SIZE {
            return;
        }
        self.stats.total_bytes += e - s;
        // SAFETY: região exclusiva; inserimos como bloco livre.
        unsafe { self.insert_free(s, e - s) };
    }

    /// Estatísticas.
    pub fn stats(&self) -> HeapStats {
        self.stats
    }

    /// Aloca conforme `layout`.
    pub fn allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let align = layout.align().max(MIN_ALIGN);
        let size = align_up(layout.size().max(1), MIN_ALIGN);

        let mut prev: Option<NonNull<FreeNode>> = None;
        let mut cur = self.head;
        while let Some(node) = cur {
            // SAFETY: nós da lista são blocos livres válidos de nossa propriedade.
            let (block_start, block_size, next) = unsafe {
                let n = node.as_ref();
                (node.as_ptr() as usize, n.size, n.next)
            };
            let block_end = block_start + block_size;

            let ptr = align_up(block_start + HEADER_SIZE, align);
            let mut front_gap = ptr - HEADER_SIZE - block_start;
            let mut ptr = ptr;
            if front_gap != 0 && front_gap < NODE_SIZE {
                // Folga pequena demais para virar bloco livre: empurra a alocação.
                ptr = align_up(block_start + NODE_SIZE + HEADER_SIZE, align);
                front_gap = ptr - HEADER_SIZE - block_start;
            }
            let end = ptr + size;
            if end <= block_end {
                let mut tail = block_end - end;
                let mut alloc_end = end;
                if tail < NODE_SIZE {
                    alloc_end = block_end;
                    tail = 0;
                }
                let alloc_start = ptr - HEADER_SIZE;

                // Reescreve a lista: [front_gap] [alocado] [tail]
                let mut replacement: Option<NonNull<FreeNode>> = next;
                if tail != 0 {
                    // SAFETY: `alloc_end..block_end` está dentro do bloco livre.
                    unsafe {
                        let t = alloc_end as *mut FreeNode;
                        t.write(FreeNode { size: tail, next });
                        replacement = Some(NonNull::new_unchecked(t));
                    }
                }
                if front_gap != 0 {
                    // SAFETY: o nó atual continua no mesmo endereço, só encolhe.
                    unsafe {
                        (node.as_ptr()).write(FreeNode {
                            size: front_gap,
                            next: replacement,
                        });
                    }
                    replacement = Some(node);
                }
                match prev {
                    // SAFETY: `p` é um nó válido da lista.
                    Some(p) => unsafe { (*p.as_ptr()).next = replacement },
                    None => self.head = replacement,
                }
                self.stats.free_blocks =
                    self.stats.free_blocks + usize::from(tail != 0) + usize::from(front_gap != 0)
                        - 1;

                // SAFETY: cabeçalho fica dentro do bloco alocado.
                unsafe {
                    (alloc_start as *mut AllocHeader).write(AllocHeader {
                        block_start: alloc_start,
                        block_size: alloc_end - alloc_start,
                        magic: MAGIC_LIVE,
                        _pad: 0,
                    });
                }
                self.stats.used_bytes += alloc_end - alloc_start;
                self.stats.peak_bytes = self.stats.peak_bytes.max(self.stats.used_bytes);
                self.stats.allocations += 1;
                // SAFETY: ptr != 0 (dentro de um bloco válido).
                return Some(unsafe { NonNull::new_unchecked(ptr as *mut u8) });
            }
            prev = cur;
            cur = next;
        }
        self.stats.failures += 1;
        None
    }

    /// Libera `ptr` obtido de [`Heap::allocate`].
    ///
    /// # Safety
    /// `ptr` deve ter sido devolvido por este heap e não ter sido liberado.
    pub unsafe fn deallocate(&mut self, ptr: NonNull<u8>, _layout: Layout) {
        let hdr = (ptr.as_ptr() as usize - HEADER_SIZE) as *mut AllocHeader;
        // SAFETY: por contrato, o cabeçalho existe antes de `ptr`.
        let (start, size) = unsafe {
            let h = &mut *hdr;
            assert!(
                h.magic == MAGIC_LIVE,
                "heap: liberacao invalida em {:#x} (magic {:#x})",
                ptr.as_ptr() as usize,
                h.magic
            );
            h.magic = MAGIC_FREED;
            (h.block_start, h.block_size)
        };
        self.stats.used_bytes -= size;
        self.stats.frees += 1;
        // SAFETY: bloco era nosso e está fora de uso.
        unsafe { self.insert_free(start, size) };
    }

    /// Insere um bloco livre mantendo a ordem por endereço e coalescendo vizinhos.
    ///
    /// # Safety
    /// `start..start+size` deve ser memória do heap fora de uso (sem aliases vivos).
    unsafe fn insert_free(&mut self, start: usize, size: usize) {
        let mut prev: Option<NonNull<FreeNode>> = None;
        let mut cur = self.head;
        while let Some(node) = cur {
            if node.as_ptr() as usize > start {
                break;
            }
            prev = cur;
            // SAFETY: nó válido.
            cur = unsafe { node.as_ref().next };
        }
        let mut size = size;
        let mut next = cur;
        // Coalesce com o próximo.
        if let Some(n) = cur {
            // SAFETY: nó válido.
            let (naddr, nsize, nnext) =
                unsafe { (n.as_ptr() as usize, n.as_ref().size, n.as_ref().next) };
            if start + size == naddr {
                size += nsize;
                next = nnext;
                self.stats.free_blocks -= 1;
            }
        }
        // Coalesce com o anterior.
        if let Some(p) = prev {
            // SAFETY: nó válido.
            let (paddr, psize) = unsafe { (p.as_ptr() as usize, p.as_ref().size) };
            if paddr + psize == start {
                size += psize;
                // SAFETY: reescreve o nó anterior no lugar.
                unsafe { p.as_ptr().write(FreeNode { size, next }) };
                return;
            }
        }
        // SAFETY: `start` é memória livre de nossa propriedade.
        let node = unsafe {
            let n = start as *mut FreeNode;
            n.write(FreeNode { size, next });
            NonNull::new_unchecked(n)
        };
        match prev {
            // SAFETY: nó válido.
            Some(p) => unsafe { (*p.as_ptr()).next = Some(node) },
            None => self.head = Some(node),
        }
        self.stats.free_blocks += 1;
    }

    /// Maior bloco livre contíguo, em bytes.
    pub fn largest_free_block(&self) -> usize {
        let mut best = 0;
        let mut cur = self.head;
        while let Some(n) = cur {
            // SAFETY: nó válido.
            let (s, next) = unsafe { (n.as_ref().size, n.as_ref().next) };
            best = best.max(s);
            cur = next;
        }
        best
    }
}

const fn align_up(v: usize, a: usize) -> usize {
    (v + a - 1) & !(a - 1)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    struct Arena {
        buf: Vec<u8>,
        heap: Heap,
    }

    impl Arena {
        fn new(size: usize) -> Self {
            let buf = vec![0u8; size + 4096];
            let mut heap = Heap::new();
            let start = align_up(buf.as_ptr() as usize, 4096);
            // SAFETY: buffer vivo enquanto Arena existir.
            unsafe { heap.extend(start, size) };
            Arena { buf, heap }
        }
        fn range(&self) -> (usize, usize) {
            let s = align_up(self.buf.as_ptr() as usize, 4096);
            (s, s + self.buf.len() - 4096)
        }
    }

    #[test]
    fn basic_alloc_free_and_reuse() {
        let mut a = Arena::new(64 * 1024);
        let l = Layout::from_size_align(100, 8).unwrap();
        let p1 = a.heap.allocate(l).unwrap();
        let p2 = a.heap.allocate(l).unwrap();
        assert_ne!(p1, p2);
        let (s, e) = a.range();
        assert!((s..e).contains(&(p1.as_ptr() as usize)));
        // SAFETY: ponteiros válidos deste heap.
        unsafe {
            core::ptr::write_bytes(p1.as_ptr(), 0xAB, 100);
            a.heap.deallocate(p1, l);
            a.heap.deallocate(p2, l);
        }
        assert_eq!(a.heap.stats().used_bytes, 0);
        assert_eq!(a.heap.stats().free_blocks, 1, "coalescencia total esperada");
        assert_eq!(a.heap.largest_free_block(), 64 * 1024);
    }

    #[test]
    fn respects_large_alignment() {
        let mut a = Arena::new(64 * 1024);
        for align in [16usize, 64, 256, 4096] {
            let l = Layout::from_size_align(24, align).unwrap();
            let p = a.heap.allocate(l).unwrap();
            assert_eq!(p.as_ptr() as usize % align, 0, "align {align}");
            // SAFETY: ponteiro válido.
            unsafe { a.heap.deallocate(p, l) };
        }
        assert_eq!(a.heap.stats().free_blocks, 1);
    }

    #[test]
    fn exhaustion_and_recovery() {
        let mut a = Arena::new(8 * 1024);
        let l = Layout::from_size_align(1000, 16).unwrap();
        let mut ptrs = Vec::new();
        while let Some(p) = a.heap.allocate(l) {
            ptrs.push(p);
        }
        assert!(ptrs.len() >= 6 && ptrs.len() <= 8, "{}", ptrs.len());
        assert_eq!(a.heap.stats().failures, 1);
        for p in ptrs {
            // SAFETY: ponteiros válidos.
            unsafe { a.heap.deallocate(p, l) };
        }
        assert_eq!(a.heap.largest_free_block(), 8 * 1024);
        let big = Layout::from_size_align(8 * 1024 - 64, 16).unwrap();
        assert!(a.heap.allocate(big).is_some());
    }

    #[test]
    fn extend_adds_capacity() {
        let mut a = Arena::new(4096);
        let big = Layout::from_size_align(6000, 16).unwrap();
        assert!(a.heap.allocate(big).is_none());
        let extra = vec![0u8; 16 * 1024];
        let s = align_up(extra.as_ptr() as usize, 16);
        // SAFETY: `extra` vive até o fim do teste.
        unsafe { a.heap.extend(s, 8 * 1024) };
        assert!(a.heap.allocate(big).is_some());
        assert_eq!(a.heap.stats().total_bytes, 4096 + 8 * 1024);
    }

    #[test]
    #[should_panic(expected = "liberacao invalida")]
    fn double_free_is_detected() {
        let mut a = Arena::new(4096);
        let l = Layout::from_size_align(32, 16).unwrap();
        let p = a.heap.allocate(l).unwrap();
        // SAFETY: primeira liberação válida; a segunda deve ser detectada.
        unsafe {
            a.heap.deallocate(p, l);
            a.heap.deallocate(p, l);
        }
    }

    #[test]
    fn randomized_no_overlap() {
        let mut a = Arena::new(256 * 1024);
        let mut live: Vec<(NonNull<u8>, Layout, u8)> = Vec::new();
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for i in 0..5000u32 {
            if live.is_empty() || rnd() % 3 != 0 {
                let size = (rnd() % 700 + 1) as usize;
                let align = 1usize << (rnd() % 8);
                let l = Layout::from_size_align(size, align).unwrap();
                if let Some(p) = a.heap.allocate(l) {
                    let tag = (i & 0xff) as u8;
                    // SAFETY: bloco recém-alocado.
                    unsafe { core::ptr::write_bytes(p.as_ptr(), tag, size) };
                    live.push((p, l, tag));
                }
            } else {
                let idx = (rnd() % live.len() as u64) as usize;
                let (p, l, tag) = live.swap_remove(idx);
                // Verifica que ninguém escreveu por cima.
                // SAFETY: bloco vivo.
                let slice = unsafe { core::slice::from_raw_parts(p.as_ptr(), l.size()) };
                assert!(slice.iter().all(|&b| b == tag), "corrupcao detectada");
                // SAFETY: bloco vivo.
                unsafe { a.heap.deallocate(p, l) };
            }
        }
        for (p, l, _) in live {
            // SAFETY: bloco vivo.
            unsafe { a.heap.deallocate(p, l) };
        }
        let st = a.heap.stats();
        assert_eq!(st.used_bytes, 0);
        assert_eq!(st.free_blocks, 1);
        assert_eq!(st.allocations, st.frees);
    }
}

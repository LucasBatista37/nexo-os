//! Entropia para o ASLR de pilha e de mapeamentos: `rdrand` quando a CPU tem (CPUID.1:ECX
//! bit 30); senão um xorshift64* semeado pelo TSC e remisturado a cada chamada — suficiente
//! para dispersar endereços entre processos, **não** para chaves criptográficas.

use core::sync::atomic::{AtomicU64, Ordering};
use nexo_arch_x86_64::cpu;

static STATE: AtomicU64 = AtomicU64::new(0);

fn rdrand() -> Option<u64> {
    if cpu::cpuid(1, 0).ecx & (1 << 30) == 0 {
        return None;
    }
    let mut v = 0u64;
    for _ in 0..10 {
        // SAFETY: a instrução existe (bit 30 de CPUID.1:ECX conferido acima); só escreve em `v`.
        if unsafe { core::arch::x86_64::_rdrand64_step(&mut v) } == 1 {
            return Some(v);
        }
    }
    None
}

/// Um valor pseudoaleatório de 64 bits.
pub fn random_u64() -> u64 {
    if let Some(v) = rdrand() {
        return v;
    }
    let mut s = STATE.load(Ordering::Relaxed);
    if s == 0 {
        s = cpu::rdtsc() | 1;
    }
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    s ^= cpu::rdtsc().rotate_left(32);
    if s == 0 {
        s = 0x9E37_79B9_7F4A_7C15;
    }
    STATE.store(s, Ordering::Relaxed);
    s.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// Deslocamento aleatório, em bytes e alinhado a página, dentro de uma janela de `pages`
/// páginas (0 se a janela é vazia).
pub fn page_offset(pages: u64) -> u64 {
    if pages == 0 {
        0
    } else {
        (random_u64() % pages) * nexo_mm::PAGE_SIZE
    }
}

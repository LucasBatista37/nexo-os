//! Normalização do mapa de memória.
//!
//! O firmware entrega regiões possivelmente desordenadas, sobrepostas e não
//! alinhadas. [`normalize`] produz uma lista ordenada, sem sobreposição, com
//! limites em página e regiões adjacentes do mesmo tipo fundidas. Regiões
//! utilizáveis encolhem para dentro das páginas; reservadas expandem para fora.

use crate::addr::{PAGE_SIZE, align_down, align_up};
use nexo_boot_abi::{MemoryKind, MemoryRegion};

/// Erros de normalização.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapError {
    /// Entrada maior que o limite `MAX`.
    TooManyInputRegions,
    /// Saída não coube no buffer fornecido.
    OutputTooSmall,
}

/// Resumo estatístico de um mapa normalizado.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MapSummary {
    /// Bytes utilizáveis após o boot.
    pub usable_bytes: u64,
    /// Bytes de RAM ocupados pelo kernel (imagem, tabelas, pilha, boot info, arquivo).
    pub kernel_bytes: u64,
    /// Bytes reservados (firmware, ACPI, MMIO, runtime).
    pub reserved_bytes: u64,
    /// Maior endereço físico (exclusivo) de qualquer região.
    pub max_addr: u64,
    /// Maior endereço físico (exclusivo) de memória utilizável.
    pub max_usable_addr: u64,
    /// Número de regiões.
    pub regions: usize,
}

/// Alinha uma região às páginas conforme sua natureza.
fn page_align(r: MemoryRegion) -> MemoryRegion {
    let (start, end) = if r.kind().is_usable_after_boot() {
        (align_up(r.start, PAGE_SIZE), align_down(r.end, PAGE_SIZE))
    } else {
        (align_down(r.start, PAGE_SIZE), align_up(r.end, PAGE_SIZE))
    };
    MemoryRegion { start, end, ..r }
}

/// Máximo de regiões de entrada aceitas por [`normalize`].
pub const MAX_INPUT_REGIONS: usize = 528;

/// Normaliza `input` em `output`. Devolve o número de regiões escritas.
///
/// Usa `2 * MAX_INPUT_REGIONS` pontos de corte em pilha (8 KiB), sem
/// alocação. Complexidade O(n²) — adequada para n ≤ 512.
pub fn normalize(input: &[MemoryRegion], output: &mut [MemoryRegion]) -> Result<usize, MapError> {
    if input.len() > MAX_INPUT_REGIONS {
        return Err(MapError::TooManyInputRegions);
    }

    // 1. Coleta pontos de corte das regiões alinhadas e não vazias.
    let mut cuts = [0u64; MAX_INPUT_REGIONS * 2];
    let mut ncuts = 0;
    let valid = |r: &MemoryRegion| {
        let a = page_align(*r);
        (!a.is_empty() && a.kind() != MemoryKind::Unknown).then_some(a)
    };
    for a in input.iter().filter_map(valid) {
        cuts[ncuts] = a.start;
        cuts[ncuts + 1] = a.end;
        ncuts += 2;
    }

    // 2. Ordena pontos de corte (insertion sort — sem alocação) e remove duplicatas.
    let cuts = &mut cuts[..ncuts];
    for i in 1..cuts.len() {
        let mut j = i;
        while j > 0 && cuts[j - 1] > cuts[j] {
            cuts.swap(j - 1, j);
            j -= 1;
        }
    }
    let mut uniq = 0;
    for i in 0..cuts.len() {
        if i == 0 || cuts[i] != cuts[uniq - 1] {
            cuts[uniq] = cuts[i];
            uniq += 1;
        }
    }
    let cuts = &cuts[..uniq];

    // 3. Para cada intervalo elementar, escolhe o tipo de maior prioridade.
    let mut out = 0;
    for w in cuts.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        let mut best: Option<MemoryKind> = None;
        for r in input.iter().filter_map(valid) {
            if r.start <= lo && r.end >= hi {
                let k = r.kind();
                if best.is_none_or(|b| k.priority() > b.priority()) {
                    best = Some(k);
                }
            }
        }
        let Some(kind) = best else { continue };
        // 4. Funde com a região anterior se contígua e do mesmo tipo.
        if out > 0 && output[out - 1].end == lo && output[out - 1].kind() == kind {
            output[out - 1].end = hi;
        } else {
            if out >= output.len() {
                return Err(MapError::OutputTooSmall);
            }
            output[out] = MemoryRegion::new(lo, hi, kind);
            out += 1;
        }
    }
    Ok(out)
}

/// Calcula o resumo de um mapa (normalizado ou não).
pub fn summarize(map: &[MemoryRegion]) -> MapSummary {
    let mut s = MapSummary {
        regions: map.len(),
        ..Default::default()
    };
    for r in map {
        let len = r.len();
        match r.kind() {
            MemoryKind::Usable | MemoryKind::LoaderReclaimable => {
                s.usable_bytes += len;
                s.max_usable_addr = s.max_usable_addr.max(r.end);
            }
            MemoryKind::KernelImage
            | MemoryKind::KernelPageTables
            | MemoryKind::KernelStack
            | MemoryKind::BootInfo
            | MemoryKind::KernelFile => {
                s.kernel_bytes += len;
                s.max_usable_addr = s.max_usable_addr.max(r.end);
            }
            _ => s.reserved_bytes += len,
        }
        s.max_addr = s.max_addr.max(r.end);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use MemoryKind::*;

    fn r(start: u64, end: u64, kind: MemoryKind) -> MemoryRegion {
        MemoryRegion::new(start, end, kind)
    }

    #[test]
    fn sorts_merges_and_aligns() {
        let input = [
            r(0x3000, 0x5000, Usable),
            r(0x0000, 0x1000, Reserved),
            r(0x1000, 0x3000, Usable),
            r(0x5000, 0x5800, Usable), // fim não alinhado: encolhe
        ];
        let mut out = [MemoryRegion::EMPTY; 8];
        let n = normalize(&input, &mut out).unwrap();
        assert_eq!(n, 2);
        assert_eq!(out[0], r(0x0000, 0x1000, Reserved));
        assert_eq!(out[1], r(0x1000, 0x5000, Usable));
    }

    #[test]
    fn reserved_wins_overlap_and_expands() {
        let input = [r(0x1000, 0x9000, Usable), r(0x4800, 0x5800, Mmio)];
        let mut out = [MemoryRegion::EMPTY; 8];
        let n = normalize(&input, &mut out).unwrap();
        assert_eq!(
            &out[..n],
            &[
                r(0x1000, 0x4000, Usable),
                r(0x4000, 0x6000, Mmio),
                r(0x6000, 0x9000, Usable)
            ]
        );
    }

    #[test]
    fn kernel_regions_carve_out_of_ram() {
        let input = [
            r(0x10_0000, 0x100_0000, Usable),
            r(0x20_0000, 0x30_0000, KernelImage),
            r(0x30_0000, 0x31_0000, KernelStack),
        ];
        let mut out = [MemoryRegion::EMPTY; 8];
        let n = normalize(&input, &mut out).unwrap();
        assert_eq!(n, 4);
        assert_eq!(out[1].kind(), KernelImage);
        assert_eq!(out[2].kind(), KernelStack);
        let s = summarize(&out[..n]);
        assert_eq!(s.kernel_bytes, 0x11_0000);
        assert_eq!(s.usable_bytes, 0x100_0000 - 0x10_0000 - 0x11_0000);
        assert_eq!(s.max_addr, 0x100_0000);
    }

    #[test]
    fn drops_empty_and_unknown() {
        let input = [
            r(0x1000, 0x1000, Usable),
            MemoryRegion {
                kind: 77,
                ..r(0, 0x1000, Usable)
            },
        ];
        let mut out = [MemoryRegion::EMPTY; 4];
        assert_eq!(normalize(&input, &mut out).unwrap(), 0);
    }

    #[test]
    fn errors_on_limits() {
        extern crate std;
        let input = std::vec![r(0, 0x1000, Usable); MAX_INPUT_REGIONS + 1];
        let mut out = [MemoryRegion::EMPTY; 8];
        assert_eq!(
            normalize(&input, &mut out),
            Err(MapError::TooManyInputRegions)
        );
        let input = [r(0, 0x1000, Usable), r(0x2000, 0x3000, Reserved)];
        let mut out = [MemoryRegion::EMPTY; 1];
        assert_eq!(normalize(&input, &mut out), Err(MapError::OutputTooSmall));
    }
}

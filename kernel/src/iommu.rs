//! Abstração de IOMMU (Fase 3: "criar abstração IOMMU e caminho sem IOMMU explicitamente
//! inseguro"). Hoje só existe o modo [`Mode::Passthrough`]: o kernel entrega páginas físicas
//! aos drivers ([`crate::x86::syscall`], `SYS_DMA_ALLOC`) e o dispositivo pode fazer DMA para
//! **qualquer** endereço físico que o driver programar — a concessão de dispositivo equivale,
//! portanto, a confiança total no driver (ADR-0015). Quando houver tradução (VT-d/AMD-Vi),
//! este módulo passa a criar domínios por concessão e a mapear apenas as páginas de DMA do
//! processo; a detecção da tabela ACPI `DMAR` já registra o que o hardware oferece.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Modo de proteção de DMA em vigor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Sem tradução: DMA irrestrito (INSEGURO por construção; documentado no ADR-0015).
    Passthrough {
        /// `true` se a plataforma anuncia VT-d (tabela `DMAR`) que ainda não usamos.
        dmar_present: bool,
    },
}

/// 0 = nao inicializado; 1 = passthrough sem DMAR; 2 = passthrough com DMAR presente.
static STATE: AtomicU8 = AtomicU8::new(0);
static WARNED: AtomicBool = AtomicBool::new(false);

/// Detecta o que a plataforma oferece e registra o modo em vigor.
pub fn init(dmar_present: bool) {
    STATE.store(if dmar_present { 2 } else { 1 }, Ordering::Release);
    let _ = WARNED.swap(true, Ordering::Relaxed);
    if dmar_present {
        kwarn!(
            "iommu: tabela DMAR presente mas sem driver VT-d — DMA de drivers IRRESTRITO (caminho inseguro; ADR-0015)"
        );
    } else {
        kwarn!(
            "iommu: ausente — DMA de drivers IRRESTRITO (caminho inseguro por construção; ADR-0015)"
        );
    }
}

/// Modo em vigor.
pub fn mode() -> Mode {
    match STATE.load(Ordering::Acquire) {
        1 => Mode::Passthrough {
            dmar_present: false,
        },
        2 => Mode::Passthrough { dmar_present: true },
        _ => panic!("iommu::init nao foi chamado"),
    }
}

/// `true` enquanto o DMA dos drivers não é contido por tradução.
pub fn dma_is_unrestricted() -> bool {
    matches!(mode(), Mode::Passthrough { .. })
}

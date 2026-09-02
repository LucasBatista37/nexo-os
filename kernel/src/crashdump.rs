//! Crash dump protegido (Plano §Fase 8): no pânico, o kernel grava um dump de texto
//! (mensagem, local, backtrace simbolizado) numa sub-área reservada do disco de dados —
//! setores `cap-16 .. cap-8`, disjuntos dos usados pelos testes crus (`cap-256 ..`). O caminho
//! é de EMERGÊNCIA por desenho: nenhum driver de usuário é confiável durante um pânico, então
//! um mini virtio-blk síncrono (reset → fila própria → uma escrita → poll) roda no kernel,
//! usando páginas de DMA pré-alocadas no boot (o pânico não pode alocar nem travar em locks).
//! A leitura fica no host: `tools/nexo-disk crashdump` (extração/limpeza; consentimento de
//! envio é decisão futura — o dump nunca sai da máquina sozinho).

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use nexo_virtio::{
    DESC_NEXT, DESC_WRITE, DmaPage, MapBar, NO_VECTOR, PciConfig, SplitQueue, Transport, parse_caps,
};

use crate::symbols::Symbolized;

/// Assinatura no início do dump.
pub const MAGIC: &[u8; 8] = b"NEXODUMP";
/// Setores gravados (8 × 512 = 4 KiB).
pub const DUMP_SECTORS: u64 = 8;
/// Distância do FIM do disco até o início do dump (fica em `cap-16 .. cap-8`).
pub const DUMP_OFFSET_FROM_END: u64 = 16;

const TEXT_MAX: usize = 4096 - 16; // cabeçalho: magic 8 + versão u32 + len u32

static READY: AtomicBool = AtomicBool::new(false);
static ONCE: AtomicBool = AtomicBool::new(false);
// Páginas de DMA pré-alocadas no boot (físicos); o pânico só as usa.
static PG: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5]; // desc, avail, used, hdr, data
// Transporte do disco-alvo, com BARs já mapeados no boot (os BARs virtio ficam ACIMA de
// 4 GiB no q35 — fora do physmap; mapear exige alocador de tabelas, proibido no pânico).
static TARGET: crate::cell::StaticCell<Option<Transport>> = crate::cell::StaticCell::new(None);
/// Janela de VA do kernel para os BARs do caminho de emergência.
const MMIO_WIN: u64 = 0xffff_9000_0000_0000;

/// Mapeia `size` bytes do físico `base` na janela de emergência; devolve o VA.
fn map_window(next: &mut u64, base: u64, size: u64) -> Option<u64> {
    use nexo_arch_x86_64::paging::PageFlags;
    use nexo_mm::{PhysAddr, VirtAddr};
    let pages = size.div_ceil(4096).min(64);
    let va = *next;
    for i in 0..pages {
        crate::mm::virt::map_page(
            VirtAddr::new(va + i * 4096),
            PhysAddr::new(base + i * 4096),
            PageFlags::KERNEL_RW | PageFlags::NO_CACHE | PageFlags::WRITE_THROUGH,
        )
        .ok()?;
    }
    *next += pages * 4096 + 4096; // página de guarda entre BARs
    Some(va)
}

/// Pré-aloca páginas de DMA e resolve o disco-alvo (virtio-blk gravável), mapeando os BARs.
/// Chamar uma vez no boot, single-core, com o alocador vivo.
pub fn init() {
    for slot in &PG {
        match crate::mm::phys::allocate_zeroed_frame() {
            Some(f) => slot.store(f.as_u64(), Ordering::Release),
            None => return, // sem páginas: dumps ficam desabilitados
        }
    }
    let mut next = MMIO_WIN;
    for d in crate::pci::devices() {
        if !d.is_virtio() || d.device != 0x1042 {
            continue;
        }
        let bdf = crate::pci::Bdf::from_packed(d.bdf);
        let mut cfg = Cfg(bdf);
        let cmd = cfg.read32(4);
        cfg.write32(4, cmd | 0x6);
        let caps = parse_caps(&mut cfg);
        // mapeia de antemão cada BAR referenciado pelas capabilities
        struct Pre {
            va: [Option<u64>; 6],
        }
        impl MapBar for Pre {
            fn map(&mut self, bar: u8) -> Result<u64, nexo_virtio::Error> {
                self.va[bar as usize].ok_or(nexo_virtio::Error::Map)
            }
        }
        let mut pre = Pre { va: [None; 6] };
        let mut precisa = [false; 6];
        if let Some((b, _)) = caps.common {
            precisa[b as usize] = true;
        }
        if let Some((b, _, _)) = caps.notify {
            precisa[b as usize] = true;
        }
        if let Some((b, _)) = caps.isr {
            precisa[b as usize] = true;
        }
        if let Some((b, _)) = caps.device {
            precisa[b as usize] = true;
        }
        for (i, need) in precisa.iter().enumerate() {
            if *need {
                let bar = d.bars[i];
                if bar.size == 0 || bar.flags & 1 != 0 {
                    continue;
                }
                pre.va[i] = map_window(&mut next, bar.base, bar.size);
            }
        }
        let Ok(t) = Transport::new(&caps, &mut pre) else {
            continue;
        };
        t.reset();
        if t.negotiate(0, 0).is_err() {
            continue;
        }
        // VIRTIO_BLK_F_RO = bit 5: o disco de boot é somente-leitura — pula
        let ro = t.device_features().0 & (1 << 5) != 0;
        t.reset(); // deixa o dispositivo limpo para o driver de usuário
        if ro {
            continue;
        }
        // SAFETY: init roda uma vez, single-core, antes de qualquer pânico possível usar TARGET.
        unsafe { *TARGET.as_ptr() = Some(t) };
        READY.store(true, Ordering::Release);
        kinfo!(
            "crashdump: pronto (virtio-blk gravavel em {:02x}:{:02x}.{})",
            bdf.bus,
            bdf.device,
            bdf.function
        );
        return;
    }
}

struct Cfg(crate::pci::Bdf);
impl PciConfig for Cfg {
    fn read32(&mut self, off: u16) -> u32 {
        crate::pci::cfg_read(self.0, off as u8)
    }
    fn write32(&mut self, off: u16, v: u32) {
        crate::pci::cfg_write(self.0, off as u8, v);
    }
}

/// Buffer de texto do dump (estático; sem alocação no pânico).
struct Buf<'a> {
    data: &'a mut [u8],
    len: usize,
}
impl Write for Buf<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let n = s.len().min(self.data.len() - self.len);
        self.data[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// Grava o dump do pânico corrente. Chamar SÓ do panic handler (uma vez; os demais retornos
/// são silenciosos — o dump é melhor-esforço e nunca pode piorar um pânico).
pub fn save(info: &core::panic::PanicInfo, rbp: u64) {
    if !READY.load(Ordering::Acquire) || ONCE.swap(true, Ordering::SeqCst) {
        return;
    }
    let (qd, qa, qu, hdr, data) = (
        PG[0].load(Ordering::Acquire),
        PG[1].load(Ordering::Acquire),
        PG[2].load(Ordering::Acquire),
        PG[3].load(Ordering::Acquire),
        PG[4].load(Ordering::Acquire),
    );
    let data_virt = crate::mm::virt::phys_to_virt(nexo_mm::PhysAddr::new(data)).as_u64();
    // monta o texto direto na página de dados, após o cabeçalho
    // SAFETY: página pré-alocada exclusiva deste caminho; único uso (guarda ONCE).
    let page = unsafe { core::slice::from_raw_parts_mut(data_virt as *mut u8, 4096) };
    page[..8].copy_from_slice(MAGIC);
    page[8..12].copy_from_slice(&1u32.to_le_bytes());
    let text_len = {
        let mut w = Buf {
            data: &mut page[16..],
            len: 0,
        };
        let _ = writeln!(w, "KERNEL PANIC (crash dump v1)");
        let _ = writeln!(w, "mensagem : {}", info.message());
        if let Some(l) = info.location() {
            let _ = writeln!(w, "local    : {}:{}:{}", l.file(), l.line(), l.column());
        }
        let _ = writeln!(w, "uptime   : {} ms", crate::time::uptime_ms());
        let _ = writeln!(w, "thread   : {}", crate::sched::current_name());
        let _ = writeln!(w, "backtrace:");
        let mut frames = 0u32;
        let mut rbp = rbp;
        // `frames` conta quadros IMPRESSOS (há `break`s) — o lint de contador não se aplica.
        #[allow(clippy::explicit_counter_loop)]
        for _ in 0..64 {
            let Some((lo, hi)) = super::panic::stack_bounds_pub(rbp) else {
                break;
            };
            if !rbp.is_multiple_of(8) || rbp < lo || rbp + 16 > hi {
                break;
            }
            // SAFETY: `rbp` validado dentro de uma pilha mapeada, alinhado a 8.
            let (saved, ret) = unsafe { (*(rbp as *const u64), *((rbp + 8) as *const u64)) };
            if ret == 0 {
                break;
            }
            let _ = writeln!(w, "  #{frames:<2} {}", Symbolized::return_address(ret));
            frames += 1;
            if saved <= rbp {
                break;
            }
            rbp = saved;
        }
        w.len.min(TEXT_MAX) as u32
    };
    page[12..16].copy_from_slice(&text_len.to_le_bytes());

    // escritor de emergência: o alvo foi resolvido e mapeado no boot; aqui só reprograma
    // SAFETY: TARGET publicado no init (single-core) e lido uma vez (guarda ONCE).
    let Some(t) = (unsafe { (*TARGET.as_ptr()).as_ref() }) else {
        return;
    };
    {
        t.reset();
        if t.negotiate(0, 0).is_err() {
            return;
        }
        let mk = |p: u64| DmaPage {
            virt: crate::mm::virt::phys_to_virt(nexo_mm::PhysAddr::new(p)).as_u64(),
            phys: p,
        };
        let Ok((size, notify)) = t.setup_queue(0, 8, qd, qa, qu, NO_VECTOR) else {
            return;
        };
        let mut q = SplitQueue::new(0, size, notify, mk(qd), mk(qa), mk(qu));
        t.driver_ok();
        let capacity = match &t.device {
            Some(m) => m.r32(0) as u64 | ((m.r32(4) as u64) << 32),
            None => return,
        };
        if capacity < 256 {
            return;
        }
        let sector = capacity - DUMP_OFFSET_FROM_END;
        // cabeçalho do pedido virtio-blk: type OUT (1) + sector; status no byte 16
        let hdr_virt = crate::mm::virt::phys_to_virt(nexo_mm::PhysAddr::new(hdr)).as_u64();
        // SAFETY: página pré-alocada exclusiva; layout do pedido virtio-blk.
        unsafe {
            core::ptr::write_bytes(hdr_virt as *mut u8, 0, 32);
            (hdr_virt as *mut u32).write_volatile(1); // OUT
            ((hdr_virt + 8) as *mut u64).write_volatile(sector);
            ((hdr_virt + 16) as *mut u8).write_volatile(0xff); // status
        }
        q.set_desc(0, hdr, 16, DESC_NEXT, 1);
        q.set_desc(1, data, (DUMP_SECTORS * 512) as u32, DESC_NEXT, 2);
        q.set_desc(2, hdr + 16, 1, DESC_WRITE, 0);
        q.submit(t, 0);
        let mut spins = 0u64;
        while q.pop_used().is_none() {
            spins += 1;
            if spins > 200_000_000 {
                return; // dispositivo mudo: desiste sem piorar o pânico
            }
            core::hint::spin_loop();
        }
        // SAFETY: mesma página; status escrito pelo dispositivo.
        let st = unsafe { ((hdr_virt + 16) as *const u8).read_volatile() };
        if st == 0 {
            kprint!(
                "crashdump: {} bytes gravados no setor {} do disco de dados\n",
                16 + text_len,
                sector
            );
        }
    }
}

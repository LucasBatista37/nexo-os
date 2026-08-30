//! Transporte VirtIO 1.x sobre PCI (interface moderna): localização das *capabilities*,
//! registradores da configuração comum, negociação de *features*, MSI-X e fila dividida
//! (*split virtqueue*). Usado por `services/blockdev` e `services/rngdev`.
//!
//! A biblioteca não sabe mapear BARs nem alocar DMA: o driver fornece [`PciConfig`] e
//! [`MapBar`] (syscalls de dispositivo) e páginas de DMA já mapeadas ([`DmaPage`]).
#![no_std]

use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{Ordering, fence};

/// Vendor ID VirtIO.
pub const VENDOR: u16 = 0x1af4;
/// Tipo de dispositivo a partir do Device ID (moderno `0x1040 + tipo`; transicional `0x1000 + tipo - 1`).
pub fn device_type(device_id: u16) -> Option<u16> {
    match device_id {
        0x1040..=0x107f => Some(device_id - 0x1040),
        0x1000 => Some(1),
        0x1001 => Some(2),
        0x1002 => Some(5),
        0x1003 => Some(3),
        0x1004 => Some(8),
        0x1005 => Some(4),
        0x1009 => Some(9),
        _ => None,
    }
}
/// Tipo: dispositivo de bloco.
pub const TYPE_BLOCK: u16 = 2;
/// Tipo: fonte de entropia.
pub const TYPE_RNG: u16 = 4;

/// Acesso ao espaço de configuração PCI da função.
pub trait PciConfig {
    /// Lê 32 bits alinhados.
    fn read32(&mut self, offset: u16) -> u32;
    /// Escreve 32 bits alinhados.
    fn write32(&mut self, offset: u16, value: u32);
    /// Lê um byte.
    fn read8(&mut self, offset: u16) -> u8 {
        (self.read32(offset & !3) >> ((offset & 3) * 8)) as u8
    }
    /// Lê 16 bits alinhados.
    fn read16(&mut self, offset: u16) -> u16 {
        (self.read32(offset & !3) >> ((offset & 2) * 8)) as u16
    }
}

/// Mapeia um BAR inteiro; devolve o endereço virtual base.
pub trait MapBar {
    /// Mapeia o BAR `bar` (pode ser chamado mais de uma vez para o mesmo BAR).
    fn map(&mut self, bar: u8) -> Result<u64, Error>;
}

/// Erros do transporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Capability de configuração comum ausente.
    NoCommonCfg,
    /// Capability de notificação ausente.
    NoNotifyCfg,
    /// Dispositivo não oferece `VIRTIO_F_VERSION_1`.
    NoVersion1,
    /// Dispositivo rejeitou as features escolhidas.
    FeaturesRejected,
    /// Fila inexistente.
    NoQueue,
    /// Falha ao mapear um BAR.
    Map,
    /// MSI-X ausente.
    NoMsix,
}

/// Capabilities VirtIO-PCI localizadas (BAR, offset).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Caps {
    /// Configuração comum.
    pub common: Option<(u8, u32)>,
    /// Notificação: (BAR, offset, multiplicador).
    pub notify: Option<(u8, u32, u32)>,
    /// Registrador ISR.
    pub isr: Option<(u8, u32)>,
    /// Configuração específica do dispositivo.
    pub device: Option<(u8, u32)>,
    /// Offset da capability MSI-X no espaço de configuração.
    pub msix: Option<u16>,
}

const CAP_VENDOR: u8 = 0x09;
const CAP_MSIX: u8 = 0x11;
const VIRTIO_PCI_CAP_COMMON: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY: u8 = 2;
const VIRTIO_PCI_CAP_ISR: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE: u8 = 4;

/// Percorre a lista de capabilities (limite de 48 elos).
pub fn parse_caps(cfg: &mut impl PciConfig) -> Caps {
    let mut caps = Caps::default();
    let status = cfg.read16(0x06);
    if status & 0x10 == 0 {
        return caps; // sem lista de capabilities
    }
    let mut off = cfg.read8(0x34) as u16 & !3;
    let mut guard = 0;
    while off >= 0x40 && guard < 48 {
        guard += 1;
        let id = cfg.read8(off);
        let next = cfg.read8(off + 1) as u16 & !3;
        if id == CAP_MSIX {
            caps.msix = Some(off);
        } else if id == CAP_VENDOR {
            let cfg_type = cfg.read8(off + 3);
            let bar = cfg.read8(off + 4);
            let offset = cfg.read32(off + 8);
            if bar < 6 {
                match cfg_type {
                    VIRTIO_PCI_CAP_COMMON => caps.common = Some((bar, offset)),
                    VIRTIO_PCI_CAP_NOTIFY => {
                        caps.notify = Some((bar, offset, cfg.read32(off + 16)))
                    }
                    VIRTIO_PCI_CAP_ISR => caps.isr = Some((bar, offset)),
                    VIRTIO_PCI_CAP_DEVICE => caps.device = Some((bar, offset)),
                    _ => {}
                }
            }
        }
        off = next;
    }
    caps
}

/// Janela MMIO mapeada (endereço virtual base).
#[derive(Clone, Copy)]
pub struct Mmio(pub u64);

impl Mmio {
    /// Lê 8 bits.
    pub fn r8(&self, off: u64) -> u8 {
        // SAFETY: `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia
        // BARs enumerados); acesso volátil e alinhado.
        unsafe { read_volatile((self.0 + off) as *const u8) }
    }
    /// Lê 16 bits.
    pub fn r16(&self, off: u64) -> u16 {
        // SAFETY: `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia
        // BARs enumerados); acesso volátil e alinhado.
        unsafe { read_volatile((self.0 + off) as *const u16) }
    }
    /// Lê 32 bits.
    pub fn r32(&self, off: u64) -> u32 {
        // SAFETY: `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia
        // BARs enumerados); acesso volátil e alinhado.
        unsafe { read_volatile((self.0 + off) as *const u32) }
    }
    /// Escreve 8 bits.
    pub fn w8(&self, off: u64, v: u8) {
        // SAFETY: `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia
        // BARs enumerados); acesso volátil e alinhado.
        unsafe { write_volatile((self.0 + off) as *mut u8, v) }
    }
    /// Escreve 16 bits.
    pub fn w16(&self, off: u64, v: u16) {
        // SAFETY: `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia
        // BARs enumerados); acesso volátil e alinhado.
        unsafe { write_volatile((self.0 + off) as *mut u16, v) }
    }
    /// Escreve 32 bits.
    pub fn w32(&self, off: u64, v: u32) {
        // SAFETY: `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia
        // BARs enumerados); acesso volátil e alinhado.
        unsafe { write_volatile((self.0 + off) as *mut u32, v) }
    }
    /// Escreve 64 bits como duas metades de 32.
    pub fn w64(&self, off: u64, v: u64) {
        self.w32(off, v as u32);
        self.w32(off + 4, (v >> 32) as u32);
    }
}

// Configuração comum (VirtIO 1.2 §4.1.4.3).
const C_DEV_FEAT_SEL: u64 = 0;
const C_DEV_FEAT: u64 = 4;
const C_DRV_FEAT_SEL: u64 = 8;
const C_DRV_FEAT: u64 = 12;
const C_MSIX_CFG: u64 = 16;
const C_NUM_QUEUES: u64 = 18;
const C_STATUS: u64 = 20;
const C_Q_SEL: u64 = 22;
const C_Q_SIZE: u64 = 24;
const C_Q_MSIX: u64 = 26;
const C_Q_ENABLE: u64 = 28;
const C_Q_NOTIFY_OFF: u64 = 30;
const C_Q_DESC: u64 = 32;
const C_Q_AVAIL: u64 = 40;
const C_Q_USED: u64 = 48;

/// Bits de status.
pub const S_ACK: u8 = 1;
/// Driver presente.
pub const S_DRIVER: u8 = 2;
/// Driver pronto.
pub const S_DRIVER_OK: u8 = 4;
/// Features aceitas.
pub const S_FEATURES_OK: u8 = 8;
/// Falha.
pub const S_FAILED: u8 = 128;
/// `VIRTIO_F_VERSION_1` (bit 32 → palavra alta, bit 0).
pub const F_VERSION_1_HI: u32 = 1;
/// Sem vetor MSI-X.
pub const NO_VECTOR: u16 = 0xffff;

/// Transporte PCI moderno de um dispositivo.
pub struct Transport {
    /// Configuração comum.
    pub common: Mmio,
    notify_base: u64,
    notify_mult: u32,
    /// Registrador ISR (se houver).
    pub isr: Option<Mmio>,
    /// Configuração do dispositivo (se houver).
    pub device: Option<Mmio>,
}

impl Transport {
    /// Mapeia as janelas necessárias.
    pub fn new(caps: &Caps, map: &mut impl MapBar) -> Result<Self, Error> {
        let (cb, co) = caps.common.ok_or(Error::NoCommonCfg)?;
        let (nb, no, mult) = caps.notify.ok_or(Error::NoNotifyCfg)?;
        let common = Mmio(map.map(cb)? + co as u64);
        let notify_base = map.map(nb)? + no as u64;
        let isr = match caps.isr {
            Some((b, o)) => Some(Mmio(map.map(b)? + o as u64)),
            None => None,
        };
        let device = match caps.device {
            Some((b, o)) => Some(Mmio(map.map(b)? + o as u64)),
            None => None,
        };
        Ok(Transport {
            common,
            notify_base,
            notify_mult: mult,
            isr,
            device,
        })
    }

    /// Reset e ACK/DRIVER.
    pub fn reset(&self) {
        self.common.w8(C_STATUS, 0);
        let mut spins = 0u32;
        while self.common.r8(C_STATUS) != 0 && spins < 1_000_000 {
            spins += 1;
        }
        self.common.w8(C_STATUS, S_ACK);
        self.common.w8(C_STATUS, S_ACK | S_DRIVER);
    }

    /// Lê as features oferecidas (palavras baixa e alta).
    pub fn device_features(&self) -> (u32, u32) {
        self.common.w32(C_DEV_FEAT_SEL, 0);
        let lo = self.common.r32(C_DEV_FEAT);
        self.common.w32(C_DEV_FEAT_SEL, 1);
        let hi = self.common.r32(C_DEV_FEAT);
        (lo, hi)
    }

    /// Negocia `wanted ∩ oferecidas` (exige `VERSION_1`); devolve as aceitas.
    pub fn negotiate(&self, wanted_lo: u32, wanted_hi: u32) -> Result<(u32, u32), Error> {
        let (lo, hi) = self.device_features();
        if hi & F_VERSION_1_HI == 0 {
            self.common.w8(C_STATUS, S_FAILED);
            return Err(Error::NoVersion1);
        }
        let (alo, ahi) = (lo & wanted_lo, (hi & wanted_hi) | F_VERSION_1_HI);
        self.common.w32(C_DRV_FEAT_SEL, 0);
        self.common.w32(C_DRV_FEAT, alo);
        self.common.w32(C_DRV_FEAT_SEL, 1);
        self.common.w32(C_DRV_FEAT, ahi);
        self.common.w8(C_STATUS, S_ACK | S_DRIVER | S_FEATURES_OK);
        if self.common.r8(C_STATUS) & S_FEATURES_OK == 0 {
            self.common.w8(C_STATUS, S_FAILED);
            return Err(Error::FeaturesRejected);
        }
        Ok((alo, ahi))
    }

    /// Programa a entrada 0 da tabela MSI-X com (`addr`, `data`), habilita MSI-X e desliga o
    /// vetor de configuração.
    pub fn setup_msix(
        &self,
        cfg: &mut impl PciConfig,
        caps: &Caps,
        map: &mut impl MapBar,
        addr: u64,
        data: u32,
    ) -> Result<(), Error> {
        let cap = caps.msix.ok_or(Error::NoMsix)?;
        let ctrl = cfg.read32(cap);
        let table = cfg.read32(cap + 4);
        let (tbar, toff) = ((table & 7) as u8, (table & !7) as u64);
        let t = Mmio(map.map(tbar)? + toff);
        t.w32(0, addr as u32);
        t.w32(4, (addr >> 32) as u32);
        t.w32(8, data);
        t.w32(12, 0);
        let mc = ((ctrl >> 16) as u16 | 0x8000) & !0x4000;
        cfg.write32(cap, (ctrl & 0xffff) | ((mc as u32) << 16));
        self.common.w16(C_MSIX_CFG, NO_VECTOR);
        Ok(())
    }

    /// Número de filas.
    pub fn num_queues(&self) -> u16 {
        self.common.r16(C_NUM_QUEUES)
    }

    /// Configura a fila `index` (tamanho ≤ máximo do dispositivo); devolve (tamanho, notify_off).
    pub fn setup_queue(
        &self,
        index: u16,
        size: u16,
        desc: u64,
        avail: u64,
        used: u64,
        msix_vector: u16,
    ) -> Result<(u16, u16), Error> {
        if index >= self.num_queues() {
            return Err(Error::NoQueue);
        }
        self.common.w16(C_Q_SEL, index);
        let max = self.common.r16(C_Q_SIZE);
        if max == 0 {
            return Err(Error::NoQueue);
        }
        let size = size.min(max);
        self.common.w16(C_Q_SIZE, size);
        self.common.w64(C_Q_DESC, desc);
        self.common.w64(C_Q_AVAIL, avail);
        self.common.w64(C_Q_USED, used);
        self.common.w16(C_Q_MSIX, msix_vector);
        let notify_off = self.common.r16(C_Q_NOTIFY_OFF);
        self.common.w16(C_Q_ENABLE, 1);
        Ok((size, notify_off))
    }

    /// Marca o driver como pronto.
    pub fn driver_ok(&self) {
        self.common
            .w8(C_STATUS, S_ACK | S_DRIVER | S_FEATURES_OK | S_DRIVER_OK);
    }

    /// Notifica a fila `index`.
    pub fn notify(&self, notify_off: u16, index: u16) {
        Mmio(self.notify_base + notify_off as u64 * self.notify_mult as u64).w16(0, index);
    }

    /// Lê (e limpa) o ISR.
    pub fn isr_ack(&self) -> u8 {
        self.isr.map(|m| m.r8(0)).unwrap_or(0)
    }
}

/// Página de DMA (4 KiB) mapeada no processo.
#[derive(Clone, Copy, Debug, Default)]
pub struct DmaPage {
    /// Endereço virtual.
    pub virt: u64,
    /// Endereço físico.
    pub phys: u64,
}

/// Descritor: próximo encadeado.
pub const DESC_NEXT: u16 = 1;
/// Descritor: o dispositivo escreve.
pub const DESC_WRITE: u16 = 2;

/// Fila dividida (descritores, anel *available* e anel *used* em páginas separadas).
pub struct SplitQueue {
    index: u16,
    size: u16,
    notify_off: u16,
    desc: DmaPage,
    avail: DmaPage,
    used: DmaPage,
    avail_idx: u16,
    used_idx: u16,
}

impl SplitQueue {
    /// Tamanho máximo de fila que cabe em uma página de descritores (16 B cada).
    pub const MAX_SIZE: u16 = 256;

    /// Cria a fila já configurada no dispositivo (`setup_queue`).
    pub fn new(
        index: u16,
        size: u16,
        notify_off: u16,
        desc: DmaPage,
        avail: DmaPage,
        used: DmaPage,
    ) -> Self {
        SplitQueue {
            index,
            size: size.min(Self::MAX_SIZE),
            notify_off,
            desc,
            avail,
            used,
            avail_idx: 0,
            used_idx: 0,
        }
    }

    /// Tamanho da fila.
    pub fn size(&self) -> u16 {
        self.size
    }

    /// Escreve o descritor `i`.
    pub fn set_desc(&self, i: u16, addr: u64, len: u32, flags: u16, next: u16) {
        assert!(i < self.size);
        let base = self.desc.virt + 16 * i as u64;
        // SAFETY: página de DMA exclusiva da fila; `i < size ≤ 256` → dentro dos 4 KiB.
        unsafe {
            write_volatile(base as *mut u64, addr);
            write_volatile((base + 8) as *mut u32, len);
            write_volatile((base + 12) as *mut u16, flags);
            write_volatile((base + 14) as *mut u16, next);
        }
    }

    /// Publica a cadeia iniciada em `head` no anel *available* e notifica.
    pub fn submit(&mut self, transport: &Transport, head: u16) {
        let slot = self.avail.virt + 4 + 2 * (self.avail_idx % self.size) as u64;
        // SAFETY: página de DMA exclusiva; slot dentro de `4 + 2*size ≤ 516` bytes.
        unsafe {
            write_volatile(slot as *mut u16, head);
            fence(Ordering::SeqCst);
            self.avail_idx = self.avail_idx.wrapping_add(1);
            write_volatile((self.avail.virt + 2) as *mut u16, self.avail_idx);
            fence(Ordering::SeqCst);
        }
        transport.notify(self.notify_off, self.index);
    }

    /// Índice *used* atual do dispositivo.
    fn device_used_idx(&self) -> u16 {
        // SAFETY: página de DMA exclusiva; offset 2 dentro da página.
        unsafe { read_volatile((self.used.virt + 2) as *const u16) }
    }

    /// Se o dispositivo completou uma cadeia, devolve (id do descritor cabeça, bytes escritos).
    pub fn pop_used(&mut self) -> Option<(u32, u32)> {
        let dev = self.device_used_idx();
        if dev == self.used_idx {
            return None;
        }
        fence(Ordering::SeqCst);
        let elem = self.used.virt + 4 + 8 * (self.used_idx % self.size) as u64;
        // SAFETY: página de DMA exclusiva; elemento dentro de `4 + 8*size ≤ 2052` bytes.
        let (id, len) = unsafe {
            (
                read_volatile(elem as *const u32),
                read_volatile((elem + 4) as *const u32),
            )
        };
        self.used_idx = self.used_idx.wrapping_add(1);
        Some((id, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeCfg([u8; 256]);
    impl PciConfig for FakeCfg {
        fn read32(&mut self, off: u16) -> u32 {
            let o = off as usize & !3;
            u32::from_le_bytes([self.0[o], self.0[o + 1], self.0[o + 2], self.0[o + 3]])
        }
        fn write32(&mut self, off: u16, v: u32) {
            let o = off as usize & !3;
            self.0[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
    }

    fn cfg_with_caps() -> FakeCfg {
        let mut c = [0u8; 256];
        c[0x06] = 0x10; // status: capabilities list
        c[0x34] = 0x40;
        // vendor cap comum em 0x40 → next 0x50
        c[0x40] = 0x09;
        c[0x41] = 0x50;
        c[0x42] = 16;
        c[0x43] = 1;
        c[0x44] = 4; // bar 4
        c[0x48..0x4c].copy_from_slice(&0x0000u32.to_le_bytes());
        // notify em 0x50 → next 0x64
        c[0x50] = 0x09;
        c[0x51] = 0x64;
        c[0x53] = 2;
        c[0x54] = 4;
        c[0x58..0x5c].copy_from_slice(&0x3000u32.to_le_bytes());
        c[0x60..0x64].copy_from_slice(&4u32.to_le_bytes());
        // msix em 0x64 → next 0x70
        c[0x64] = 0x11;
        c[0x65] = 0x70;
        // device cfg em 0x70 → fim; bar invalido (7) deve ser ignorado
        c[0x70] = 0x09;
        c[0x71] = 0;
        c[0x73] = 4;
        c[0x74] = 7;
        FakeCfg(c)
    }

    #[test]
    fn parses_capabilities() {
        let caps = parse_caps(&mut cfg_with_caps());
        assert_eq!(caps.common, Some((4, 0)));
        assert_eq!(caps.notify, Some((4, 0x3000, 4)));
        assert_eq!(caps.msix, Some(0x64));
        assert_eq!(caps.device, None);
        assert_eq!(caps.isr, None);
    }

    #[test]
    fn no_capability_list() {
        let caps = parse_caps(&mut FakeCfg([0; 256]));
        assert_eq!(caps, Caps::default());
    }

    #[test]
    fn capability_loop_terminates() {
        let mut c = cfg_with_caps();
        c.0[0x71] = 0x40; // ciclo
        let caps = parse_caps(&mut c);
        assert_eq!(caps.common, Some((4, 0)));
    }

    #[test]
    fn device_types() {
        assert_eq!(device_type(0x1042), Some(TYPE_BLOCK));
        assert_eq!(device_type(0x1001), Some(TYPE_BLOCK));
        assert_eq!(device_type(0x1044), Some(TYPE_RNG));
        assert_eq!(device_type(0x1005), Some(TYPE_RNG));
        assert_eq!(device_type(0x2000), None);
    }

    #[test]
    fn split_queue_rings() {
        let mut desc = [0u8; 4096];
        let mut avail = [0u8; 4096];
        let mut used = [0u8; 4096];
        let pages = [
            DmaPage {
                virt: desc.as_mut_ptr() as u64,
                phys: 0x1000,
            },
            DmaPage {
                virt: avail.as_mut_ptr() as u64,
                phys: 0x2000,
            },
            DmaPage {
                virt: used.as_mut_ptr() as u64,
                phys: 0x3000,
            },
        ];
        let mut q = SplitQueue::new(0, 8, 0, pages[0], pages[1], pages[2]);
        q.set_desc(3, 0xdead_beef, 512, DESC_NEXT | DESC_WRITE, 4);
        assert_eq!(
            &desc[48..64],
            &[0xef, 0xbe, 0xad, 0xde, 0, 0, 0, 0, 0, 2, 0, 0, 3, 0, 4, 0]
        );
        // Sem transporte real: escreve o anel diretamente.
        assert_eq!(q.pop_used(), None);
        used[2..4].copy_from_slice(&1u16.to_le_bytes());
        used[4..8].copy_from_slice(&3u32.to_le_bytes());
        used[8..12].copy_from_slice(&512u32.to_le_bytes());
        assert_eq!(q.pop_used(), Some((3, 512)));
        assert_eq!(q.pop_used(), None);
        assert_eq!(q.size(), 8);
    }
}

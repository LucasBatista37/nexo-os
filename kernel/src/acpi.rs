//! Descoberta de plataforma via ACPI: CPUs, LAPIC, I/O APICs, overrides e HPET.

use nexo_acpi::{Hpet, Madt, MadtEntry, Rsdp, TableReader, find_table};
use nexo_arch_x86_64::cpu;
use nexo_mm::PhysAddr;
use nexo_sync::Once;

/// Máximo de CPUs suportadas nesta versão.
pub const MAX_CPUS: usize = 64;
const MAX_IOAPICS: usize = 8;
const MAX_OVERRIDES: usize = 24;

/// Leitor de tabelas pelo physmap.
struct PhysMapReader;

impl TableReader for PhysMapReader {
    fn read(&self, phys: u64, len: usize) -> Option<&[u8]> {
        let end = phys.checked_add(len as u64)?;
        if end > crate::boot::info().phys_map_size {
            return None;
        }
        let ptr = crate::mm::virt::phys_to_virt(PhysAddr::new(phys)).as_ptr::<u8>();
        // SAFETY: intervalo dentro do physmap; tabelas ACPI são somente leitura.
        Some(unsafe { core::slice::from_raw_parts(ptr, len) })
    }
}

/// Uma CPU descoberta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuInfo {
    /// UID ACPI.
    pub acpi_id: u32,
    /// APIC ID.
    pub apic_id: u32,
}

/// Um I/O APIC descoberto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IoApicInfo {
    /// ID.
    pub id: u8,
    /// Endereço físico.
    pub phys: u64,
    /// Primeiro GSI.
    pub gsi_base: u32,
}

/// Override IRQ ISA → GSI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IsaOverride {
    /// IRQ ISA.
    pub irq: u8,
    /// GSI.
    pub gsi: u32,
    /// Flags MPS INTI (polaridade bits 0..2, gatilho bits 2..4).
    pub flags: u16,
}

/// Informação de plataforma consolidada.
pub struct PlatformInfo {
    /// Revisão do RSDP.
    pub acpi_revision: u8,
    /// OEM ID.
    pub oem_id: [u8; 6],
    /// Endereço físico do LAPIC (0 se desconhecido).
    pub lapic_phys: u64,
    /// PIC 8259 presente.
    pub pic_present: bool,
    cpus: [CpuInfo; MAX_CPUS],
    cpu_count: usize,
    ioapics: [IoApicInfo; MAX_IOAPICS],
    ioapic_count: usize,
    overrides: [IsaOverride; MAX_OVERRIDES],
    override_count: usize,
    /// HPET (se houver).
    pub hpet: Option<Hpet>,
    /// APIC ID da CPU de boot.
    pub bsp_apic_id: u32,
}

impl PlatformInfo {
    /// CPUs habilitadas (a BSP primeiro).
    pub fn cpus(&self) -> &[CpuInfo] {
        &self.cpus[..self.cpu_count]
    }
    /// I/O APICs.
    pub fn ioapics(&self) -> &[IoApicInfo] {
        &self.ioapics[..self.ioapic_count]
    }
    /// Overrides ISA.
    pub fn overrides(&self) -> &[IsaOverride] {
        &self.overrides[..self.override_count]
    }
    /// GSI e flags de uma IRQ ISA.
    pub fn isa_irq_to_gsi(&self, irq: u8) -> (u32, u16) {
        self.overrides()
            .iter()
            .find(|o| o.irq == irq)
            .map_or((irq as u32, 0), |o| (o.gsi, o.flags))
    }
}

static INFO: Once<PlatformInfo> = Once::new();

/// Lê RSDP/XSDT/MADT/HPET e publica [`PlatformInfo`]. Sem ACPI, assume 1 CPU.
pub fn init() {
    let bi = crate::boot::info();
    let (_, _, is_bsp) = nexo_arch_x86_64::apic::apic_base();
    let bsp_apic_id = (cpu::cpuid(1, 0).ebx >> 24) & 0xff;
    let mut info = PlatformInfo {
        acpi_revision: 0,
        oem_id: [0; 6],
        lapic_phys: 0,
        pic_present: true,
        cpus: [CpuInfo::default(); MAX_CPUS],
        cpu_count: 0,
        ioapics: [IoApicInfo::default(); MAX_IOAPICS],
        ioapic_count: 0,
        overrides: [IsaOverride::default(); MAX_OVERRIDES],
        override_count: 0,
        hpet: None,
        bsp_apic_id,
    };
    let vendor = cpu::vendor();
    kinfo!(
        "acpi: CPU de boot apic_id={} vendor={} bsp_msr={}",
        bsp_apic_id,
        core::str::from_utf8(&vendor).unwrap_or("?"),
        is_bsp
    );

    let reader = PhysMapReader;
    let parsed = (bi.rsdp_addr != 0)
        .then(|| Rsdp::parse(&reader, bi.rsdp_addr))
        .transpose();
    match parsed {
        Ok(Some(rsdp)) => {
            info.acpi_revision = rsdp.revision;
            info.oem_id = rsdp.oem_id;
            kinfo!(
                "acpi: RSDP rev {} oem \"{}\" xsdt={:#x} rsdt={:#x}",
                rsdp.revision,
                core::str::from_utf8(&rsdp.oem_id).unwrap_or("?").trim(),
                rsdp.xsdt_addr,
                rsdp.rsdt_addr
            );
            match find_table(&reader, &rsdp, b"APIC").and_then(|t| Madt::parse(&t)) {
                Ok(madt) => fill_from_madt(&mut info, &madt),
                Err(e) => kwarn!("acpi: MADT indisponivel ({e}); assumindo 1 CPU"),
            }
            match find_table(&reader, &rsdp, b"HPET").and_then(|t| Hpet::parse(&t)) {
                Ok(h) => {
                    kinfo!("acpi: HPET em {:#x} (min tick {})", h.address, h.min_tick);
                    info.hpet = Some(h);
                }
                Err(e) => kdebug!("acpi: HPET ausente ({e})"),
            }
        }
        Ok(None) => kwarn!("acpi: loader nao entregou RSDP; assumindo 1 CPU"),
        Err(e) => kwarn!("acpi: RSDP invalido ({e}); assumindo 1 CPU"),
    }
    if info.cpu_count == 0 {
        info.cpus[0] = CpuInfo {
            acpi_id: 0,
            apic_id: bsp_apic_id,
        };
        info.cpu_count = 1;
    }
    kinfo!(
        "acpi: {} CPU(s), {} I/O APIC(s), {} override(s), LAPIC em {:#x}, PIC {}",
        info.cpu_count,
        info.ioapic_count,
        info.override_count,
        info.lapic_phys,
        if info.pic_present {
            "presente"
        } else {
            "ausente"
        }
    );
    let _ = INFO.set(info);
}

fn fill_from_madt(info: &mut PlatformInfo, madt: &Madt<'_>) {
    info.lapic_phys = madt.lapic_phys();
    info.pic_present = madt.flags & 1 != 0;
    for e in madt.entries() {
        match e {
            MadtEntry::LocalApic {
                acpi_id,
                apic_id,
                flags,
            } => {
                kinfo!("acpi:   cpu acpi_id={acpi_id} apic_id={apic_id} flags={flags:#x}");
            }
            MadtEntry::LocalX2Apic {
                x2apic_id,
                flags,
                acpi_uid,
            } => {
                kinfo!("acpi:   cpu(x2) uid={acpi_uid} x2apic_id={x2apic_id} flags={flags:#x}");
            }
            MadtEntry::IoApic {
                id,
                address,
                gsi_base,
            } => {
                kinfo!("acpi:   ioapic id={id} em {address:#x} gsi_base={gsi_base}");
                if info.ioapic_count < MAX_IOAPICS {
                    info.ioapics[info.ioapic_count] = IoApicInfo {
                        id,
                        phys: address as u64,
                        gsi_base,
                    };
                    info.ioapic_count += 1;
                }
            }
            MadtEntry::InterruptSourceOverride {
                bus,
                source,
                gsi,
                flags,
            } => {
                kinfo!("acpi:   override bus={bus} irq={source} -> gsi={gsi} flags={flags:#x}");
                if bus == 0 && info.override_count < MAX_OVERRIDES {
                    info.overrides[info.override_count] = IsaOverride {
                        irq: source,
                        gsi,
                        flags,
                    };
                    info.override_count += 1;
                }
            }
            MadtEntry::LocalApicNmi {
                acpi_id,
                lint,
                flags,
            } => {
                kdebug!("acpi:   lapic nmi acpi_id={acpi_id:#x} lint={lint} flags={flags:#x}");
            }
            MadtEntry::LocalApicAddressOverride { address } => {
                kinfo!("acpi:   lapic address override {address:#x}");
            }
            MadtEntry::Other { kind, len } => {
                kdebug!("acpi:   entrada tipo {kind} ({len} bytes) ignorada")
            }
        }
    }
    // BSP primeiro, depois as demais, limitadas a MAX_CPUS.
    for (acpi_id, apic_id) in madt.enabled_cpus() {
        if apic_id == info.bsp_apic_id {
            info.cpus[0] = CpuInfo { acpi_id, apic_id };
            info.cpu_count = info.cpu_count.max(1);
        }
    }
    if info.cpu_count == 0 {
        kwarn!(
            "acpi: BSP (apic_id {}) nao aparece na MADT",
            info.bsp_apic_id
        );
        info.cpus[0] = CpuInfo {
            acpi_id: 0,
            apic_id: info.bsp_apic_id,
        };
        info.cpu_count = 1;
    }
    for (acpi_id, apic_id) in madt.enabled_cpus() {
        if apic_id != info.bsp_apic_id {
            if info.cpu_count >= MAX_CPUS {
                kwarn!("acpi: mais de {MAX_CPUS} CPUs; excedentes ignoradas");
                break;
            }
            info.cpus[info.cpu_count] = CpuInfo { acpi_id, apic_id };
            info.cpu_count += 1;
        }
    }
}

/// Informação de plataforma.
pub fn info() -> &'static PlatformInfo {
    INFO.get().expect("acpi::init nao foi chamado")
}

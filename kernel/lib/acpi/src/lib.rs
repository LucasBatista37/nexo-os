//! Parser mínimo de ACPI para o bring-up: RSDP → XSDT/RSDT → MADT (CPUs,
//! LAPIC, I/O APIC, overrides) e HPET.
//!
//! O crate não acessa memória diretamente: recebe um [`TableReader`] que
//! materializa bytes a partir de endereços físicos (physmap no kernel, buffers
//! sintéticos nos testes). Nada aqui aloca.
#![no_std]
#![deny(unsafe_code)]

use core::fmt;

/// Fornece acesso a memória física para o parser.
pub trait TableReader {
    /// Bytes `[phys, phys+len)`, ou `None` se a região não for acessível.
    fn read(&self, phys: u64, len: usize) -> Option<&[u8]>;
}

/// Erros de análise.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AcpiError {
    /// Assinatura inválida.
    BadSignature,
    /// Soma de verificação inválida.
    BadChecksum,
    /// Região inacessível ou tabela truncada.
    Unreadable(u64),
    /// Comprimento inconsistente.
    BadLength,
    /// Tabela não encontrada.
    NotFound,
}

impl fmt::Display for AcpiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AcpiError::BadSignature => write!(f, "assinatura invalida"),
            AcpiError::BadChecksum => write!(f, "checksum invalido"),
            AcpiError::Unreadable(p) => write!(f, "regiao inacessivel em {p:#x}"),
            AcpiError::BadLength => write!(f, "comprimento inconsistente"),
            AcpiError::NotFound => write!(f, "tabela nao encontrada"),
        }
    }
}

fn checksum_ok(b: &[u8]) -> bool {
    b.iter().fold(0u8, |a, &x| a.wrapping_add(x)) == 0
}
fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

/// RSDP validado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rsdp {
    /// Revisão (0 = ACPI 1.0, ≥ 2 = ACPI 2.0+).
    pub revision: u8,
    /// Endereço físico da RSDT (32 bits).
    pub rsdt_addr: u32,
    /// Endereço físico da XSDT (0 se revisão < 2).
    pub xsdt_addr: u64,
    /// OEM ID.
    pub oem_id: [u8; 6],
}

impl Rsdp {
    /// Lê e valida o RSDP em `phys`.
    pub fn parse(reader: &impl TableReader, phys: u64) -> Result<Rsdp, AcpiError> {
        let b = reader.read(phys, 20).ok_or(AcpiError::Unreadable(phys))?;
        if &b[0..8] != b"RSD PTR " {
            return Err(AcpiError::BadSignature);
        }
        if !checksum_ok(&b[..20]) {
            return Err(AcpiError::BadChecksum);
        }
        let revision = b[15];
        let rsdt_addr = u32_at(b, 16).ok_or(AcpiError::BadLength)?;
        let mut oem_id = [0u8; 6];
        oem_id.copy_from_slice(&b[9..15]);
        let mut xsdt_addr = 0;
        if revision >= 2 {
            let ext = reader.read(phys, 36).ok_or(AcpiError::Unreadable(phys))?;
            let len = u32_at(ext, 20).ok_or(AcpiError::BadLength)? as usize;
            if len < 36 {
                return Err(AcpiError::BadLength);
            }
            let full = reader.read(phys, len).ok_or(AcpiError::Unreadable(phys))?;
            if !checksum_ok(full) {
                return Err(AcpiError::BadChecksum);
            }
            xsdt_addr = u64_at(ext, 24).ok_or(AcpiError::BadLength)?;
        }
        Ok(Rsdp {
            revision,
            rsdt_addr,
            xsdt_addr,
            oem_id,
        })
    }
}

/// Cabeçalho comum das tabelas de descrição (36 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SdtHeader {
    /// Assinatura ASCII (ex.: `APIC`, `HPET`).
    pub signature: [u8; 4],
    /// Comprimento total, incluindo o cabeçalho.
    pub length: u32,
    /// Revisão.
    pub revision: u8,
    /// OEM ID.
    pub oem_id: [u8; 6],
}

/// Tabela validada (cabeçalho + bytes completos).
#[derive(Clone, Copy, Debug)]
pub struct Table<'a> {
    /// Cabeçalho.
    pub header: SdtHeader,
    /// Endereço físico.
    pub phys: u64,
    /// Bytes completos (inclui cabeçalho).
    pub bytes: &'a [u8],
}

impl<'a> Table<'a> {
    /// Lê e valida a tabela em `phys` (assinatura, comprimento, checksum).
    pub fn parse(reader: &'a impl TableReader, phys: u64) -> Result<Table<'a>, AcpiError> {
        let h = reader.read(phys, 36).ok_or(AcpiError::Unreadable(phys))?;
        let length = u32_at(h, 4).ok_or(AcpiError::BadLength)?;
        if !(36..=16 * 1024 * 1024).contains(&length) {
            return Err(AcpiError::BadLength);
        }
        let bytes = reader
            .read(phys, length as usize)
            .ok_or(AcpiError::Unreadable(phys))?;
        if !checksum_ok(bytes) {
            return Err(AcpiError::BadChecksum);
        }
        let mut signature = [0u8; 4];
        signature.copy_from_slice(&bytes[0..4]);
        let mut oem_id = [0u8; 6];
        oem_id.copy_from_slice(&bytes[10..16]);
        Ok(Table {
            header: SdtHeader {
                signature,
                length,
                revision: bytes[8],
                oem_id,
            },
            phys,
            bytes,
        })
    }

    /// Dados após o cabeçalho.
    pub fn payload(&self) -> &'a [u8] {
        &self.bytes[36..]
    }
}

/// Itera os ponteiros de tabela de uma XSDT (64 bits) ou RSDT (32 bits).
#[derive(Debug)]
pub struct RootTables<'a> {
    entries: &'a [u8],
    width: usize,
    pos: usize,
}

impl Iterator for RootTables<'_> {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        if self.pos + self.width > self.entries.len() {
            return None;
        }
        let v = if self.width == 8 {
            u64_at(self.entries, self.pos)?
        } else {
            u32_at(self.entries, self.pos)? as u64
        };
        self.pos += self.width;
        Some(v)
    }
}

/// Abre a XSDT (preferida) ou RSDT e devolve o iterador de tabelas.
pub fn root_tables<'a>(
    reader: &'a impl TableReader,
    rsdp: &Rsdp,
) -> Result<RootTables<'a>, AcpiError> {
    let (phys, sig, width) = if rsdp.xsdt_addr != 0 {
        (rsdp.xsdt_addr, b"XSDT", 8)
    } else {
        (rsdp.rsdt_addr as u64, b"RSDT", 4)
    };
    let t = Table::parse(reader, phys)?;
    if &t.header.signature != sig {
        return Err(AcpiError::BadSignature);
    }
    Ok(RootTables {
        entries: t.payload(),
        width,
        pos: 0,
    })
}

/// Procura a tabela com `signature`.
pub fn find_table<'a>(
    reader: &'a impl TableReader,
    rsdp: &Rsdp,
    signature: &[u8; 4],
) -> Result<Table<'a>, AcpiError> {
    for phys in root_tables(reader, rsdp)? {
        if let Ok(t) = Table::parse(reader, phys)
            && &t.header.signature == signature
        {
            return Ok(t);
        }
    }
    Err(AcpiError::NotFound)
}

// ---------------------------------------------------------------------------
// MADT
// ---------------------------------------------------------------------------

/// Entrada da MADT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MadtEntry {
    /// Processador com Local APIC (tipo 0).
    LocalApic {
        /// ID do processador em ACPI.
        acpi_id: u8,
        /// ID do LAPIC.
        apic_id: u8,
        /// Bit 0: habilitado; bit 1: pode ser ligado online.
        flags: u32,
    },
    /// I/O APIC (tipo 1).
    IoApic {
        /// ID.
        id: u8,
        /// Endereço físico dos registradores.
        address: u32,
        /// Primeiro GSI atendido.
        gsi_base: u32,
    },
    /// Override de fonte de interrupção (tipo 2): IRQ ISA → GSI.
    InterruptSourceOverride {
        /// Barramento (0 = ISA).
        bus: u8,
        /// IRQ de origem.
        source: u8,
        /// GSI de destino.
        gsi: u32,
        /// Polaridade/gatilho (MPS INTI flags).
        flags: u16,
    },
    /// NMI no LAPIC (tipo 4).
    LocalApicNmi {
        /// Processador (0xff = todos).
        acpi_id: u8,
        /// Flags.
        flags: u16,
        /// LINT0/LINT1.
        lint: u8,
    },
    /// Override do endereço do LAPIC (tipo 5).
    LocalApicAddressOverride {
        /// Endereço físico de 64 bits.
        address: u64,
    },
    /// Processador com x2APIC (tipo 9).
    LocalX2Apic {
        /// ID x2APIC (32 bits).
        x2apic_id: u32,
        /// Flags.
        flags: u32,
        /// UID ACPI.
        acpi_uid: u32,
    },
    /// Tipo não interpretado.
    Other {
        /// Tipo.
        kind: u8,
        /// Comprimento.
        len: u8,
    },
}

/// MADT analisada.
#[derive(Clone, Copy, Debug)]
pub struct Madt<'a> {
    /// Endereço físico do LAPIC (32 bits; ver override).
    pub lapic_address: u32,
    /// Bit 0: PIC 8259 presente (PCAT_COMPAT).
    pub flags: u32,
    entries: &'a [u8],
}

impl<'a> Madt<'a> {
    /// Interpreta uma tabela `APIC`.
    pub fn parse(table: &Table<'a>) -> Result<Madt<'a>, AcpiError> {
        if &table.header.signature != b"APIC" {
            return Err(AcpiError::BadSignature);
        }
        let p = table.payload();
        let lapic_address = u32_at(p, 0).ok_or(AcpiError::BadLength)?;
        let flags = u32_at(p, 4).ok_or(AcpiError::BadLength)?;
        Ok(Madt {
            lapic_address,
            flags,
            entries: &p[8..],
        })
    }

    /// Itera as entradas.
    pub fn entries(&self) -> MadtEntries<'a> {
        MadtEntries {
            b: self.entries,
            pos: 0,
        }
    }

    /// Endereço físico efetivo do LAPIC (considera override de 64 bits).
    pub fn lapic_phys(&self) -> u64 {
        self.entries()
            .find_map(|e| match e {
                MadtEntry::LocalApicAddressOverride { address } => Some(address),
                _ => None,
            })
            .unwrap_or(self.lapic_address as u64)
    }

    /// Processadores habilitados (LAPIC e x2APIC), como `(acpi_id, apic_id)`.
    pub fn enabled_cpus(&self) -> impl Iterator<Item = (u32, u32)> + 'a {
        self.entries().filter_map(|e| match e {
            MadtEntry::LocalApic {
                acpi_id,
                apic_id,
                flags,
            } if flags & 0b11 != 0 => Some((acpi_id as u32, apic_id as u32)),
            MadtEntry::LocalX2Apic {
                x2apic_id,
                flags,
                acpi_uid,
            } if flags & 0b11 != 0 => Some((acpi_uid, x2apic_id)),
            _ => None,
        })
    }

    /// GSI correspondente a uma IRQ ISA, aplicando overrides.
    pub fn isa_irq_to_gsi(&self, irq: u8) -> (u32, u16) {
        self.entries()
            .find_map(|e| match e {
                MadtEntry::InterruptSourceOverride {
                    bus: 0,
                    source,
                    gsi,
                    flags,
                } if source == irq => Some((gsi, flags)),
                _ => None,
            })
            .unwrap_or((irq as u32, 0))
    }
}

/// Iterador de entradas da MADT.
pub struct MadtEntries<'a> {
    b: &'a [u8],
    pos: usize,
}

impl Iterator for MadtEntries<'_> {
    type Item = MadtEntry;
    fn next(&mut self) -> Option<MadtEntry> {
        let b = self.b.get(self.pos..)?;
        if b.len() < 2 {
            return None;
        }
        let (kind, len) = (b[0], b[1] as usize);
        if len < 2 || len > b.len() {
            return None;
        }
        let e = &b[..len];
        self.pos += len;
        Some(match kind {
            0 if len >= 8 => MadtEntry::LocalApic {
                acpi_id: e[2],
                apic_id: e[3],
                flags: u32_at(e, 4).unwrap_or(0),
            },
            1 if len >= 12 => MadtEntry::IoApic {
                id: e[2],
                address: u32_at(e, 4).unwrap_or(0),
                gsi_base: u32_at(e, 8).unwrap_or(0),
            },
            2 if len >= 10 => MadtEntry::InterruptSourceOverride {
                bus: e[2],
                source: e[3],
                gsi: u32_at(e, 4).unwrap_or(0),
                flags: u16_at(e, 8).unwrap_or(0),
            },
            4 if len >= 6 => MadtEntry::LocalApicNmi {
                acpi_id: e[2],
                flags: u16_at(e, 3).unwrap_or(0),
                lint: e[5],
            },
            5 if len >= 12 => MadtEntry::LocalApicAddressOverride {
                address: u64_at(e, 4).unwrap_or(0),
            },
            9 if len >= 16 => MadtEntry::LocalX2Apic {
                x2apic_id: u32_at(e, 4).unwrap_or(0),
                flags: u32_at(e, 8).unwrap_or(0),
                acpi_uid: u32_at(e, 12).unwrap_or(0),
            },
            _ => MadtEntry::Other {
                kind,
                len: len as u8,
            },
        })
    }
}

// ---------------------------------------------------------------------------
// HPET
// ---------------------------------------------------------------------------

/// Tabela HPET (apenas o necessário).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hpet {
    /// Endereço físico do bloco de registradores.
    pub address: u64,
    /// Número do bloco.
    pub sequence: u8,
    /// Período mínimo de tick.
    pub min_tick: u16,
    /// Bloco de ID de hardware (vendor/comparadores).
    pub event_timer_block_id: u32,
}

impl Hpet {
    /// Interpreta uma tabela `HPET`.
    pub fn parse(table: &Table<'_>) -> Result<Hpet, AcpiError> {
        if &table.header.signature != b"HPET" {
            return Err(AcpiError::BadSignature);
        }
        let p = table.payload();
        Ok(Hpet {
            event_timer_block_id: u32_at(p, 0).ok_or(AcpiError::BadLength)?,
            // Generic Address Structure: id(1) width(1) offset(1) access(1) address(8)
            address: u64_at(p, 8).ok_or(AcpiError::BadLength)?,
            sequence: *p.get(16).ok_or(AcpiError::BadLength)?,
            min_tick: u16_at(p, 17).ok_or(AcpiError::BadLength)?,
        })
    }
}

#[cfg(test)]
mod tests {
    /// PRNG determinístico para os testes "fuzz-lite" (sem dependências).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    /// Mutação aleatória de um buffer válido: bit flips, bytes aleatórios, truncamentos, extensões.
    fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
        let mut v = base.to_vec();
        match rng.below(5) {
            0 => {
                for _ in 0..1 + rng.below(8) {
                    if !v.is_empty() {
                        let i = rng.below(v.len());
                        v[i] ^= 1 << rng.below(8);
                    }
                }
            }
            1 => {
                for _ in 0..1 + rng.below(16) {
                    if !v.is_empty() {
                        let i = rng.below(v.len());
                        v[i] = rng.next() as u8;
                    }
                }
            }
            2 => v.truncate(rng.below(v.len() + 1)),
            3 => {
                let extra = rng.below(64);
                v.extend((0..extra).map(|_| rng.next() as u8));
            }
            _ => {
                if v.len() >= 8 {
                    let i = rng.below(v.len() - 7);
                    v[i..i + 8].copy_from_slice(&rng.next().to_le_bytes());
                }
            }
        }
        v
    }

    #[test]
    fn fuzz_lite_never_panics() {
        let (mem, rsdp_addr) = build();
        let mut rng = Rng(0xa5a5_5a5a_1357_2468);
        for _ in 0..5_000 {
            let mutated = Mem {
                base: mem.base,
                data: mutate(&mut rng, &mem.data),
            };
            if let Ok(rsdp) = Rsdp::parse(&mutated, rsdp_addr) {
                if let Ok(t) = find_table(&mutated, &rsdp, b"APIC")
                    && let Ok(m) = Madt::parse(&t)
                {
                    let _ = m.entries().count();
                    let _ = m.enabled_cpus().count();
                    let _ = m.lapic_phys();
                    let _ = m.isa_irq_to_gsi(0);
                }
                if let Ok(t) = find_table(&mutated, &rsdp, b"HPET") {
                    let _ = Hpet::parse(&t);
                }
            }
        }
    }

    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    /// "Memória física" sintética: um único buffer em `base`.
    struct Mem {
        base: u64,
        data: Vec<u8>,
    }
    impl TableReader for Mem {
        fn read(&self, phys: u64, len: usize) -> Option<&[u8]> {
            let off = phys.checked_sub(self.base)? as usize;
            self.data.get(off..off.checked_add(len)?)
        }
    }

    fn with_checksum(mut t: Vec<u8>) -> Vec<u8> {
        let sum = t.iter().fold(0u8, |a, &x| a.wrapping_add(x));
        t[9] = t[9].wrapping_sub(sum); // byte de checksum do cabeçalho SDT
        t
    }

    fn sdt(sig: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut t = vec![0u8; 36];
        t[0..4].copy_from_slice(sig);
        t[4..8].copy_from_slice(&((36 + payload.len()) as u32).to_le_bytes());
        t[8] = 1;
        t[10..16].copy_from_slice(b"NEXO  ");
        t.extend_from_slice(payload);
        with_checksum(t)
    }

    fn build() -> (Mem, u64) {
        // Layout: RSDP @ 0x1000, XSDT @ 0x2000, MADT @ 0x3000, HPET @ 0x4000, tabela lixo @ 0x5000
        let base = 0x1000u64;
        let mut data = vec![0u8; 0x5000];
        // MADT
        let mut madt = Vec::new();
        madt.extend_from_slice(&0xfee0_0000u32.to_le_bytes());
        madt.extend_from_slice(&1u32.to_le_bytes()); // PCAT_COMPAT
        madt.extend_from_slice(&[0, 8, 0, 0, 1, 0, 0, 0]); // LAPIC acpi 0 apic 0 enabled
        madt.extend_from_slice(&[0, 8, 1, 1, 1, 0, 0, 0]); // LAPIC acpi 1 apic 1
        madt.extend_from_slice(&[0, 8, 2, 2, 0, 0, 0, 0]); // LAPIC desabilitado
        madt.extend_from_slice(&[1, 12, 0, 0, 0x00, 0x00, 0xc0, 0xfe, 0, 0, 0, 0]); // IOAPIC id0 @fec00000 gsi 0
        madt.extend_from_slice(&[2, 10, 0, 0, 2, 0, 0, 0, 0, 0]); // ISO irq0 -> gsi2
        madt.extend_from_slice(&[2, 10, 0, 9, 9, 0, 0, 0, 0x0d, 0]); // ISO irq9 -> gsi9 flags 0xd
        madt.extend_from_slice(&[4, 6, 0xff, 5, 0, 1]); // LAPIC NMI all LINT1
        madt.extend_from_slice(&[9, 16, 0, 0, 7, 0, 0, 0, 1, 0, 0, 0, 7, 0, 0, 0]); // x2APIC id 7 uid 7
        madt.extend_from_slice(&[0x7f, 3, 0]); // tipo desconhecido
        let madt = sdt(b"APIC", &madt);
        // HPET
        let mut hpet = Vec::new();
        hpet.extend_from_slice(&0x8086_a201u32.to_le_bytes());
        hpet.extend_from_slice(&[0, 64, 0, 0]);
        hpet.extend_from_slice(&0xfed0_0000u64.to_le_bytes());
        hpet.push(0);
        hpet.extend_from_slice(&0x80u16.to_le_bytes());
        hpet.push(0);
        let hpet = sdt(b"HPET", &hpet);
        let junk = sdt(b"JUNK", &[1, 2, 3]);
        // XSDT
        let mut xs = Vec::new();
        for p in [0x3000u64, 0x4000, 0x5000] {
            xs.extend_from_slice(&p.to_le_bytes());
        }
        let xsdt = sdt(b"XSDT", &xs);
        // RSDP v2
        let mut rsdp = vec![0u8; 36];
        rsdp[0..8].copy_from_slice(b"RSD PTR ");
        rsdp[9..15].copy_from_slice(b"NEXO  ");
        rsdp[15] = 2;
        rsdp[16..20].copy_from_slice(&0u32.to_le_bytes());
        rsdp[20..24].copy_from_slice(&36u32.to_le_bytes());
        rsdp[24..32].copy_from_slice(&0x2000u64.to_le_bytes());
        let s1 = rsdp[..20].iter().fold(0u8, |a, &x| a.wrapping_add(x));
        rsdp[8] = rsdp[8].wrapping_sub(s1);
        let s2 = rsdp.iter().fold(0u8, |a, &x| a.wrapping_add(x));
        rsdp[32] = rsdp[32].wrapping_sub(s2);

        data[0..36].copy_from_slice(&rsdp);
        data[0x1000..0x1000 + xsdt.len()].copy_from_slice(&xsdt);
        data[0x2000..0x2000 + madt.len()].copy_from_slice(&madt);
        data[0x3000..0x3000 + hpet.len()].copy_from_slice(&hpet);
        data[0x4000..0x4000 + junk.len()].copy_from_slice(&junk);
        (Mem { base, data }, base)
    }

    #[test]
    fn parses_rsdp_and_finds_tables() {
        let (mem, rsdp_addr) = build();
        let rsdp = Rsdp::parse(&mem, rsdp_addr).unwrap();
        assert_eq!(rsdp.revision, 2);
        assert_eq!(rsdp.xsdt_addr, 0x2000);
        assert_eq!(&rsdp.oem_id, b"NEXO  ");
        let all: Vec<u64> = root_tables(&mem, &rsdp).unwrap().collect();
        assert_eq!(all, vec![0x3000, 0x4000, 0x5000]);
        let madt_t = find_table(&mem, &rsdp, b"APIC").unwrap();
        assert_eq!(madt_t.phys, 0x3000);
        assert_eq!(
            find_table(&mem, &rsdp, b"FACP").unwrap_err(),
            AcpiError::NotFound
        );
    }

    #[test]
    fn parses_madt() {
        let (mem, rsdp_addr) = build();
        let rsdp = Rsdp::parse(&mem, rsdp_addr).unwrap();
        let t = find_table(&mem, &rsdp, b"APIC").unwrap();
        let m = Madt::parse(&t).unwrap();
        assert_eq!(m.lapic_address, 0xfee0_0000);
        assert_eq!(m.lapic_phys(), 0xfee0_0000);
        assert_eq!(m.flags & 1, 1);
        let cpus: Vec<(u32, u32)> = m.enabled_cpus().collect();
        assert_eq!(cpus, vec![(0, 0), (1, 1), (7, 7)]);
        assert_eq!(m.entries().count(), 9);
        assert!(matches!(
            m.entries().nth(3),
            Some(MadtEntry::IoApic {
                id: 0,
                address: 0xfec0_0000,
                gsi_base: 0
            })
        ));
        assert_eq!(m.isa_irq_to_gsi(0), (2, 0));
        assert_eq!(m.isa_irq_to_gsi(9), (9, 0x0d));
        assert_eq!(m.isa_irq_to_gsi(4), (4, 0));
        assert!(matches!(
            m.entries().nth(6),
            Some(MadtEntry::LocalApicNmi {
                acpi_id: 0xff,
                lint: 1,
                ..
            })
        ));
        assert!(matches!(
            m.entries().last(),
            Some(MadtEntry::Other { kind: 0x7f, len: 3 })
        ));
    }

    #[test]
    fn parses_hpet() {
        let (mem, rsdp_addr) = build();
        let rsdp = Rsdp::parse(&mem, rsdp_addr).unwrap();
        let t = find_table(&mem, &rsdp, b"HPET").unwrap();
        let h = Hpet::parse(&t).unwrap();
        assert_eq!(h.address, 0xfed0_0000);
        assert_eq!(h.min_tick, 0x80);
        assert_eq!(h.event_timer_block_id, 0x8086_a201);
        assert_eq!(Madt::parse(&t).unwrap_err(), AcpiError::BadSignature);
    }

    #[test]
    fn rejects_corruption() {
        let (mut mem, rsdp_addr) = build();
        mem.data[0x1000 + 20] ^= 0xff; // corrompe a XSDT (fis 0x2000 = data[0x1000])
        let rsdp = Rsdp::parse(&mem, rsdp_addr).unwrap();
        assert_eq!(
            root_tables(&mem, &rsdp).unwrap_err(),
            AcpiError::BadChecksum
        );
        mem.data[0] = b'X';
        assert_eq!(
            Rsdp::parse(&mem, rsdp_addr).unwrap_err(),
            AcpiError::BadSignature
        );
        let empty = Mem {
            base: 0,
            data: vec![],
        };
        assert_eq!(
            Rsdp::parse(&empty, 0).unwrap_err(),
            AcpiError::Unreadable(0)
        );
    }
}

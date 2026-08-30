//! Leitor mínimo de ELF64 x86_64 (apenas o necessário para carregar segmentos).
#![no_std]
#![deny(unsafe_code)]

/// Segmento carregável.
pub const PT_LOAD: u32 = 1;
/// Executável.
pub const PF_X: u32 = 1;
/// Gravável.
pub const PF_W: u32 = 2;
/// Legível.
pub const PF_R: u32 = 4;

const EM_X86_64: u16 = 0x3e;
const ET_EXEC: u16 = 2;

/// Erros de análise.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElfError {
    /// Assinatura ausente.
    NotElf,
    /// Não é ELF64 little-endian.
    WrongClass,
    /// Não é `ET_EXEC` (estático, sem relocação).
    NotExecutable,
    /// Não é x86_64.
    WrongMachine,
    /// Cabeçalho ou program headers truncados/inválidos.
    Truncated,
    /// Segmento aponta para fora do arquivo.
    BadSegment,
}

impl core::fmt::Display for ElfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            ElfError::NotElf => "assinatura ELF ausente",
            ElfError::WrongClass => "nao e ELF64 little-endian",
            ElfError::NotExecutable => "nao e executavel estatico (ET_EXEC)",
            ElfError::WrongMachine => "nao e x86_64",
            ElfError::Truncated => "cabecalho truncado",
            ElfError::BadSegment => "segmento fora do arquivo",
        };
        f.write_str(s)
    }
}

/// Program header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramHeader {
    /// Tipo (`PT_LOAD` = 1).
    pub p_type: u32,
    /// Flags `PF_*`.
    pub p_flags: u32,
    /// Deslocamento no arquivo.
    pub p_offset: u64,
    /// Endereço virtual.
    pub p_vaddr: u64,
    /// Bytes no arquivo.
    pub p_filesz: u64,
    /// Bytes em memória.
    pub p_memsz: u64,
}

impl ProgramHeader {
    /// `true` se deve ser carregado.
    pub fn is_load(&self) -> bool {
        self.p_type == PT_LOAD && self.p_memsz > 0
    }
    /// `true` se executável.
    pub fn executable(&self) -> bool {
        self.p_flags & PF_X != 0
    }
    /// `true` se gravável.
    pub fn writable(&self) -> bool {
        self.p_flags & PF_W != 0
    }
}

/// Arquivo ELF64 validado.
#[derive(Clone, Copy, Debug)]
pub struct ElfFile<'a> {
    data: &'a [u8],
    /// Ponto de entrada.
    pub entry: u64,
    phoff: usize,
    phentsize: usize,
    phnum: usize,
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

impl<'a> ElfFile<'a> {
    /// Valida cabeçalho: ELF64, little-endian, x86_64, executável estático.
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.get(0..4) != Some(b"\x7fELF") {
            return Err(ElfError::NotElf);
        }
        if data.get(4) != Some(&2) || data.get(5) != Some(&1) {
            return Err(ElfError::WrongClass);
        }
        if u16_at(data, 0x10).ok_or(ElfError::Truncated)? != ET_EXEC {
            return Err(ElfError::NotExecutable);
        }
        if u16_at(data, 0x12).ok_or(ElfError::Truncated)? != EM_X86_64 {
            return Err(ElfError::WrongMachine);
        }
        let entry = u64_at(data, 0x18).ok_or(ElfError::Truncated)?;
        let phoff = u64_at(data, 0x20).ok_or(ElfError::Truncated)? as usize;
        let phentsize = u16_at(data, 0x36).ok_or(ElfError::Truncated)? as usize;
        let phnum = u16_at(data, 0x38).ok_or(ElfError::Truncated)? as usize;
        if phentsize < 56 || phnum == 0 || phnum > 64 {
            return Err(ElfError::Truncated);
        }
        if phoff
            .checked_add(phentsize * phnum)
            .is_none_or(|end| end > data.len())
        {
            return Err(ElfError::Truncated);
        }
        Ok(ElfFile {
            data,
            entry,
            phoff,
            phentsize,
            phnum,
        })
    }

    /// Itera os program headers.
    pub fn program_headers(&self) -> impl Iterator<Item = ProgramHeader> + 'a {
        let data = self.data;
        let (phoff, phentsize) = (self.phoff, self.phentsize);
        (0..self.phnum).filter_map(move |i| {
            let b = &data[phoff + i * phentsize..];
            Some(ProgramHeader {
                p_type: u32_at(b, 0)?,
                p_flags: u32_at(b, 4)?,
                p_offset: u64_at(b, 8)?,
                p_vaddr: u64_at(b, 16)?,
                p_filesz: u64_at(b, 32)?,
                p_memsz: u64_at(b, 40)?,
            })
        })
    }

    /// Segmentos carregáveis.
    pub fn load_segments(&self) -> impl Iterator<Item = ProgramHeader> + 'a {
        self.program_headers().filter(|p| p.is_load())
    }

    /// Bytes do segmento no arquivo.
    pub fn segment_data(&self, ph: &ProgramHeader) -> Result<&'a [u8], ElfError> {
        if ph.p_filesz > ph.p_memsz {
            return Err(ElfError::BadSegment);
        }
        let start = ph.p_offset as usize;
        let end = start
            .checked_add(ph.p_filesz as usize)
            .ok_or(ElfError::BadSegment)?;
        self.data.get(start..end).ok_or(ElfError::BadSegment)
    }

    /// Menor e maior endereço virtual (exclusivo) entre os segmentos carregáveis.
    pub fn address_range(&self) -> Option<(u64, u64)> {
        let mut lo = u64::MAX;
        let mut hi = 0;
        for p in self.load_segments() {
            lo = lo.min(p.p_vaddr);
            hi = hi.max(p.p_vaddr.checked_add(p.p_memsz)?);
        }
        (lo < hi).then_some((lo, hi))
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
        let base = synthetic(
            0x40_1000,
            &[(1, 5, 0x100, 0x40_0000, 32), (1, 6, 0x200, 0x40_2000, 8)],
        );
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let mut ok = 0;
        for _ in 0..20_000 {
            let input = mutate(&mut rng, &base);
            if let Ok(elf) = ElfFile::parse(&input) {
                ok += 1;
                for ph in elf.program_headers() {
                    let _ = elf.segment_data(&ph);
                    let _ = ph.is_load();
                }
                let _ = elf.address_range();
            }
        }
        assert!(
            ok > 0,
            "nenhuma entrada mutada foi aceita (mutacao forte demais?)"
        );
    }

    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    fn synthetic(entry: u64, segs: &[(u32, u32, u64, u64, u64)]) -> Vec<u8> {
        let mut e = vec![0u8; 64];
        e[0..4].copy_from_slice(b"\x7fELF");
        e[4] = 2;
        e[5] = 1;
        e[0x10..0x12].copy_from_slice(&2u16.to_le_bytes());
        e[0x12..0x14].copy_from_slice(&0x3eu16.to_le_bytes());
        e[0x18..0x20].copy_from_slice(&entry.to_le_bytes());
        e[0x20..0x28].copy_from_slice(&64u64.to_le_bytes());
        e[0x36..0x38].copy_from_slice(&56u16.to_le_bytes());
        e[0x38..0x3a].copy_from_slice(&(segs.len() as u16).to_le_bytes());
        for (ty, flags, off, vaddr, size) in segs {
            let mut ph = [0u8; 56];
            ph[0..4].copy_from_slice(&ty.to_le_bytes());
            ph[4..8].copy_from_slice(&flags.to_le_bytes());
            ph[8..16].copy_from_slice(&off.to_le_bytes());
            ph[16..24].copy_from_slice(&vaddr.to_le_bytes());
            ph[32..40].copy_from_slice(&size.to_le_bytes());
            ph[40..48].copy_from_slice(&(size + 16).to_le_bytes());
            e.extend_from_slice(&ph);
        }
        e.resize(4096, 0xAB);
        e
    }

    #[test]
    fn parses_segments() {
        let data = synthetic(
            0x40_1000,
            &[
                (1, 5, 0x100, 0x40_0000, 32),
                (1, 6, 0x200, 0x40_2000, 8),
                (4, 4, 0, 0, 4),
            ],
        );
        let elf = ElfFile::parse(&data).unwrap();
        assert_eq!(elf.entry, 0x40_1000);
        assert_eq!(elf.program_headers().count(), 3);
        let segs: Vec<_> = elf.load_segments().collect();
        assert_eq!(segs.len(), 2);
        assert!(segs[0].executable() && !segs[0].writable());
        assert!(segs[1].writable());
        assert_eq!(elf.segment_data(&segs[0]).unwrap().len(), 32);
        assert_eq!(elf.address_range(), Some((0x40_0000, 0x40_2000 + 24)));
    }

    #[test]
    fn rejects_bad_headers() {
        assert_eq!(ElfFile::parse(b"nope").unwrap_err(), ElfError::NotElf);
        let mut d = synthetic(0, &[(1, 5, 0, 0x1000, 8)]);
        d[4] = 1;
        assert_eq!(ElfFile::parse(&d).unwrap_err(), ElfError::WrongClass);
        let mut d = synthetic(0, &[(1, 5, 0, 0x1000, 8)]);
        d[0x10] = 3;
        assert_eq!(ElfFile::parse(&d).unwrap_err(), ElfError::NotExecutable);
        let mut d = synthetic(0, &[(1, 5, 0, 0x1000, 8)]);
        d[0x12] = 0x03;
        assert_eq!(ElfFile::parse(&d).unwrap_err(), ElfError::WrongMachine);
        let d = synthetic(0, &[(1, 5, 0x10_0000, 0x1000, 8)]);
        let elf = ElfFile::parse(&d).unwrap();
        let seg = elf.load_segments().next().unwrap();
        assert_eq!(elf.segment_data(&seg).unwrap_err(), ElfError::BadSegment);
    }
}

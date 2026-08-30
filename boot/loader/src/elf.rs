//! Leitor mínimo de ELF64 (apenas o necessário para carregar o kernel).

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

/// Program header.
#[derive(Clone, Copy, Debug)]
pub struct ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
}

/// Arquivo ELF64 validado.
pub struct ElfFile<'a> {
    data: &'a [u8],
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
    pub fn parse(data: &'a [u8]) -> Result<Self, &'static str> {
        if data.get(0..4) != Some(b"\x7fELF") {
            return Err("assinatura ELF ausente");
        }
        if data.get(4) != Some(&2) {
            return Err("nao e ELF64");
        }
        if data.get(5) != Some(&1) {
            return Err("nao e little-endian");
        }
        if u16_at(data, 0x10).ok_or("cabecalho truncado")? != ET_EXEC {
            return Err("nao e executavel estatico (ET_EXEC)");
        }
        if u16_at(data, 0x12).ok_or("cabecalho truncado")? != EM_X86_64 {
            return Err("nao e x86_64");
        }
        let entry = u64_at(data, 0x18).ok_or("cabecalho truncado")?;
        let phoff = u64_at(data, 0x20).ok_or("cabecalho truncado")? as usize;
        let phentsize = u16_at(data, 0x36).ok_or("cabecalho truncado")? as usize;
        let phnum = u16_at(data, 0x38).ok_or("cabecalho truncado")? as usize;
        if phentsize < 56 || phnum == 0 {
            return Err("program headers invalidos");
        }
        if phoff
            .checked_add(phentsize * phnum)
            .is_none_or(|end| end > data.len())
        {
            return Err("program headers fora do arquivo");
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
    pub fn program_headers(&self) -> impl Iterator<Item = ProgramHeader> + '_ {
        (0..self.phnum).filter_map(move |i| {
            let b = &self.data[self.phoff + i * self.phentsize..];
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

    /// Bytes do segmento no arquivo.
    pub fn segment_data(&self, ph: &ProgramHeader) -> Result<&'a [u8], &'static str> {
        let start = ph.p_offset as usize;
        let end = start
            .checked_add(ph.p_filesz as usize)
            .ok_or("segmento com overflow")?;
        self.data.get(start..end).ok_or("segmento fora do arquivo")
    }
}

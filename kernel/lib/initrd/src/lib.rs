//! Initramfs do Nexo OS ("NEXOIRD1"): um arquivo simples, somente leitura,
//! com vários membros nomeados. Gerado por `tools/mkinitrd.py`.
//!
//! Layout (little-endian):
//! - cabeçalho (16 B): magic `NEXOIRD1`, `count: u32`, `reserved: u32`;
//! - `count` entradas de 48 B: `name: [u8; 32]` (UTF-8, preenchido com 0),
//!   `offset: u64`, `size: u64` (relativos ao início do arquivo);
//! - dados.
#![no_std]
#![deny(unsafe_code)]

/// Assinatura.
pub const MAGIC: &[u8; 8] = b"NEXOIRD1";
/// Tamanho do cabeçalho.
pub const HEADER_SIZE: usize = 16;
/// Tamanho de uma entrada.
pub const ENTRY_SIZE: usize = 48;
/// Tamanho máximo de nome.
pub const NAME_MAX: usize = 32;

/// Erros de leitura.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InitrdError {
    /// Assinatura inválida.
    BadMagic,
    /// Cabeçalho/tabela truncados.
    Truncated,
    /// Entrada aponta para fora do arquivo.
    BadEntry,
}

/// Arquivo initrd validado.
#[derive(Clone, Copy, Debug)]
pub struct Initrd<'a> {
    data: &'a [u8],
    count: usize,
}

/// Um membro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member<'a> {
    /// Nome.
    pub name: &'a str,
    /// Conteúdo.
    pub data: &'a [u8],
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

impl<'a> Initrd<'a> {
    /// Valida cabeçalho e tabela.
    pub fn parse(data: &'a [u8]) -> Result<Initrd<'a>, InitrdError> {
        if data.get(0..8) != Some(MAGIC.as_slice()) {
            return Err(InitrdError::BadMagic);
        }
        let count = u32_at(data, 8).ok_or(InitrdError::Truncated)? as usize;
        if count > 4096 || HEADER_SIZE + count * ENTRY_SIZE > data.len() {
            return Err(InitrdError::Truncated);
        }
        let ird = Initrd { data, count };
        for i in 0..count {
            ird.member(i).ok_or(InitrdError::BadEntry)?;
        }
        Ok(ird)
    }

    /// Número de membros.
    pub fn len(&self) -> usize {
        self.count
    }

    /// `true` se vazio.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn member(&self, i: usize) -> Option<Member<'a>> {
        let base = HEADER_SIZE + i * ENTRY_SIZE;
        let raw = self.data.get(base..base + NAME_MAX)?;
        let end = raw.iter().position(|&c| c == 0).unwrap_or(NAME_MAX);
        let name = core::str::from_utf8(&raw[..end]).ok()?;
        let offset = u64_at(self.data, base + 32)? as usize;
        let size = u64_at(self.data, base + 40)? as usize;
        let data = self.data.get(offset..offset.checked_add(size)?)?;
        Some(Member { name, data })
    }

    /// Itera os membros.
    pub fn iter(&self) -> impl Iterator<Item = Member<'a>> + '_ {
        (0..self.count).filter_map(move |i| self.member(i))
    }

    /// Procura por nome.
    pub fn find(&self, name: &str) -> Option<&'a [u8]> {
        self.iter().find(|m| m.name == name).map(|m| m.data)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn pack(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(members.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let mut off = HEADER_SIZE + members.len() * ENTRY_SIZE;
        let mut table = Vec::new();
        let mut blob = Vec::new();
        for (name, data) in members {
            let mut n = [0u8; NAME_MAX];
            n[..name.len()].copy_from_slice(name.as_bytes());
            table.extend_from_slice(&n);
            table.extend_from_slice(&(off as u64).to_le_bytes());
            table.extend_from_slice(&(data.len() as u64).to_le_bytes());
            blob.extend_from_slice(data);
            off += data.len();
        }
        out.extend_from_slice(&table);
        out.extend_from_slice(&blob);
        out
    }

    #[test]
    fn roundtrip() {
        let img = pack(&[
            ("init", b"\x7fELF-init"),
            ("svcmgr", b"\x7fELF-svc"),
            ("vazio", b""),
        ]);
        let ird = Initrd::parse(&img).unwrap();
        assert_eq!(ird.len(), 3);
        assert_eq!(ird.find("init"), Some(b"\x7fELF-init".as_slice()));
        assert_eq!(ird.find("svcmgr").map(|d| d.len()), Some(8));
        assert_eq!(ird.find("vazio"), Some(b"".as_slice()));
        assert_eq!(ird.find("nada"), None);
        let names: Vec<&str> = ird.iter().map(|m| m.name).collect();
        assert_eq!(names, ["init", "svcmgr", "vazio"]);
    }

    #[test]
    fn rejects_bad_images() {
        assert_eq!(Initrd::parse(b"NOPE").unwrap_err(), InitrdError::BadMagic);
        let mut img = pack(&[("a", b"xyz")]);
        img[8] = 9; // count maior que a tabela
        assert_eq!(Initrd::parse(&img).unwrap_err(), InitrdError::Truncated);
        let mut img = pack(&[("a", b"xyz")]);
        img[HEADER_SIZE + 40] = 0xff; // size absurdo
        assert_eq!(Initrd::parse(&img).unwrap_err(), InitrdError::BadEntry);
        let empty = pack(&[]);
        assert!(Initrd::parse(&empty).unwrap().is_empty());
    }
}

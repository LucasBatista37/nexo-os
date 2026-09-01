//! Formato de pacote de aplicativos (Plano §Fase 6: "definir formato de pacote e manifesto" e a
//! declaração de "permissões declarativas"). Um pacote `NEXOPKG1` carrega um **manifesto** textual
//! (linhas `chave=valor`: `name`, `version`, `entry` e `perms` — a lista de permissões que o app
//! **declara** precisar) e os arquivos do app, tudo protegido por CRC32. Assinatura/verificação e
//! instalação transacional vêm em blocos próprios, por cima deste formato.
//!
//! Layout (little-endian, sem alinhamento):
//! `"NEXOPKG1"` (8 B) · `versao: u32` (= 1) · `manifest_len: u32` · `file_count: u32` ·
//! `crc32: u32` (do payload) · payload = manifesto + por arquivo { `name_len: u16`, nome,
//! `data_len: u32`, dados }.

#![no_std]
#![forbid(unsafe_code)]

/// Assinatura do formato.
pub const MAGIC: &[u8; 8] = b"NEXOPKG1";
/// Versão do formato.
pub const VERSION: u32 = 1;
/// Tamanho do cabeçalho fixo.
pub const HEADER_LEN: usize = 8 + 4 + 4 + 4 + 4;
/// Limite do nome do aplicativo no manifesto.
pub const MAX_NAME: usize = 32;
/// Limite da versão (texto) no manifesto.
pub const MAX_VERSION_STR: usize = 16;
/// Limite do nome de arquivo/entry dentro do pacote.
pub const MAX_ENTRY: usize = 32;
/// Máximo de arquivos num pacote.
pub const MAX_FILES: u32 = 64;

/// Erros de decodificação.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PkgError {
    /// Assinatura errada.
    BadMagic,
    /// Versão de formato desconhecida.
    BadVersion,
    /// Bytes de menos (ou comprimentos inconsistentes).
    Truncated,
    /// CRC32 do payload não confere.
    BadCrc,
    /// Manifesto inválido (chave obrigatória ausente, valor longo demais, não-UTF-8).
    BadManifest,
    /// Limite excedido (arquivos demais).
    TooBig,
}

/// CRC32 (IEEE, bit a bit — o mesmo polinômio do NexoFS).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// O manifesto de um pacote: o que o app é e o que ele **declara** precisar.
#[derive(Clone, Copy, Debug)]
pub struct Manifest<'a> {
    /// Nome do aplicativo.
    pub name: &'a str,
    /// Versão do aplicativo (texto).
    pub version: &'a str,
    /// Arquivo executável dentro do pacote.
    pub entry: &'a str,
    /// Lista crua de permissões declaradas (separadas por vírgula; pode ser vazia).
    perms_raw: &'a str,
}

impl<'a> Manifest<'a> {
    /// Interpreta um manifesto textual (`chave=valor` por linha; `#` comenta; ordem livre).
    pub fn parse(bytes: &'a [u8]) -> Result<Manifest<'a>, PkgError> {
        let text = core::str::from_utf8(bytes).map_err(|_| PkgError::BadManifest)?;
        let (mut name, mut version, mut entry, mut perms) = (None, None, None, "");
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                return Err(PkgError::BadManifest);
            };
            match (k.trim(), v.trim()) {
                ("name", v) if !v.is_empty() && v.len() <= MAX_NAME => name = Some(v),
                ("version", v) if !v.is_empty() && v.len() <= MAX_VERSION_STR => version = Some(v),
                ("entry", v) if !v.is_empty() && v.len() <= MAX_ENTRY => entry = Some(v),
                ("perms", v) => perms = v,
                _ => return Err(PkgError::BadManifest),
            }
        }
        match (name, version, entry) {
            (Some(name), Some(version), Some(entry)) => Ok(Manifest {
                name,
                version,
                entry,
                perms_raw: perms,
            }),
            _ => Err(PkgError::BadManifest),
        }
    }

    /// Permissões declaradas, uma a uma.
    pub fn perms(&self) -> impl Iterator<Item = &'a str> {
        self.perms_raw
            .split(',')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
    }

    /// `true` se o app declara a permissão `perm`.
    pub fn declares(&self, perm: &str) -> bool {
        self.perms().any(|p| p == perm)
    }
}

/// Um pacote validado (assinatura, versão, CRC e limites conferidos no `parse`).
#[derive(Clone, Copy, Debug)]
pub struct Package<'a> {
    manifest: Manifest<'a>,
    files: &'a [u8],
    file_count: u32,
}

impl<'a> Package<'a> {
    /// Valida e abre um pacote.
    pub fn parse(bytes: &'a [u8]) -> Result<Package<'a>, PkgError> {
        if bytes.len() < HEADER_LEN {
            return Err(PkgError::Truncated);
        }
        if &bytes[..8] != MAGIC {
            return Err(PkgError::BadMagic);
        }
        let u32_at =
            |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        if u32_at(8) != VERSION {
            return Err(PkgError::BadVersion);
        }
        let manifest_len = u32_at(12) as usize;
        let file_count = u32_at(16);
        let crc = u32_at(20);
        if file_count > MAX_FILES {
            return Err(PkgError::TooBig);
        }
        let payload = &bytes[HEADER_LEN..];
        if manifest_len > payload.len() {
            return Err(PkgError::Truncated);
        }
        if crc32(payload) != crc {
            return Err(PkgError::BadCrc);
        }
        let manifest = Manifest::parse(&payload[..manifest_len])?;
        let files = &payload[manifest_len..];
        // valida a tabela de arquivos inteira já no parse
        let mut it = FileIter {
            rest: files,
            left: file_count,
        };
        for _ in 0..file_count {
            it.step()?;
        }
        if !it.rest.is_empty() {
            return Err(PkgError::Truncated); // bytes sobrando = inconsistência
        }
        Ok(Package {
            manifest,
            files,
            file_count,
        })
    }

    /// O manifesto.
    pub fn manifest(&self) -> Manifest<'a> {
        self.manifest
    }

    /// Quantos arquivos o pacote tem.
    pub fn file_count(&self) -> u32 {
        self.file_count
    }

    /// Itera `(nome, dados)` dos arquivos.
    pub fn files(&self) -> FileIter<'a> {
        FileIter {
            rest: self.files,
            left: self.file_count,
        }
    }

    /// Dados do arquivo `name`, se existir.
    pub fn file(&self, name: &str) -> Option<&'a [u8]> {
        let mut it = self.files();
        while let Ok(Some((n, d))) = it.step() {
            if n == name {
                return Some(d);
            }
        }
        None
    }
}

/// Iterador dos arquivos de um pacote.
pub struct FileIter<'a> {
    rest: &'a [u8],
    left: u32,
}

impl<'a> FileIter<'a> {
    /// Próximo arquivo (ou `Ok(None)` no fim; `Err` em truncamento/nome inválido).
    pub fn step(&mut self) -> Result<Option<(&'a str, &'a [u8])>, PkgError> {
        if self.left == 0 {
            return Ok(None);
        }
        let b = self.rest;
        if b.len() < 2 {
            return Err(PkgError::Truncated);
        }
        let name_len = u16::from_le_bytes([b[0], b[1]]) as usize;
        if name_len == 0 || name_len > MAX_ENTRY || b.len() < 2 + name_len + 4 {
            return Err(PkgError::Truncated);
        }
        let name = core::str::from_utf8(&b[2..2 + name_len]).map_err(|_| PkgError::BadManifest)?;
        let o = 2 + name_len;
        let data_len = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
        if b.len() < o + 4 + data_len {
            return Err(PkgError::Truncated);
        }
        let data = &b[o + 4..o + 4 + data_len];
        self.rest = &b[o + 4 + data_len..];
        self.left -= 1;
        Ok(Some((name, data)))
    }
}

impl<'a> Iterator for FileIter<'a> {
    type Item = (&'a str, &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        self.step().ok().flatten()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn build(manifest: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut payload = Vec::from(manifest.as_bytes());
        for (name, data) in files {
            payload.extend_from_slice(&(name.len() as u16).to_le_bytes());
            payload.extend_from_slice(name.as_bytes());
            payload.extend_from_slice(&(data.len() as u32).to_le_bytes());
            payload.extend_from_slice(data);
        }
        let mut out = Vec::from(&MAGIC[..]);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&(files.len() as u32).to_le_bytes());
        out.extend_from_slice(&crc32(&payload).to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    const MANIFEST: &str =
        "# app de teste\nname=calc\nversion=0.1.0\nentry=calc.elf\nperms=janelas, clipboard\n";

    #[test]
    fn round_trip_manifest_and_files() {
        let bytes = build(
            MANIFEST,
            &[("calc.elf", b"ELFDATA"), ("icone.px", b"\x01\x02")],
        );
        let pkg = Package::parse(&bytes).unwrap();
        let m = pkg.manifest();
        assert_eq!(m.name, "calc");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.entry, "calc.elf");
        let perms: Vec<_> = m.perms().collect();
        assert_eq!(perms, ["janelas", "clipboard"]);
        assert!(m.declares("clipboard"));
        assert!(!m.declares("rede"));
        assert_eq!(pkg.file_count(), 2);
        assert_eq!(pkg.file("calc.elf").unwrap(), b"ELFDATA");
        assert_eq!(pkg.file("icone.px").unwrap(), b"\x01\x02");
        assert!(pkg.file("nao-existe").is_none());
        let names: Vec<_> = pkg.files().map(|(n, _)| n).collect();
        assert_eq!(names, ["calc.elf", "icone.px"]);
    }

    #[test]
    fn rejects_bad_magic_version_crc() {
        let good = build(MANIFEST, &[("a", b"x")]);
        let mut bad = good.clone();
        bad[0] = b'X';
        assert_eq!(Package::parse(&bad).unwrap_err(), PkgError::BadMagic);
        let mut bad = good.clone();
        bad[8] = 9;
        assert_eq!(Package::parse(&bad).unwrap_err(), PkgError::BadVersion);
        let mut bad = good.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xff;
        assert_eq!(Package::parse(&bad).unwrap_err(), PkgError::BadCrc);
    }

    #[test]
    fn rejects_bad_manifests() {
        // sem entry
        let bytes = build("name=a\nversion=1\n", &[]);
        assert_eq!(Package::parse(&bytes).unwrap_err(), PkgError::BadManifest);
        // chave desconhecida
        let bytes = build("name=a\nversion=1\nentry=e\nmalicia=1\n", &[]);
        assert_eq!(Package::parse(&bytes).unwrap_err(), PkgError::BadManifest);
        // sem '='
        let bytes = build("name\n", &[]);
        assert_eq!(Package::parse(&bytes).unwrap_err(), PkgError::BadManifest);
    }

    #[test]
    fn fuzz_lite_truncations_never_panic() {
        let bytes = build(MANIFEST, &[("calc.elf", b"ELFDATA"), ("b", b"1234")]);
        for n in 0..bytes.len() {
            let _ = Package::parse(&bytes[..n]); // qualquer prefixo: erro limpo, sem panico
        }
        // e mutacoes de 1 byte
        for i in 0..bytes.len() {
            let mut m = bytes.clone();
            m[i] ^= 0x5a;
            let _ = Package::parse(&m);
        }
    }
}

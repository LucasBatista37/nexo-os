//! Instalação **transacional** de pacotes (Plano §Fase 6: "implementar instalação transacional").
//! O padrão é o do commit do NexoFS: todo o conteúdo vai para um **diretório versionado**
//! (`/apps/<nome>.v<N>/`) e só então o **ponteiro** `/apps/<nome>.cur` é gravado com `v<N>` — a
//! escrita do ponteiro é o commit. Um corte de energia antes do commit deixa a versão anterior
//! intacta (o diretório meio-escrito é re-preenchido na próxima tentativa, com os mesmos
//! caminhos); depois do commit, a versão nova está completa por construção.
//!
//! A biblioteca é agnóstica do transporte: o chamador dá um [`AppFs`] (no Nexo, um adaptador
//! sobre o protocolo `nexo.fs`; nos testes de host, um mock com falha injetada por operação).

#![no_std]
#![forbid(unsafe_code)]

use nexo_pkg::{Package, PkgError};

/// Erro de sistema de arquivos (opaco para a biblioteca).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FsErr;

/// As operações de que a instalação precisa.
pub trait AppFs {
    /// Cria o diretório (deve ser **idempotente**: Ok se já existe).
    fn mkdir(&mut self, path: &str) -> Result<(), FsErr>;
    /// Cria (ou trunca) `path` e grava `data` inteiro.
    fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), FsErr>;
    /// Lê `path` inteiro para `buf`; devolve o tamanho (erro se não existe/não cabe).
    fn read_file(&mut self, path: &str, buf: &mut [u8]) -> Result<usize, FsErr>;
    /// Remove um arquivo ou diretório vazio.
    fn unlink(&mut self, path: &str) -> Result<(), FsErr>;
}

/// Erros da instalação.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstError {
    /// Pacote inválido (nada é tocado no disco).
    Pkg(PkgError),
    /// Falha do sistema de arquivos (a versão corrente não muda).
    Fs,
    /// Caminho composto excedeu o limite interno.
    PathTooLong,
    /// O aplicativo está na lista de revogação (`/apps/.revoked`): não instala nem lança.
    Revoked,
}

impl From<PkgError> for InstError {
    fn from(e: PkgError) -> Self {
        InstError::Pkg(e)
    }
}

impl From<FsErr> for InstError {
    fn from(_: FsErr) -> Self {
        InstError::Fs
    }
}

/// Raiz das instalações.
pub const APPS_DIR: &str = "/apps";
/// Versões mantidas por aplicativo: a corrente e a anterior (rollback).
pub const KEEP_VERSIONS: u32 = 2;
/// Diretório do repositório local de pacotes: `/repo/<nome>.npk` (+ `indice.txt` informativo).
pub const REPO_DIR: &str = "/repo";
/// Lista de revogação: um nome de aplicativo por linha. Consultada por `install` e pelos
/// lançadores ([`is_revoked`]); alimentada pelo processo de revisão ([`revoke`]).
pub const REVOKED_PATH: &str = "/apps/.revoked";
/// Tamanho máximo da lista de revogação.
pub const REVOKED_MAX: usize = 1024;
/// Tamanho máximo de um caminho composto.
pub const MAX_PATH: usize = 96;

/// Monta caminhos sem alocação.
struct PathBuf {
    buf: [u8; MAX_PATH],
    len: usize,
}

impl PathBuf {
    fn new() -> PathBuf {
        PathBuf {
            buf: [0; MAX_PATH],
            len: 0,
        }
    }

    fn push(&mut self, s: &str) -> Result<(), InstError> {
        let b = s.as_bytes();
        if self.len + b.len() > MAX_PATH {
            return Err(InstError::PathTooLong);
        }
        self.buf[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
        Ok(())
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

/// Formata `v<N>` num buffer pequeno.
fn ver_str(v: u32, out: &mut [u8; 12]) -> &str {
    out[0] = b'v';
    let mut n = v;
    let mut tmp = [0u8; 10];
    let mut i = 0;
    if n == 0 {
        tmp[0] = b'0';
        i = 1;
    }
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for k in 0..i {
        out[1 + k] = tmp[i - 1 - k];
    }
    core::str::from_utf8(&out[..1 + i]).unwrap_or("v0")
}

/// Versão corrente instalada de `name` (lê `/apps/<name>.cur`), se houver.
pub fn current_version(fs: &mut impl AppFs, name: &str) -> Option<u32> {
    let mut p = PathBuf::new();
    p.push(APPS_DIR).ok()?;
    p.push("/").ok()?;
    p.push(name).ok()?;
    p.push(".cur").ok()?;
    let mut buf = [0u8; 12];
    let n = fs.read_file(p.as_str(), &mut buf).ok()?;
    let s = core::str::from_utf8(&buf[..n]).ok()?;
    s.strip_prefix('v')?.trim().parse().ok()
}

/// Monta em `out` o caminho de `file` dentro da versão `v` de `name`.
pub fn versioned_path<'a>(
    name: &str,
    v: u32,
    file: &str,
    out: &'a mut [u8; MAX_PATH],
) -> Result<&'a str, InstError> {
    let mut p = PathBuf::new();
    p.push(APPS_DIR)?;
    p.push("/")?;
    p.push(name)?;
    p.push(".")?;
    let mut vb = [0u8; 12];
    p.push(ver_str(v, &mut vb))?;
    if !file.is_empty() {
        p.push("/")?;
        p.push(file)?;
    }
    out[..p.len].copy_from_slice(&p.buf[..p.len]);
    let len = p.len;
    Ok(core::str::from_utf8(&out[..len]).unwrap_or(""))
}

/// `true` se `name` está na lista de revogação.
pub fn is_revoked(fs: &mut impl AppFs, name: &str) -> bool {
    let mut buf = [0u8; REVOKED_MAX];
    let Ok(n) = fs.read_file(REVOKED_PATH, &mut buf) else {
        return false; // sem lista = nada revogado
    };
    let Ok(text) = core::str::from_utf8(&buf[..n]) else {
        return true; // lista corrompida: falha fechada (nega tudo)
    };
    text.lines().any(|l| l.trim() == name)
}

/// Acrescenta `name` à lista de revogação (idempotente). A partir daí, `install` recusa o app e
/// lançadores devem consultar [`is_revoked`] antes de executar.
pub fn revoke(fs: &mut impl AppFs, name: &str) -> Result<(), InstError> {
    if is_revoked(fs, name) {
        return Ok(());
    }
    let mut buf = [0u8; REVOKED_MAX];
    let n = fs.read_file(REVOKED_PATH, &mut buf).unwrap_or(0);
    if n + name.len() + 1 > REVOKED_MAX {
        return Err(InstError::Fs);
    }
    buf[n..n + name.len()].copy_from_slice(name.as_bytes());
    buf[n + name.len()] = b'\n';
    fs.mkdir(APPS_DIR)?;
    fs.write_file(REVOKED_PATH, &buf[..n + name.len() + 1])?;
    Ok(())
}

/// Instala (ou atualiza) o pacote `pkg`; devolve a versão instalada. Transacional: o ponteiro
/// `.cur` só é gravado depois de todo o conteúdo — antes disso, a versão corrente não muda.
pub fn install(fs: &mut impl AppFs, pkg: &[u8]) -> Result<u32, InstError> {
    let p = Package::parse(pkg)?; // valida tudo antes de tocar no disco
    let name = p.manifest().name;
    if is_revoked(fs, name) {
        return Err(InstError::Revoked); // revisão/revogação: app revogado não instala
    }
    let next = current_version(fs, name).map_or(1, |v| v + 1);
    fs.mkdir(APPS_DIR)?;
    let mut pb = [0u8; MAX_PATH];
    let dir = versioned_path(name, next, "", &mut pb)?;
    fs.mkdir(dir)?;
    let mut pb2 = [0u8; MAX_PATH];
    fs.write_file(
        versioned_path(name, next, "manifest.txt", &mut pb2)?,
        p.manifest_bytes(),
    )?;
    let mut it = p.files();
    let mut listing = [0u8; 1024];
    let mut ll = 0usize;
    while let Ok(Some((fname, data))) = it.step() {
        let mut pb3 = [0u8; MAX_PATH];
        fs.write_file(versioned_path(name, next, fname, &mut pb3)?, data)?;
        if ll + fname.len() < listing.len() {
            listing[ll..ll + fname.len()].copy_from_slice(fname.as_bytes());
            listing[ll + fname.len()] = b'\n';
            ll += fname.len() + 1;
        }
    }
    // files.txt: o que a coleta ([`gc`]) precisa para esvaziar esta versão no futuro
    let mut pb4 = [0u8; MAX_PATH];
    fs.write_file(
        versioned_path(name, next, "files.txt", &mut pb4)?,
        &listing[..ll],
    )?;
    // COMMIT: o ponteiro por último
    let mut cur = PathBuf::new();
    cur.push(APPS_DIR)?;
    cur.push("/")?;
    cur.push(name)?;
    cur.push(".cur")?;
    let mut vb = [0u8; 12];
    let v = ver_str(next, &mut vb);
    fs.write_file(cur.as_str(), v.as_bytes())?;
    // pós-commit: coleta versões antigas (best-effort — falha aqui não desfaz a instalação)
    let _ = gc(fs, name);
    Ok(next)
}

/// Instala `name` a partir do repositório local (`/repo/<name>.npk`), com toda a validação e
/// as regras de sempre (revogação, transação, coleta). `buf` é o espaço de leitura do pacote
/// (do chamador, para o tamanho ficar sob controle de quem instala).
pub fn install_from_repo(
    fs: &mut impl AppFs,
    name: &str,
    buf: &mut [u8],
) -> Result<u32, InstError> {
    let mut pb = PathBuf::new();
    pb.push(REPO_DIR)?;
    pb.push("/")?;
    pb.push(name)?;
    pb.push(".npk")?;
    let n = fs.read_file(pb.as_str(), buf)?;
    install(fs, &buf[..n])
}

/// Coleta versões antigas de `name`: remove toda `vN` com `N <= corrente - KEEP_VERSIONS`.
/// Best-effort — cada versão falha ou some por inteiro; a próxima instalação tenta de novo.
/// Versões gravadas antes do `files.txt` existir não são tocadas. Devolve quantas removeu.
pub fn gc(fs: &mut impl AppFs, name: &str) -> u32 {
    let Some(cur) = current_version(fs, name) else {
        return 0;
    };
    if cur <= KEEP_VERSIONS {
        return 0;
    }
    let mut removed = 0;
    for v in 1..=cur - KEEP_VERSIONS {
        if gc_version(fs, name, v).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn gc_version(fs: &mut impl AppFs, name: &str, v: u32) -> Result<(), InstError> {
    let mut pb = [0u8; MAX_PATH];
    let mut list = [0u8; 1024];
    let n = fs.read_file(versioned_path(name, v, "files.txt", &mut pb)?, &mut list)?;
    let text = core::str::from_utf8(&list[..n]).map_err(|_| InstError::Fs)?;
    for fname in text.lines() {
        let fname = fname.trim();
        if fname.is_empty() {
            continue;
        }
        let mut p = [0u8; MAX_PATH];
        let _ = fs.unlink(versioned_path(name, v, fname, &mut p)?);
    }
    for extra in ["files.txt", "manifest.txt"] {
        let mut p = [0u8; MAX_PATH];
        let _ = fs.unlink(versioned_path(name, v, extra, &mut p)?);
    }
    let mut p = [0u8; MAX_PATH];
    fs.unlink(versioned_path(name, v, "", &mut p)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::string::{String, ToString};
    use std::vec::Vec;

    /// Mock com falha injetada: cada operação MUTANTE conta; a partir de `fail_after`, falha.
    struct Mock {
        files: BTreeMap<String, Vec<u8>>,
        dirs: BTreeSet<String>,
        ops: usize,
        fail_after: usize,
    }

    impl Mock {
        fn new() -> Mock {
            Mock {
                files: BTreeMap::new(),
                dirs: BTreeSet::new(),
                ops: 0,
                fail_after: usize::MAX,
            }
        }

        fn tick(&mut self) -> Result<(), FsErr> {
            self.ops += 1;
            if self.ops > self.fail_after {
                Err(FsErr)
            } else {
                Ok(())
            }
        }
    }

    impl AppFs for Mock {
        fn mkdir(&mut self, path: &str) -> Result<(), FsErr> {
            self.tick()?;
            self.dirs.insert(path.to_string());
            Ok(())
        }
        fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), FsErr> {
            self.tick()?;
            self.files.insert(path.to_string(), Vec::from(data));
            Ok(())
        }
        fn read_file(&mut self, path: &str, buf: &mut [u8]) -> Result<usize, FsErr> {
            let d = self.files.get(path).ok_or(FsErr)?;
            if d.len() > buf.len() {
                return Err(FsErr);
            }
            buf[..d.len()].copy_from_slice(d);
            Ok(d.len())
        }
        fn unlink(&mut self, path: &str) -> Result<(), FsErr> {
            self.tick()?;
            if self.files.remove(path).is_some() {
                return Ok(());
            }
            if self.dirs.contains(path) {
                let mut prefix = String::from(path);
                prefix.push('/');
                if self.files.keys().any(|k| k.starts_with(&prefix)) {
                    return Err(FsErr); // não vazio, como no NexoFS
                }
                self.dirs.remove(path);
                return Ok(());
            }
            Err(FsErr)
        }
    }

    fn pkg(version: &str, payload: &[u8]) -> Vec<u8> {
        let manifest =
            std::format!("name=calc\nversion={version}\nentry=calc.elf\nperms=janelas\n");
        let mut body = Vec::from(manifest.as_bytes());
        for (name, data) in [("calc.elf", payload), ("leia.txt", b"ola".as_slice())] {
            body.extend_from_slice(&(name.len() as u16).to_le_bytes());
            body.extend_from_slice(name.as_bytes());
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
        }
        let mut out = Vec::from(&nexo_pkg::MAGIC[..]);
        out.extend_from_slice(&nexo_pkg::VERSION.to_le_bytes());
        out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&nexo_pkg::crc32(&body).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn install_and_update_flip_the_pointer() {
        let mut fs = Mock::new();
        assert_eq!(install(&mut fs, &pkg("1", b"AAA")).unwrap(), 1);
        assert_eq!(current_version(&mut fs, "calc"), Some(1));
        assert_eq!(fs.files["/apps/calc.v1/calc.elf"], b"AAA");
        assert!(fs.files.contains_key("/apps/calc.v1/manifest.txt"));
        // atualização: v2, ponteiro vira, v1 fica intacta
        assert_eq!(install(&mut fs, &pkg("2", b"BBB")).unwrap(), 2);
        assert_eq!(current_version(&mut fs, "calc"), Some(2));
        assert_eq!(fs.files["/apps/calc.v2/calc.elf"], b"BBB");
        assert_eq!(fs.files["/apps/calc.v1/calc.elf"], b"AAA");
    }

    #[test]
    fn bad_package_touches_nothing() {
        let mut fs = Mock::new();
        let mut bad = pkg("1", b"AAA");
        bad[0] = b'X';
        assert!(matches!(install(&mut fs, &bad), Err(InstError::Pkg(_))));
        assert!(fs.files.is_empty() && fs.dirs.is_empty());
    }

    #[test]
    fn power_cut_at_every_op_preserves_current_version() {
        // instala a v1 inteira; depois, para cada K, corta a atualização para a v2 após K ops
        for k in 0..8 {
            let mut fs = Mock::new();
            install(&mut fs, &pkg("1", b"AAA")).unwrap();
            let ops0 = fs.ops;
            fs.fail_after = ops0 + k;
            let r = install(&mut fs, &pkg("2", b"BBB"));
            if r.is_ok() {
                // ops suficientes: commit aconteceu e a v2 esta completa
                assert_eq!(current_version(&mut fs, "calc"), Some(2));
                assert_eq!(fs.files["/apps/calc.v2/calc.elf"], b"BBB");
            } else {
                // corte antes do commit: a v1 segue corrente e intacta
                assert_eq!(current_version(&mut fs, "calc"), Some(1));
                assert_eq!(fs.files["/apps/calc.v1/calc.elf"], b"AAA");
                // e uma nova tentativa completa funciona (re-preenche a v2)
                fs.fail_after = usize::MAX;
                assert_eq!(install(&mut fs, &pkg("2", b"BBB")).unwrap(), 2);
                assert_eq!(current_version(&mut fs, "calc"), Some(2));
                assert_eq!(fs.files["/apps/calc.v2/calc.elf"], b"BBB");
            }
        }
    }

    #[test]
    fn installs_from_local_repo() {
        let mut fs = Mock::new();
        let p = pkg("1", b"AAA");
        fs.dirs.insert("/repo".into());
        fs.files.insert("/repo/calc.npk".into(), p.clone());
        let mut buf = std::vec![0u8; p.len()];
        assert_eq!(install_from_repo(&mut fs, "calc", &mut buf).unwrap(), 1);
        assert_eq!(current_version(&mut fs, "calc"), Some(1));
        // ausente no repositório: erro de fs, nada instalado
        assert!(install_from_repo(&mut fs, "nao-existe", &mut buf).is_err());
        // revogado: recusado pelo MESMO caminho
        revoke(&mut fs, "calc").unwrap();
        assert_eq!(
            install_from_repo(&mut fs, "calc", &mut buf).unwrap_err(),
            InstError::Revoked
        );
    }

    #[test]
    fn gc_keeps_current_and_previous_only() {
        let mut fs = Mock::new();
        for v in 1..=4 {
            let vs = std::format!("{v}");
            install(&mut fs, &pkg(&vs, b"AAA")).unwrap();
        }
        assert_eq!(current_version(&mut fs, "calc"), Some(4));
        assert!(fs.files.contains_key("/apps/calc.v4/calc.elf"));
        assert!(fs.files.contains_key("/apps/calc.v3/calc.elf"));
        // v1 e v2 coletadas por inteiro: arquivos, manifesto, files.txt e o diretório
        for v in [1u32, 2] {
            let prefix = std::format!("/apps/calc.v{v}/");
            assert!(!fs.files.keys().any(|k| k.starts_with(&prefix)), "{prefix}");
            assert!(!fs.dirs.contains(std::format!("/apps/calc.v{v}").as_str()));
        }
    }

    #[test]
    fn revoked_app_neither_installs_nor_reads_as_ok() {
        let mut fs = Mock::new();
        install(&mut fs, &pkg("1", b"AAA")).unwrap();
        assert!(!is_revoked(&mut fs, "calc"));
        revoke(&mut fs, "calc").unwrap();
        revoke(&mut fs, "calc").unwrap(); // idempotente
        assert!(is_revoked(&mut fs, "calc"));
        assert!(matches!(
            install(&mut fs, &pkg("2", b"BBB")),
            Err(InstError::Revoked)
        ));
        // outro app segue instalável; lista com várias linhas funciona
        revoke(&mut fs, "outro").unwrap();
        assert!(is_revoked(&mut fs, "outro"));
        assert!(!is_revoked(&mut fs, "livre"));
    }

    #[test]
    fn versioned_path_builds_and_limits() {
        let mut b = [0u8; MAX_PATH];
        assert_eq!(
            versioned_path("calc", 3, "calc.elf", &mut b).unwrap(),
            "/apps/calc.v3/calc.elf"
        );
        let long = core::str::from_utf8(&[b'a'; 90]).unwrap();
        let mut b2 = [0u8; MAX_PATH];
        assert!(matches!(
            versioned_path(long, 1, "x", &mut b2),
            Err(InstError::PathTooLong)
        ));
    }
}

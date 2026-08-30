//! Símbolos para diagnóstico: localiza `.symtab`/`.strtab` em uma imagem ELF64
//! e resolve endereços em nomes (com *demangling* do esquema legado do Rust).
//!
//! Usado pelo kernel para *symbolication* em panics e exceções, a partir da
//! cópia do próprio arquivo ELF entregue pelo loader.
#![no_std]
#![deny(unsafe_code)]

use core::fmt;

mod v0;
pub use v0::{DemangleError, demangle_v0, is_valid_v0};

const SHT_SYMTAB: u32 = 2;
const STT_FUNC: u8 = 2;
const STT_OBJECT: u8 = 1;
const STT_NOTYPE: u8 = 0;

/// Tabela de símbolos de uma imagem ELF64.
#[derive(Clone, Copy)]
pub struct SymbolTable<'a> {
    symtab: &'a [u8],
    strtab: &'a [u8],
}

/// Símbolo resolvido.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Symbol<'a> {
    /// Nome cru (possivelmente *mangled*).
    pub name: &'a str,
    /// Endereço inicial.
    pub start: u64,
    /// Tamanho em bytes (0 se desconhecido).
    pub size: u64,
}

impl<'a> Symbol<'a> {
    /// Nome legível.
    pub fn demangled(&self) -> Demangled<'a> {
        Demangled(self.name)
    }
}

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(off..off + 8)?.try_into().ok()?))
}

impl<'a> SymbolTable<'a> {
    /// Localiza a tabela de símbolos em `elf`. Devolve `None` se o arquivo não
    /// é ELF64 little-endian ou não contém `.symtab`.
    pub fn parse(elf: &'a [u8]) -> Option<Self> {
        if elf.get(0..4)? != b"\x7fELF" || elf.get(4)? != &2 || elf.get(5)? != &1 {
            return None;
        }
        let shoff = u64_at(elf, 0x28)? as usize;
        let shentsize = u16_at(elf, 0x3a)? as usize;
        let shnum = u16_at(elf, 0x3c)? as usize;
        if shentsize < 64 {
            return None;
        }
        for i in 0..shnum {
            let sh = shoff + i * shentsize;
            if u32_at(elf, sh + 4)? != SHT_SYMTAB {
                continue;
            }
            let off = u64_at(elf, sh + 24)? as usize;
            let size = u64_at(elf, sh + 32)? as usize;
            let link = u32_at(elf, sh + 40)? as usize;
            let symtab = elf.get(off..off.checked_add(size)?)?;
            let lsh = shoff + link * shentsize;
            let loff = u64_at(elf, lsh + 24)? as usize;
            let lsize = u64_at(elf, lsh + 32)? as usize;
            let strtab = elf.get(loff..loff.checked_add(lsize)?)?;
            return Some(SymbolTable { symtab, strtab });
        }
        None
    }

    /// Número de entradas.
    pub fn len(&self) -> usize {
        self.symtab.len() / 24
    }

    /// `true` se não há símbolos.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn entry(&self, i: usize) -> Option<(u32, u8, u64, u64)> {
        let b = self.symtab.get(i * 24..i * 24 + 24)?;
        Some((u32_at(b, 0)?, b[4] & 0xf, u64_at(b, 8)?, u64_at(b, 16)?))
    }

    fn name_at(&self, off: u32) -> Option<&'a str> {
        let s = self.strtab.get(off as usize..)?;
        let end = s.iter().position(|&c| c == 0)?;
        core::str::from_utf8(&s[..end]).ok()
    }

    /// Itera símbolos de função/objeto com nome.
    pub fn iter(&self) -> impl Iterator<Item = Symbol<'a>> + '_ {
        (0..self.len()).filter_map(move |i| {
            let (name, ty, value, size) = self.entry(i)?;
            if name == 0 || !matches!(ty, STT_FUNC | STT_OBJECT | STT_NOTYPE) {
                return None;
            }
            Some(Symbol {
                name: self.name_at(name)?,
                start: value,
                size,
            })
        })
    }

    /// Resolve `addr`: prefere um símbolo cujo intervalo `[start, start+size)`
    /// contenha `addr`; senão, o símbolo de função mais próximo abaixo.
    pub fn lookup(&self, addr: u64) -> Option<Symbol<'a>> {
        let mut best: Option<Symbol<'a>> = None;
        for i in 0..self.len() {
            let Some((name, ty, value, size)) = self.entry(i) else {
                continue;
            };
            if name == 0 || value == 0 || value > addr {
                continue;
            }
            if size != 0 && addr < value + size && ty == STT_FUNC {
                return Some(Symbol {
                    name: self.name_at(name)?,
                    start: value,
                    size,
                });
            }
            if ty != STT_FUNC {
                continue;
            }
            if best.is_none_or(|b| value > b.start) {
                best = Some(Symbol {
                    name: self.name_at(name)?,
                    start: value,
                    size,
                });
            }
        }
        best
    }

    /// Procura um símbolo pelo nome exato.
    pub fn find(&self, name: &str) -> Option<Symbol<'a>> {
        self.iter().find(|s| s.name == name)
    }
}

/// Exibe um nome de símbolo Rust de forma legível (esquemas legado `_ZN…E`
/// e v0 `_R…`). Nomes C e entradas não reconhecidas são exibidos sem alteração.
#[derive(Clone, Copy, Debug)]
pub struct Demangled<'a>(pub &'a str);

impl fmt::Display for Demangled<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.0;
        if s.starts_with("_R") {
            return if is_valid_v0(s) {
                demangle_v0(s, f).map_err(|_| fmt::Error)
            } else {
                f.write_str(s)
            };
        }
        let Some(rest) = s.strip_prefix("_ZN") else {
            return f.write_str(s);
        };
        // Sufixos como ".llvm.1234" são descartados.
        let rest = rest.split(".llvm.").next().unwrap_or(rest);
        let mut b = rest.as_bytes();
        let mut first = true;
        loop {
            if b.first() == Some(&b'E') {
                break;
            }
            let mut len = 0usize;
            let mut digits = 0;
            while let Some(&c) = b.get(digits) {
                if c.is_ascii_digit() {
                    len = len * 10 + (c - b'0') as usize;
                    digits += 1;
                } else {
                    break;
                }
            }
            if digits == 0 || len == 0 || b.len() < digits + len {
                return f.write_str(s); // malformado: mostra cru
            }
            let ident = &rest[rest.len() - b.len() + digits..rest.len() - b.len() + digits + len];
            b = &b[digits + len..];
            if is_hash(ident) && b.first() == Some(&b'E') {
                break;
            }
            if !first {
                f.write_str("::")?;
            }
            first = false;
            write_ident(f, ident)?;
        }
        Ok(())
    }
}

fn is_hash(ident: &str) -> bool {
    ident.len() == 17 && ident.starts_with('h') && ident[1..].bytes().all(|c| c.is_ascii_hexdigit())
}

fn write_ident(f: &mut fmt::Formatter<'_>, ident: &str) -> fmt::Result {
    // O mangler legado prefixa `_` quando o identificador começaria com `$`.
    let mut rest = ident.strip_prefix("_$").map_or(ident, |_| &ident[1..]);
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("..") {
            f.write_str("::")?;
            rest = after;
            continue;
        }
        if let Some(after) = rest.strip_prefix('$')
            && let Some(end) = after.find('$')
        {
            let code = &after[..end];
            let rep = match code {
                "LT" => Some('<'),
                "GT" => Some('>'),
                "LP" => Some('('),
                "RP" => Some(')'),
                "C" => Some(','),
                "SP" => Some('@'),
                "BP" => Some('*'),
                "RF" => Some('&'),
                _ => code
                    .strip_prefix('u')
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .and_then(char::from_u32),
            };
            if let Some(c) = rep {
                write!(f, "{c}")?;
                rest = &after[end + 1..];
                continue;
            }
        }
        let mut chars = rest.chars();
        let c = chars.next().unwrap();
        write!(f, "{c}")?;
        rest = chars.as_str();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::format;
    use std::string::String;
    use std::vec::Vec;

    fn dm(s: &str) -> String {
        format!("{}", Demangled(s))
    }

    #[test]
    fn demangles_legacy_names() {
        assert_eq!(
            dm("_ZN4core9panicking5panic17h1234567890abcdefE"),
            "core::panicking::panic"
        );
        assert_eq!(
            dm("_ZN11nexo_kernel5kmain17h0000000000000000E"),
            "nexo_kernel::kmain"
        );
        assert_eq!(
            dm("_ZN5alloc7raw_vec19RawVec$LT$T$C$A$GT$4grow17habcdefabcdefabcdE"),
            "alloc::raw_vec::RawVec<T,A>::grow"
        );
        assert_eq!(
            dm("_ZN3foo3bar28_$u7b$$u7b$closure$u7d$$u7d$17hfedcbafedcbafedcE"),
            "foo::bar::{{closure}}"
        );
        assert_eq!(dm("_ZN1a4b..c17h0000000000000000E.llvm.99"), "a::b::c");
        assert_eq!(dm("_start"), "_start");
        assert_eq!(dm("_RNvCs1234_7mycrate4main"), "mycrate::main");
        assert_eq!(dm("_RNvC3foo"), "_RNvC3foo"); // v0 malformado: cru
        assert_eq!(dm("_ZN99xE"), "_ZN99xE"); // malformado
    }

    /// Constrói um ELF64 mínimo com .symtab/.strtab.
    fn synthetic_elf(syms: &[(&str, u8, u64, u64)]) -> Vec<u8> {
        let mut strtab = std::vec![0u8];
        let mut symtab = std::vec![0u8; 24]; // símbolo nulo
        for (name, ty, value, size) in syms {
            let off = strtab.len() as u32;
            strtab.extend_from_slice(name.as_bytes());
            strtab.push(0);
            let mut e = [0u8; 24];
            e[0..4].copy_from_slice(&off.to_le_bytes());
            e[4] = *ty;
            e[8..16].copy_from_slice(&value.to_le_bytes());
            e[16..24].copy_from_slice(&size.to_le_bytes());
            symtab.extend_from_slice(&e);
        }
        let mut elf = std::vec![0u8; 64];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        let symoff = elf.len();
        elf.extend_from_slice(&symtab);
        let stroff = elf.len();
        elf.extend_from_slice(&strtab);
        let shoff = elf.len();
        // seção 0 nula, 1 symtab, 2 strtab
        elf.extend_from_slice(&[0u8; 64]);
        let mut sh = [0u8; 64];
        sh[4..8].copy_from_slice(&SHT_SYMTAB.to_le_bytes());
        sh[24..32].copy_from_slice(&(symoff as u64).to_le_bytes());
        sh[32..40].copy_from_slice(&(symtab.len() as u64).to_le_bytes());
        sh[40..44].copy_from_slice(&2u32.to_le_bytes());
        elf.extend_from_slice(&sh);
        let mut sh = [0u8; 64];
        sh[4..8].copy_from_slice(&3u32.to_le_bytes());
        sh[24..32].copy_from_slice(&(stroff as u64).to_le_bytes());
        sh[32..40].copy_from_slice(&(strtab.len() as u64).to_le_bytes());
        elf.extend_from_slice(&sh);
        elf[0x28..0x30].copy_from_slice(&(shoff as u64).to_le_bytes());
        elf[0x3a..0x3c].copy_from_slice(&64u16.to_le_bytes());
        elf[0x3c..0x3e].copy_from_slice(&3u16.to_le_bytes());
        elf
    }

    #[test]
    fn parses_and_looks_up() {
        let elf = synthetic_elf(&[
            ("_ZN1k5kmain17h0000000000000000E", STT_FUNC, 0x1000, 0x100),
            ("_ZN1k5panic17h0000000000000000E", STT_FUNC, 0x1100, 0x40),
            ("_ZN1k4data17h0000000000000000E", STT_OBJECT, 0x2000, 8),
            ("nosize", STT_FUNC, 0x3000, 0),
        ]);
        let t = SymbolTable::parse(&elf).expect("parse");
        assert_eq!(t.len(), 5);
        let s = t.lookup(0x1050).unwrap();
        assert_eq!(format!("{}", s.demangled()), "k::kmain");
        assert_eq!(s.start, 0x1000);
        assert_eq!(t.lookup(0x1100).unwrap().start, 0x1100);
        assert_eq!(t.lookup(0x1200).unwrap().start, 0x1100); // mais próximo abaixo
        assert_eq!(t.lookup(0x3fff).unwrap().name, "nosize");
        assert!(t.lookup(0xfff).is_none());
        assert_eq!(t.find("nosize").unwrap().start, 0x3000);
        assert!(SymbolTable::parse(b"not an elf").is_none());
    }
}

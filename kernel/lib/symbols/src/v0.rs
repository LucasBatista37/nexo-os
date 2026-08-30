//! Demangler (best-effort) do esquema **v0** do Rust (RFC 2603).
//!
//! Cobre caminhos, impls inerentes/de trait, closures, argumentos genéricos,
//! tipos básicos/compostos, constantes simples e *backrefs*. Qualquer entrada
//! fora da gramática suportada resulta em `Err`, e o chamador exibe o nome cru.
//! Não aloca: a saída vai direto para um `fmt::Write`.

use core::fmt;

const MAX_DEPTH: u32 = 64;

struct Parser<'a, 'w> {
    b: &'a [u8],
    pos: usize,
    depth: u32,
    silent: u32,
    out: &'w mut dyn fmt::Write,
}

type R = Result<(), ()>;

impl<'a, 'w> Parser<'a, 'w> {
    fn w(&mut self, s: &str) -> R {
        if self.silent == 0 {
            self.out.write_str(s).map_err(|_| ())?;
        }
        Ok(())
    }

    fn wf(&mut self, args: fmt::Arguments<'_>) -> R {
        if self.silent == 0 {
            self.out.write_fmt(args).map_err(|_| ())?;
        }
        Ok(())
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn next(&mut self) -> Result<u8, ()> {
        let c = self.peek().ok_or(())?;
        self.pos += 1;
        Ok(c)
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn enter(&mut self) -> R {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err(())
        } else {
            Ok(())
        }
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// `base-62-number`: vazio = 0; senão valor codificado + 1.
    fn base62(&mut self) -> Result<u64, ()> {
        let mut n: u64 = 0;
        let mut any = false;
        loop {
            let c = self.next()?;
            let d = match c {
                b'_' => return Ok(if any { n.checked_add(1).ok_or(())? } else { 0 }),
                b'0'..=b'9' => c - b'0',
                b'a'..=b'z' => c - b'a' + 10,
                b'A'..=b'Z' => c - b'A' + 36,
                _ => return Err(()),
            };
            n = n
                .checked_mul(62)
                .ok_or(())?
                .checked_add(d as u64)
                .ok_or(())?;
            any = true;
        }
    }

    fn decimal(&mut self) -> Result<usize, ()> {
        let mut n: usize = 0;
        let mut any = false;
        while let Some(c) = self.peek() {
            if !c.is_ascii_digit() {
                break;
            }
            n = n
                .checked_mul(10)
                .ok_or(())?
                .checked_add((c - b'0') as usize)
                .ok_or(())?;
            self.pos += 1;
            any = true;
        }
        if any { Ok(n) } else { Err(()) }
    }

    /// `identifier`: devolve (texto, disambiguator).
    fn ident(&mut self) -> Result<(&'a str, u64), ()> {
        let dis = if self.eat(b's') {
            self.base62()? + 1
        } else {
            0
        };
        let punycode = self.eat(b'u');
        let len = self.decimal()?;
        self.eat(b'_');
        let bytes = self
            .b
            .get(self.pos..self.pos.checked_add(len).ok_or(())?)
            .ok_or(())?;
        self.pos += len;
        let s = core::str::from_utf8(bytes).map_err(|_| ())?;
        let _ = punycode; // exibido cru
        Ok((s, dis))
    }

    fn backref<F: FnOnce(&mut Self) -> R>(&mut self, f: F) -> R {
        let off = self.base62()? as usize;
        if off >= self.pos.saturating_sub(1) {
            return Err(());
        }
        let saved = self.pos;
        self.pos = off;
        self.enter()?;
        let r = f(self);
        self.leave();
        self.pos = saved;
        r
    }

    fn path(&mut self) -> R {
        self.enter()?;
        let r = self.path_inner();
        self.leave();
        r
    }

    fn path_inner(&mut self) -> R {
        match self.next()? {
            b'C' => {
                let (id, _) = self.ident()?;
                self.w(id)
            }
            b'M' => {
                self.impl_path()?;
                self.w("<")?;
                self.ty()?;
                self.w(">")
            }
            b'X' => {
                self.impl_path()?;
                self.w("<")?;
                self.ty()?;
                self.w(" as ")?;
                self.path()?;
                self.w(">")
            }
            b'Y' => {
                self.w("<")?;
                self.ty()?;
                self.w(" as ")?;
                self.path()?;
                self.w(">")
            }
            b'N' => {
                let ns = self.next()?;
                self.path()?;
                let (id, dis) = self.ident()?;
                match ns {
                    b'C' => self.wf(format_args!("::{{closure#{dis}}}")),
                    b'S' => self.wf(format_args!("::{{shim:{id}#{dis}}}")),
                    c if c.is_ascii_uppercase() => {
                        self.wf(format_args!("::{{{}:{id}#{dis}}}", c as char))
                    }
                    c if c.is_ascii_lowercase() => {
                        if id.is_empty() {
                            Ok(())
                        } else {
                            self.w("::")?;
                            self.w(id)
                        }
                    }
                    _ => Err(()),
                }
            }
            b'I' => {
                self.path()?;
                self.w("<")?;
                let mut first = true;
                while !self.eat(b'E') {
                    if !first {
                        self.w(", ")?;
                    }
                    first = false;
                    self.generic_arg()?;
                }
                self.w(">")
            }
            b'B' => self.backref(|p| p.path()),
            _ => Err(()),
        }
    }

    fn impl_path(&mut self) -> R {
        if self.eat(b's') {
            self.base62()?;
        }
        self.silent += 1;
        let r = self.path();
        self.silent -= 1;
        r
    }

    fn generic_arg(&mut self) -> R {
        match self.peek() {
            Some(b'L') => {
                self.pos += 1;
                self.base62()?;
                self.w("'_")
            }
            Some(b'K') => {
                self.pos += 1;
                self.const_()
            }
            _ => self.ty(),
        }
    }

    fn ty(&mut self) -> R {
        self.enter()?;
        let r = self.ty_inner();
        self.leave();
        r
    }

    fn ty_inner(&mut self) -> R {
        let c = self.next()?;
        let basic = match c {
            b'a' => "i8",
            b'b' => "bool",
            b'c' => "char",
            b'd' => "f64",
            b'e' => "str",
            b'f' => "f32",
            b'h' => "u8",
            b'i' => "isize",
            b'j' => "usize",
            b'l' => "i32",
            b'm' => "u32",
            b'n' => "i128",
            b'o' => "u128",
            b's' => "i16",
            b't' => "u16",
            b'u' => "()",
            b'v' => "...",
            b'x' => "i64",
            b'y' => "u64",
            b'z' => "!",
            b'p' => "_",
            _ => "",
        };
        if !basic.is_empty() {
            return self.w(basic);
        }
        match c {
            b'R' | b'Q' => {
                if self.eat(b'L') {
                    self.base62()?;
                }
                self.w(if c == b'R' { "&" } else { "&mut " })?;
                self.ty()
            }
            b'P' => {
                self.w("*const ")?;
                self.ty()
            }
            b'O' => {
                self.w("*mut ")?;
                self.ty()
            }
            b'A' => {
                self.w("[")?;
                self.ty()?;
                self.w("; ")?;
                self.const_()?;
                self.w("]")
            }
            b'S' => {
                self.w("[")?;
                self.ty()?;
                self.w("]")
            }
            b'T' => {
                self.w("(")?;
                let mut n = 0;
                while !self.eat(b'E') {
                    if n > 0 {
                        self.w(", ")?;
                    }
                    self.ty()?;
                    n += 1;
                }
                if n == 1 {
                    self.w(",")?;
                }
                self.w(")")
            }
            b'F' => {
                if self.eat(b'G') {
                    self.base62()?;
                }
                if self.eat(b'U') {
                    self.w("unsafe ")?;
                }
                if self.eat(b'K') {
                    if self.eat(b'C') {
                        self.w("extern \"C\" ")?;
                    } else {
                        let (abi, _) = self.ident()?;
                        self.wf(format_args!("extern \"{abi}\" "))?;
                    }
                }
                self.w("fn(")?;
                let mut first = true;
                while !self.eat(b'E') {
                    if !first {
                        self.w(", ")?;
                    }
                    first = false;
                    self.ty()?;
                }
                self.w(")")?;
                if self.peek() == Some(b'u') {
                    self.pos += 1;
                    Ok(())
                } else {
                    self.w(" -> ")?;
                    self.ty()
                }
            }
            b'D' => {
                if self.eat(b'G') {
                    self.base62()?;
                }
                self.w("dyn ")?;
                let mut first = true;
                while !self.eat(b'E') {
                    if !first {
                        self.w(" + ")?;
                    }
                    first = false;
                    self.path()?;
                    while self.eat(b'p') {
                        let (id, _) = self.ident()?;
                        self.wf(format_args!("<{id} = "))?;
                        self.ty()?;
                        self.w(">")?;
                    }
                }
                if self.eat(b'L') {
                    self.base62()?;
                }
                Ok(())
            }
            b'B' => self.backref(|p| p.ty()),
            _ => {
                self.pos -= 1;
                self.path()
            }
        }
    }

    fn const_(&mut self) -> R {
        match self.peek() {
            Some(b'p') => {
                self.pos += 1;
                self.w("_")
            }
            Some(b'B') => {
                self.pos += 1;
                self.backref(|p| p.const_())
            }
            _ => {
                let t = self.next()?;
                let neg = self.eat(b'n');
                let mut v: u128 = 0;
                loop {
                    let c = self.next()?;
                    let d = match c {
                        b'_' => break,
                        b'0'..=b'9' => c - b'0',
                        b'a'..=b'f' => c - b'a' + 10,
                        _ => return Err(()),
                    };
                    v = v
                        .checked_mul(16)
                        .ok_or(())?
                        .checked_add(d as u128)
                        .ok_or(())?;
                }
                match t {
                    b'b' => self.w(if v != 0 { "true" } else { "false" }),
                    b'c' => match char::from_u32(v as u32) {
                        Some(ch) => self.wf(format_args!("'{ch}'")),
                        None => Err(()),
                    },
                    _ => {
                        if neg {
                            self.w("-")?;
                        }
                        self.wf(format_args!("{v}"))
                    }
                }
            }
        }
    }
}

/// Entrada fora da gramática suportada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DemangleError;

/// Demangla `sym` (com prefixo `_R`) em `out`. `Err` se a entrada não for suportada.
pub fn demangle_v0(sym: &str, out: &mut dyn fmt::Write) -> Result<(), DemangleError> {
    let inner = sym.strip_prefix("_R").ok_or(DemangleError)?;
    let inner = inner.split('.').next().unwrap_or(inner);
    let mut p = Parser {
        b: inner.as_bytes(),
        pos: 0,
        depth: 0,
        silent: 0,
        out,
    };
    if p.peek().is_some_and(|c| c.is_ascii_digit()) {
        return Err(DemangleError); // versão de codificação desconhecida
    }
    // O que sobra após o caminho (instantiating-crate) é ignorado.
    p.path().map_err(|_| DemangleError)
}

/// Valida sem escrever; usado para decidir entre saída demanglada ou crua.
pub fn is_valid_v0(sym: &str) -> bool {
    struct Sink;
    impl fmt::Write for Sink {
        fn write_str(&mut self, _: &str) -> fmt::Result {
            Ok(())
        }
    }
    demangle_v0(sym, &mut Sink).is_ok()
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::string::String;

    fn dm(s: &str) -> String {
        let mut out = String::new();
        demangle_v0(s, &mut out).unwrap_or_else(|_| out = String::from("<ERR>"));
        out
    }

    #[test]
    fn paths() {
        assert_eq!(
            dm("_RNvNtCsaK74HBiwFa0_4core9panicking9panic_fmt"),
            "core::panicking::panic_fmt"
        );
        assert_eq!(
            dm("_RNvCsa3dg2UBIvOz_11nexo_kernel5kmain"),
            "nexo_kernel::kmain"
        );
        assert_eq!(
            dm("_RNvNvNtCsa3dg2UBIvOz_11nexo_kernel8selftest25deliberate_stack_overflow7recurse"),
            "nexo_kernel::selftest::deliberate_stack_overflow::recurse"
        );
        assert_eq!(
            dm("_RNvNtCs1234_7mycrate3foo3bar.llvm.123"),
            "mycrate::foo::bar"
        );
        assert_eq!(
            dm("_RNCNvCs1234_7mycrate4main0"),
            "mycrate::main::{closure#0}"
        );
        assert_eq!(
            dm("_RNCNvCs1234_7mycrate4mains_0"),
            "mycrate::main::{closure#1}"
        );
    }

    #[test]
    fn generics_and_backrefs() {
        assert_eq!(dm("_RINvNtC4core3mem7size_ofdE"), "core::mem::size_of<f64>");
        assert_eq!(
            dm("_RINvNtC4core3mem7size_ofNtB4_3FooE"),
            "core::mem::size_of<core::Foo>"
        );
        assert_eq!(dm("_RINvC3foo3barReE"), "foo::bar<&str>");
        assert_eq!(dm("_RINvC3foo3barTlmEE"), "foo::bar<(i32, u32)>");
        assert_eq!(dm("_RINvC3foo3barAhj8_E"), "foo::bar<[u8; 8]>");
        assert_eq!(dm("_RINvC3foo3barKlnff_E"), "foo::bar<-255>");
        assert_eq!(dm("_RINvC3foo3barKb1_E"), "foo::bar<true>");
    }

    #[test]
    fn impls() {
        assert_eq!(
            dm("_RNvMNtC4core6optionINtNtC4core6option6OptionlE4take"),
            "<core::option::Option<i32>>::take"
        );
        assert_eq!(
            dm("_RNvXNtC4core3fmtNtC4core3FooNtNtC4core3fmt5Debug3fmt"),
            "<core::Foo as core::fmt::Debug>::fmt"
        );
        assert_eq!(
            dm("_RNvYNtC4core3FooNtC4core5Trait1f"),
            "<core::Foo as core::Trait>::f"
        );
    }

    #[test]
    fn types() {
        assert_eq!(dm("_RINvC3foo3barFEuE"), "foo::bar<fn()>");
        assert_eq!(
            dm("_RINvC3foo3barFKClEmE"),
            "foo::bar<extern \"C\" fn(i32) -> u32>"
        );
        assert_eq!(
            dm("_RINvC3foo3barDNtC4core5TraitEL_E"),
            "foo::bar<dyn core::Trait>"
        );
        assert_eq!(
            dm("_RINvC3foo3barQlPmSyE"),
            "foo::bar<&mut i32, *const u32, [u64]>"
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(!is_valid_v0("_R"));
        assert!(!is_valid_v0("_R0NvC3foo3bar"));
        assert!(!is_valid_v0("_RNvC99foo"));
        assert!(!is_valid_v0("_RB_"));
        assert!(!is_valid_v0("_RNvBa_3foo")); // backref para frente
        assert!(is_valid_v0("_RNvC3foo3bar"));
    }
}

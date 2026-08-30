//! Symbolication a partir da cópia do ELF do kernel entregue pelo loader.

use core::fmt;
use nexo_mm::PhysAddr;
use nexo_symbols::{Symbol, SymbolTable};
use nexo_sync::Once;

static TABLE: Once<SymbolTable<'static>> = Once::new();

/// Carrega `.symtab` do ELF.
pub fn init() {
    let bi = crate::boot::info();
    if bi.kernel_file_len == 0 {
        kwarn!("symbols: loader nao entregou o ELF; backtraces sem nomes");
        return;
    }
    let ptr = crate::mm::virt::phys_to_virt(PhysAddr::new(bi.kernel_file_addr)).as_ptr();
    // SAFETY: páginas do tipo KernelFile, reservadas e imutáveis.
    let bytes: &'static [u8] =
        unsafe { core::slice::from_raw_parts(ptr, bi.kernel_file_len as usize) };
    match SymbolTable::parse(bytes) {
        Some(t) => {
            kinfo!(
                "symbols: {} entradas em .symtab ({} KiB de ELF)",
                t.len(),
                bytes.len() >> 10
            );
            let _ = TABLE.set(t);
        }
        None => kwarn!("symbols: .symtab ausente no ELF"),
    }
}

/// Resolve um endereço.
pub fn lookup(addr: u64) -> Option<Symbol<'static>> {
    TABLE.get()?.lookup(addr)
}

/// Procura por nome exato.
#[allow(dead_code)]
pub fn find(name: &str) -> Option<Symbol<'static>> {
    TABLE.get()?.find(name)
}

/// Endereço formatado como `0x... <nome+0xoff>`.
pub struct Symbolized {
    addr: u64,
    lookup: u64,
}

impl Symbolized {
    /// Endereço de instrução (ex.: RIP de uma exceção).
    pub fn pc(addr: u64) -> Self {
        Symbolized { addr, lookup: addr }
    }
    /// Endereço de retorno: exibe `addr`, mas resolve `addr - 1` (dentro do `call`).
    pub fn return_address(addr: u64) -> Self {
        Symbolized {
            addr,
            lookup: addr.saturating_sub(1),
        }
    }
}

impl fmt::Display for Symbolized {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match lookup(self.lookup) {
            Some(s) => write!(
                f,
                "{:#018x} <{}+{:#x}>",
                self.addr,
                s.demangled(),
                self.addr - s.start
            ),
            None => write!(f, "{:#018x} <?>", self.addr),
        }
    }
}

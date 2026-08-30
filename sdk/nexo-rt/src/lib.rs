//! Runtime mínimo para programas de usuário: formatação em buffer fixo,
//! `log!` para o kernel e `panic_handler` (feature `panic-handler`).
#![no_std]

pub use nexo_sys as sys;

/// Buffer de texto de tamanho fixo que implementa `core::fmt::Write`.
pub struct Buf<const N: usize> {
    data: [u8; N],
    len: usize,
}

impl<const N: usize> Default for Buf<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Buf<N> {
    /// Buffer vazio.
    pub const fn new() -> Self {
        Buf {
            data: [0; N],
            len: 0,
        }
    }
    /// Texto acumulado.
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.len]).unwrap_or("")
    }
    /// Bytes acumulados.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data[..self.len]
    }
    /// Esvazia.
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl<const N: usize> core::fmt::Write for Buf<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(N - self.len);
        self.data[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// Formata e envia ao log do kernel (até 512 bytes).
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        use core::fmt::Write as _;
        let mut b = $crate::Buf::<512>::new();
        let _ = write!(b, $($arg)*);
        $crate::sys::log(b.as_str());
    }};
}

/// Handler de panic: registra e encerra com 101.
#[cfg(feature = "panic-handler")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log!("panic: {}", info.message());
    nexo_sys::exit(101)
}

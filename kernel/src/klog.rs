//! Logger estruturado mínimo: `[segundos.micros] NIVEL modulo: mensagem`.
//!
//! Saída primária é a serial COM1 (lida pelo CI); opcionalmente espelhada no
//! console de framebuffer. Nunca aloca. No caminho de panic o lock é forçado.

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicU8, Ordering};
use nexo_arch_x86_64::{cpu, serial::SerialPort};
use nexo_sync::SpinLock;

/// Nível de log.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Level {
    /// Erro.
    Error = 0,
    /// Aviso.
    Warn = 1,
    /// Informação.
    Info = 2,
    /// Depuração.
    Debug = 3,
    /// Rastreamento.
    Trace = 4,
}

impl Level {
    /// Nome fixo de 5 colunas.
    pub const fn name(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN ",
            Level::Info => "INFO ",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }
    /// Converte o valor de `loglevel=`.
    pub fn parse(s: &str) -> Option<Level> {
        match s {
            "error" => Some(Level::Error),
            "warn" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            "trace" => Some(Level::Trace),
            _ => None,
        }
    }
    fn from_u8(v: u8) -> Level {
        match v {
            0 => Level::Error,
            1 => Level::Warn,
            2 => Level::Info,
            3 => Level::Debug,
            _ => Level::Trace,
        }
    }
}

struct Sink {
    serial: SerialPort,
    console: bool,
}

static SINK: SpinLock<Sink> = SpinLock::new(Sink {
    serial: SerialPort::new(SerialPort::COM1),
    console: false,
});
static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// Inicializa a serial.
pub fn init() {
    // SAFETY: COM1 é a UART padrão do PC; a inicialização é idempotente.
    let ok = unsafe { SerialPort::new(SerialPort::COM1).init() };
    if !ok {
        // Sem UART funcional nada mais pode ser dito; seguimos mesmo assim.
        let mut s = SerialPort::new(SerialPort::COM1);
        let _ = s.write_str("serial: loopback falhou; saida pode ser perdida\n");
    }
}

/// Define o nível máximo exibido.
pub fn set_level(l: Level) {
    LEVEL.store(l as u8, Ordering::Relaxed);
}

/// Nível atual.
pub fn level() -> Level {
    Level::from_u8(LEVEL.load(Ordering::Relaxed))
}

/// Liga o espelhamento no console de framebuffer.
pub fn enable_console() {
    cpu::without_interrupts(|| SINK.lock().console = true);
}

/// Suspende o espelho no console gráfico (a serial continua): usado quando outro dono desenha no
/// framebuffer (ex.: teste de apresentação do compositor). Reative com [`enable_console`].
pub fn disable_console() {
    cpu::without_interrupts(|| SINK.lock().console = false);
}

/// Escreve uma linha de log.
pub fn write(level: Level, target: &str, args: fmt::Arguments) {
    if level > self::level() {
        return;
    }
    let us = crate::time::uptime_micros();
    let target = match target.strip_prefix("nexo_kernel::") {
        Some(t) => t,
        None if target == "nexo_kernel" => "kernel",
        None => target,
    };
    cpu::without_interrupts(|| {
        let mut s = SINK.lock();
        let _ = writeln!(
            s.serial,
            "[{:5}.{:06}] {} {}: {}",
            us / 1_000_000,
            us % 1_000_000,
            level.name(),
            target,
            args
        );
        if s.console {
            crate::console::write_fmt(format_args!(
                "[{:4}.{:03}] {} {}\n",
                us / 1_000_000,
                (us / 1000) % 1000,
                level.name().trim_end(),
                args
            ));
        }
    });
}

/// Escreve texto cru (sem prefixo) na serial e no console.
pub fn print(args: fmt::Arguments) {
    cpu::without_interrupts(|| {
        let mut s = SINK.lock();
        let _ = s.serial.write_fmt(args);
        if s.console {
            crate::console::write_fmt(args);
        }
    });
}

/// Libera o lock à força (caminho de panic).
///
/// # Safety
/// Somente quando o detentor não voltará a executar (panic/exceção fatal).
pub unsafe fn force_unlock() {
    // SAFETY: contrato da função.
    unsafe { SINK.force_unlock() };
}

macro_rules! kprint {
    ($($arg:tt)*) => { $crate::klog::print(format_args!($($arg)*)) };
}
macro_rules! klog {
    ($lvl:expr, $($arg:tt)*) => {
        $crate::klog::write($lvl, module_path!(), format_args!($($arg)*))
    };
}
macro_rules! kerror {
    ($($arg:tt)*) => { klog!($crate::klog::Level::Error, $($arg)*) };
}
macro_rules! kwarn {
    ($($arg:tt)*) => { klog!($crate::klog::Level::Warn, $($arg)*) };
}
macro_rules! kinfo {
    ($($arg:tt)*) => { klog!($crate::klog::Level::Info, $($arg)*) };
}
macro_rules! kdebug {
    ($($arg:tt)*) => { klog!($crate::klog::Level::Debug, $($arg)*) };
}
#[allow(unused_macros)]
macro_rules! ktrace {
    ($($arg:tt)*) => { klog!($crate::klog::Level::Trace, $($arg)*) };
}

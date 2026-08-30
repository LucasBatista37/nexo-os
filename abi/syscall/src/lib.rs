//! ABI de syscalls do Nexo OS — **versão 0 (instável)**. Especificação em
//! `docs/spec/syscall-abi.md`.
//!
//! Convenção x86_64: instrução `syscall`; número em `RAX`; argumentos em
//! `RDI, RSI, RDX, R10, R8, R9`; retorno: `RAX` = [`Status`] (0 = OK),
//! `RDX` = valor. `RCX` e `R11` são destruídos pela instrução; os demais
//! registradores são preservados pelo kernel.
#![no_std]
#![deny(unsafe_code)]

/// Versão da ABI devolvida por [`SYS_ABI_VERSION`].
pub const ABI_VERSION: u64 = 0;

/// Encerra o processo atual. `a0` = código de saída.
pub const SYS_EXIT: u64 = 0;
/// Escreve `a1` bytes de `a0` no log do kernel (UTF-8; máximo [`LOG_MAX`]).
pub const SYS_LOG: u64 = 1;
/// Relógio monotônico em nanossegundos → `RDX`.
pub const SYS_TIME_NOW: u64 = 2;
/// Cede a CPU.
pub const SYS_YIELD: u64 = 3;
/// Dorme `a0` nanossegundos (arredondado para cima ao tick).
pub const SYS_SLEEP: u64 = 4;
/// ID do processo atual → `RDX`.
pub const SYS_GET_PID: u64 = 5;
/// Versão da ABI → `RDX`.
pub const SYS_ABI_VERSION: u64 = 6;
/// Informação de depuração do kernel: `a0` seleciona (0 = CPUs online, 1 = uptime ms, 2 = syscalls do processo) → `RDX`.
pub const SYS_DEBUG_INFO: u64 = 7;
/// Maior número válido nesta versão.
pub const SYS_MAX: u64 = 7;

/// Tamanho máximo de uma mensagem de [`SYS_LOG`].
pub const LOG_MAX: usize = 1024;

/// Fim da metade de usuário do espaço de endereçamento (exclusivo).
pub const USER_ADDRESS_LIMIT: u64 = 0x0000_8000_0000_0000;

/// Código de exit atribuído pelo kernel a processos encerrados por falha.
pub const EXIT_KILLED: i64 = -1;

/// Status de uma syscall.
#[repr(u64)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Sucesso.
    Ok = 0,
    /// Argumento inválido.
    InvalidArgs = 1,
    /// Ponteiro fora do espaço do usuário ou não mapeado.
    BadAddress = 2,
    /// Número de syscall desconhecido.
    NotSupported = 3,
    /// Sem memória.
    NoMemory = 4,
    /// Recurso inexistente.
    NotFound = 5,
    /// Permissão negada (capability ausente).
    Denied = 6,
    /// Valor desconhecido (reservado).
    Unknown = u64::MAX,
}

impl Status {
    /// Converte o valor de `RAX`.
    pub const fn from_u64(v: u64) -> Status {
        match v {
            0 => Status::Ok,
            1 => Status::InvalidArgs,
            2 => Status::BadAddress,
            3 => Status::NotSupported,
            4 => Status::NoMemory,
            5 => Status::NotFound,
            6 => Status::Denied,
            _ => Status::Unknown,
        }
    }
    /// `true` se sucesso.
    pub const fn is_ok(self) -> bool {
        matches!(self, Status::Ok)
    }
    /// Nome curto.
    pub const fn name(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::InvalidArgs => "invalid-args",
            Status::BadAddress => "bad-address",
            Status::NotSupported => "not-supported",
            Status::NoMemory => "no-memory",
            Status::NotFound => "not-found",
            Status::Denied => "denied",
            Status::Unknown => "unknown",
        }
    }
}

/// Nome de uma syscall (para logs).
pub const fn syscall_name(n: u64) -> &'static str {
    match n {
        SYS_EXIT => "exit",
        SYS_LOG => "log",
        SYS_TIME_NOW => "time_now",
        SYS_YIELD => "yield",
        SYS_SLEEP => "sleep",
        SYS_GET_PID => "get_pid",
        SYS_ABI_VERSION => "abi_version",
        SYS_DEBUG_INFO => "debug_info",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        for s in [
            Status::Ok,
            Status::InvalidArgs,
            Status::BadAddress,
            Status::NotSupported,
            Status::NoMemory,
            Status::NotFound,
            Status::Denied,
        ] {
            assert_eq!(Status::from_u64(s as u64), s);
        }
        assert_eq!(Status::from_u64(999), Status::Unknown);
        assert!(Status::Ok.is_ok());
        assert_eq!(syscall_name(SYS_LOG), "log");
        assert_eq!(syscall_name(SYS_MAX + 1), "?");
    }
}

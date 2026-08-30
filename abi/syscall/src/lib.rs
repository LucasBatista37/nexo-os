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
/// Fecha o handle `a0`.
pub const SYS_HANDLE_CLOSE: u64 = 8;
/// Duplica o handle `a0` com direitos `a1` (subconjunto dos atuais) → novo handle em `RDX`.
pub const SYS_HANDLE_DUPLICATE: u64 = 9;
/// Cria um canal; `RDX` = `h0 | (h1 << 32)`.
pub const SYS_CHANNEL_CREATE: u64 = 10;
/// Envia `a2` bytes de `a1` pelo canal `a0`, com `a4` handles lidos de `a3` (u32 cada).
pub const SYS_CHANNEL_SEND: u64 = 11;
/// Recebe do canal `a0` em `a1` (capacidade `a2`), handles em `a3` (capacidade `a4`); `RDX` = `len | (nhandles << 32)`. Bloqueia até haver mensagem.
pub const SYS_CHANNEL_RECV: u64 = 12;
/// Informação do handle `a0`: `RDX` = `rights | (kind << 32)`.
pub const SYS_HANDLE_INFO: u64 = 13;
/// Maior número válido nesta versão.
pub const SYS_MAX: u64 = 13;

/// Tamanho máximo de uma mensagem de canal.
pub const MSG_MAX: usize = 4096;
/// Máximo de handles por mensagem.
pub const MSG_HANDLES_MAX: usize = 8;
/// Profundidade da fila de cada extremidade.
pub const CHANNEL_QUEUE_MAX: usize = 64;
/// Handles por processo.
pub const HANDLES_MAX: usize = 256;
/// Valor de handle inválido.
pub const HANDLE_INVALID: u32 = u32::MAX;

/// Direito de ler/receber.
pub const RIGHT_READ: u32 = 1 << 0;
/// Direito de escrever/enviar.
pub const RIGHT_WRITE: u32 = 1 << 1;
/// Direito de transferir por canal.
pub const RIGHT_TRANSFER: u32 = 1 << 2;
/// Direito de duplicar.
pub const RIGHT_DUPLICATE: u32 = 1 << 3;
/// Direito de sinalizar.
pub const RIGHT_SIGNAL: u32 = 1 << 4;
/// Direito de mapear.
pub const RIGHT_MAP: u32 = 1 << 5;
/// Direito administrativo.
pub const RIGHT_ADMIN: u32 = 1 << 6;
/// Todos os direitos aplicáveis a um canal.
pub const RIGHTS_CHANNEL_DEFAULT: u32 = RIGHT_READ | RIGHT_WRITE | RIGHT_TRANSFER | RIGHT_DUPLICATE;

/// Tipo de objeto: extremidade de canal.
pub const KIND_CHANNEL: u32 = 1;

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
    /// A outra extremidade do canal foi fechada e não há mensagens pendentes.
    PeerClosed = 7,
    /// Handle inválido ou fechado.
    BadHandle = 8,
    /// Operação bloquearia (reservado).
    WouldBlock = 9,
    /// Mensagem/lista maior que o limite, ou buffer pequeno demais.
    TooBig = 10,
    /// Fila cheia.
    QueueFull = 11,
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
            7 => Status::PeerClosed,
            8 => Status::BadHandle,
            9 => Status::WouldBlock,
            10 => Status::TooBig,
            11 => Status::QueueFull,
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
            Status::PeerClosed => "peer-closed",
            Status::BadHandle => "bad-handle",
            Status::WouldBlock => "would-block",
            Status::TooBig => "too-big",
            Status::QueueFull => "queue-full",
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
        SYS_HANDLE_CLOSE => "handle_close",
        SYS_HANDLE_DUPLICATE => "handle_duplicate",
        SYS_CHANNEL_CREATE => "channel_create",
        SYS_CHANNEL_SEND => "channel_send",
        SYS_CHANNEL_RECV => "channel_recv",
        SYS_HANDLE_INFO => "handle_info",
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
            Status::PeerClosed,
            Status::BadHandle,
            Status::TooBig,
            Status::QueueFull,
        ] {
            assert_eq!(Status::from_u64(s as u64), s);
        }
        assert_eq!(Status::from_u64(999), Status::Unknown);
        assert!(Status::Ok.is_ok());
        assert_eq!(syscall_name(SYS_LOG), "log");
        assert_eq!(syscall_name(SYS_MAX + 1), "?");
    }
}

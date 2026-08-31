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
/// Cria um processo a partir do membro `a0[..a1]` do initrd com `RDI = a2` e `a4` handles lidos de `a3`; `RDX` = handle do processo.
pub const SYS_PROCESS_SPAWN: u64 = 14;
/// Aguarda o processo do handle `a0` terminar; `RDX` = código de saída (i64).
pub const SYS_PROCESS_WAIT: u64 = 15;
/// Informação do processo do handle `a0`: `RDX` = `pid | (1 << 63 se terminou)`.
pub const SYS_PROCESS_INFO: u64 = 16;
/// Dispositivos (exigem handle de dispositivo `a0` com os direitos indicados):
/// copia até `a2` [`PciInfo`] para `a1`; `RDX` = total de funções PCI. (`READ`)
pub const SYS_PCI_ENUM: u64 = 17;
/// Lê 32 bits do espaço de configuração PCI de `a1` (BDF compacto) no offset `a2`. (`READ`)
pub const SYS_PCI_CFG_READ: u64 = 18;
/// Escreve `a3` (32 bits) no espaço de configuração de `a1` no offset `a2`. (`WRITE`)
pub const SYS_PCI_CFG_WRITE: u64 = 19;
/// Mapeia MMIO `[a1, a1+a2)` (dentro de um BAR conhecido) no processo; `RDX` = endereço virtual. (`MAP`)
pub const SYS_MMIO_MAP: u64 = 20;
/// Aloca uma página de DMA (4 KiB, zerada); escreve [`DmaBuffer`] em `a1`. (`MAP`)
pub const SYS_DMA_ALLOC: u64 = 21;
/// Reserva um vetor de interrupção para MSI/MSI-X; escreve [`IrqInfo`] em `a1`. (`SIGNAL`)
pub const SYS_IRQ_ALLOC: u64 = 22;
/// Bloqueia até o vetor `a1` ter disparado mais de `a2` vezes; `RDX` = contagem atual. (`SIGNAL`)
pub const SYS_IRQ_WAIT: u64 = 23;
/// Deriva de uma concessão raiz (`ADMIN`) uma concessão restrita à função PCI `a1` (BDF compacto);
/// `RDX` = novo handle com [`RIGHTS_DEVICE_DEFAULT`]. (`ADMIN`)
pub const SYS_DEVICE_OPEN: u64 = 24;
/// Como [`SYS_CHANNEL_RECV`], mas devolve [`Status::WouldBlock`] em vez de bloquear quando
/// não há mensagem (e o par continua aberto).
pub const SYS_CHANNEL_TRY_RECV: u64 = 25;
/// Espera múltipla: bloqueia até algum dos canais em `a0` (array de `a1` handles, ≤ 16) ter
/// mensagem ou par fechado; `RDX` = índice do primeiro pronto. Exige `READ` em todos.
pub const SYS_CHANNEL_WAIT_ANY: u64 = 26;
/// Maior número válido nesta versão.
pub const SYS_MAX: u64 = 26;
/// Máximo de handles em uma espera múltipla.
pub const WAIT_ANY_MAX: usize = 16;

/// Tipo de objeto: concessão de acesso a dispositivos.
pub const KIND_DEVICE: u32 = 3;
/// Direitos da concessão raiz entregue ao gerenciador de dispositivos (pode derivar concessões).
pub const RIGHTS_DEVICE_ALL: u32 = RIGHTS_DEVICE_DEFAULT | RIGHT_ADMIN;
/// Direitos padrão de uma concessão de dispositivo.
pub const RIGHTS_DEVICE_DEFAULT: u32 =
    RIGHT_READ | RIGHT_WRITE | RIGHT_MAP | RIGHT_SIGNAL | RIGHT_TRANSFER | RIGHT_DUPLICATE;
/// Base da região de usuário onde o kernel mapeia MMIO e páginas de DMA.
pub const USER_DEVICE_REGION: u64 = 0x0000_6000_0000_0000;
/// Máximo de BARs por função PCI.
pub const PCI_BARS: usize = 6;

/// Um BAR PCI decodificado.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PciBar {
    /// Endereço físico (MMIO) ou porta (E/S).
    pub base: u64,
    /// Tamanho em bytes (0 = ausente).
    pub size: u64,
    /// Bit 0: espaço de E/S; bit 1: 64 bits; bit 2: prefetchable.
    pub flags: u32,
    /// Reservado.
    pub reserved: u32,
}

/// Uma função PCI (64 + 6×24 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PciInfo {
    /// BDF compacto (`bus << 8 | dev << 3 | func`).
    pub bdf: u16,
    /// Vendor ID.
    pub vendor: u16,
    /// Device ID.
    pub device: u16,
    /// Revisão.
    pub revision: u8,
    /// Tipo de cabeçalho (bit 7 = multifunção).
    pub header_type: u8,
    /// Classe.
    pub class: u8,
    /// Subclasse.
    pub subclass: u8,
    /// Interface de programação.
    pub prog_if: u8,
    /// Linha de IRQ legada.
    pub irq_line: u8,
    /// Pino de IRQ (1 = INTA#).
    pub irq_pin: u8,
    /// Reservado.
    pub reserved: [u8; 3],
    /// Subsystem vendor/device.
    pub subsystem: u32,
    /// BARs.
    pub bars: [PciBar; PCI_BARS],
}

impl PciInfo {
    /// `true` se é um dispositivo VirtIO (vendor 0x1AF4).
    pub const fn is_virtio(&self) -> bool {
        self.vendor == 0x1af4
    }
}

/// Página de DMA devolvida por [`SYS_DMA_ALLOC`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DmaBuffer {
    /// Endereço virtual no processo.
    pub virt: u64,
    /// Endereço físico (para o dispositivo).
    pub phys: u64,
    /// Tamanho em bytes.
    pub len: u64,
}

/// Vetor de interrupção reservado por [`SYS_IRQ_ALLOC`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrqInfo {
    /// Vetor.
    pub vector: u32,
    /// Reservado.
    pub reserved: u32,
    /// Endereço de mensagem MSI (`0xFEE0_0000 | apic_id << 12`).
    pub msi_address: u64,
    /// Dados de mensagem MSI (vetor).
    pub msi_data: u32,
    /// Reservado.
    pub reserved2: u32,
}

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
/// Tipo de objeto: processo.
pub const KIND_PROCESS: u32 = 2;
/// Direitos padrão de um handle de processo (`READ` = esperar/consultar).
pub const RIGHTS_PROCESS_DEFAULT: u32 = RIGHT_READ | RIGHT_TRANSFER | RIGHT_DUPLICATE;
/// Bit de "terminou" em `SYS_PROCESS_INFO`.
pub const PROCESS_INFO_EXITED: u64 = 1 << 63;

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
        SYS_PROCESS_SPAWN => "process_spawn",
        SYS_PROCESS_WAIT => "process_wait",
        SYS_PROCESS_INFO => "process_info",
        SYS_PCI_ENUM => "pci_enum",
        SYS_PCI_CFG_READ => "pci_cfg_read",
        SYS_PCI_CFG_WRITE => "pci_cfg_write",
        SYS_MMIO_MAP => "mmio_map",
        SYS_DMA_ALLOC => "dma_alloc",
        SYS_IRQ_ALLOC => "irq_alloc",
        SYS_IRQ_WAIT => "irq_wait",
        SYS_DEVICE_OPEN => "device_open",
        SYS_CHANNEL_TRY_RECV => "channel_try_recv",
        SYS_CHANNEL_WAIT_ANY => "channel_wait_any",
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
        assert_eq!(core::mem::size_of::<PciInfo>(), 24 + 24 * PCI_BARS);
        assert_eq!(core::mem::size_of::<DmaBuffer>(), 24);
        assert_eq!(core::mem::size_of::<IrqInfo>(), 24);
    }
}

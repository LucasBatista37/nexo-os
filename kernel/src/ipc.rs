//! Objetos do kernel, handles com direitos e canais de IPC (ADR-0004/0005).
//!
//! - Um [`Handle`] é uma referência a um objeto com um conjunto de direitos
//!   que só diminui (`duplicate` com subconjunto). Handles vivem na tabela do
//!   processo e são inteiros opacos para o usuário.
//! - Um canal tem duas extremidades; cada uma tem uma fila de mensagens
//!   (bytes + handles). `send` na extremidade A enfileira em B. Handles
//!   enviados são retirados da tabela do remetente e entregues no `recv`.
//! - `recv` bloqueia a thread até haver mensagem ou o par fechar.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use nexo_syscall_abi::*;

use crate::sched::{self, ThreadId};
use crate::sync::IrqLock;

/// Direitos de um handle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rights(pub u32);

impl Rights {
    /// `true` se contém todos os bits de `r`.
    pub const fn contains(self, r: u32) -> bool {
        self.0 & r == r
    }
    /// `true` se `other` é subconjunto.
    pub const fn is_superset_of(self, other: Rights) -> bool {
        other.0 & !self.0 == 0
    }
}

/// Objeto referenciado por um handle.
#[derive(Clone)]
pub enum Object {
    /// Extremidade de canal.
    Channel(Arc<ChannelEnd>),
}

impl Object {
    /// Tipo para `SYS_HANDLE_INFO`.
    pub fn kind(&self) -> u32 {
        match self {
            Object::Channel(_) => KIND_CHANNEL,
        }
    }
}

/// Handle: objeto + direitos.
#[derive(Clone)]
pub struct Handle {
    /// Objeto.
    pub object: Object,
    /// Direitos.
    pub rights: Rights,
}

/// Tabela de handles de um processo.
pub struct HandleTable {
    slots: Vec<Option<Handle>>,
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::new()
    }
}

impl HandleTable {
    /// Tabela vazia.
    pub const fn new() -> Self {
        HandleTable { slots: Vec::new() }
    }

    /// Insere e devolve o índice.
    pub fn insert(&mut self, h: Handle) -> Result<u32, Status> {
        if let Some(i) = self.slots.iter().position(|s| s.is_none()) {
            self.slots[i] = Some(h);
            return Ok(i as u32);
        }
        if self.slots.len() >= HANDLES_MAX {
            return Err(Status::NoMemory);
        }
        self.slots.push(Some(h));
        Ok((self.slots.len() - 1) as u32)
    }

    /// Obtém uma cópia do handle `i`.
    pub fn get(&self, i: u32) -> Result<Handle, Status> {
        self.slots
            .get(i as usize)
            .and_then(|s| s.clone())
            .ok_or(Status::BadHandle)
    }

    /// Remove e devolve o handle `i`.
    pub fn take(&mut self, i: u32) -> Result<Handle, Status> {
        self.slots
            .get_mut(i as usize)
            .and_then(|s| s.take())
            .ok_or(Status::BadHandle)
    }

    /// Handles ocupados.
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// `true` se vazia.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Mensagem em trânsito.
pub struct Message {
    /// Bytes.
    pub data: Vec<u8>,
    /// Handles transferidos.
    pub handles: Vec<Handle>,
}

struct ChannelInner {
    queues: [VecDeque<Message>; 2],
    closed: [bool; 2],
    waiters: [Vec<ThreadId>; 2],
}

/// Extremidade de canal (`side` 0 ou 1). Ao ser destruída, fecha a extremidade.
pub struct ChannelEnd {
    inner: Arc<IrqLock<ChannelInner>>,
    side: usize,
}

static LIVE_ENDS: AtomicU64 = AtomicU64::new(0);
static MESSAGES_SENT: AtomicU64 = AtomicU64::new(0);

/// Extremidades de canal vivas (para testes de vazamento).
pub fn live_channel_ends() -> u64 {
    LIVE_ENDS.load(Ordering::Relaxed)
}

/// Mensagens enviadas desde o boot.
pub fn messages_sent() -> u64 {
    MESSAGES_SENT.load(Ordering::Relaxed)
}

impl ChannelEnd {
    /// Cria um canal e devolve as duas extremidades.
    pub fn create_pair() -> (Arc<ChannelEnd>, Arc<ChannelEnd>) {
        let inner = Arc::new(IrqLock::new(ChannelInner {
            queues: [VecDeque::new(), VecDeque::new()],
            closed: [false, false],
            waiters: [Vec::new(), Vec::new()],
        }));
        LIVE_ENDS.fetch_add(2, Ordering::Relaxed);
        (
            Arc::new(ChannelEnd {
                inner: inner.clone(),
                side: 0,
            }),
            Arc::new(ChannelEnd { inner, side: 1 }),
        )
    }

    /// Envia para a outra extremidade.
    pub fn send(&self, msg: Message) -> Result<(), Status> {
        let peer = 1 - self.side;
        let mut g = self.inner.lock();
        if g.closed[peer] {
            return Err(Status::PeerClosed);
        }
        if g.queues[peer].len() >= CHANNEL_QUEUE_MAX {
            return Err(Status::QueueFull);
        }
        g.queues[peer].push_back(msg);
        MESSAGES_SENT.fetch_add(1, Ordering::Relaxed);
        let waiters = core::mem::take(&mut g.waiters[peer]);
        drop(g);
        for w in waiters {
            sched::unpark(w);
        }
        Ok(())
    }

    /// Recebe (bloqueante). `Err(PeerClosed)` quando o par fechou e a fila está vazia.
    pub fn recv(&self) -> Result<Message, Status> {
        loop {
            let mut g = self.inner.lock();
            if let Some(m) = g.queues[self.side].pop_front() {
                return Ok(m);
            }
            if g.closed[1 - self.side] {
                return Err(Status::PeerClosed);
            }
            let me = sched::current().map(|t| t.id).ok_or(Status::Denied)?;
            g.waiters[self.side].push(me);
            // Solta o lock do canal já com a thread marcada como bloqueada (sem perder wakeups).
            sched::park_with(g);
        }
    }
}

impl Drop for ChannelEnd {
    fn drop(&mut self) {
        let mut g = self.inner.lock();
        g.closed[self.side] = true;
        let waiters = core::mem::take(&mut g.waiters[1 - self.side]);
        drop(g);
        for w in waiters {
            sched::unpark(w);
        }
        LIVE_ENDS.fetch_sub(1, Ordering::Relaxed);
    }
}

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
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    /// Processo.
    Process(Arc<crate::process::Process>),
    /// Concessão de acesso a dispositivos (PCI/MMIO/DMA/IRQ).
    Device(Arc<DeviceGrant>),
}

/// Concessão de acesso a dispositivos: `scope` = `None` (raiz: qualquer função PCI) ou uma
/// função específica (BDF compacto) — config, BARs e enumeração ficam limitados a ela.
pub struct DeviceGrant {
    /// Função PCI a que a concessão se limita (`None` = todas).
    pub scope: Option<u16>,
    /// Vetores de interrupção reservados por esta concessão (devolvidos no drop).
    pub vectors: IrqLock<Vec<u8>>,
}

impl DeviceGrant {
    /// Concessão total (raiz).
    pub fn all() -> Self {
        Self {
            scope: None,
            vectors: IrqLock::new(Vec::new()),
        }
    }

    /// Concessão restrita a uma função PCI.
    pub fn for_device(bdf: u16) -> Self {
        Self {
            scope: Some(bdf),
            vectors: IrqLock::new(Vec::new()),
        }
    }

    /// `true` se a concessão cobre `bdf`.
    pub fn covers(&self, bdf: u16) -> bool {
        self.scope.is_none_or(|s| s == bdf)
    }
}

impl Drop for DeviceGrant {
    fn drop(&mut self) {
        for v in self.vectors.lock().drain(..) {
            crate::irq::free(v);
        }
    }
}

impl Object {
    /// Tipo para `SYS_HANDLE_INFO`.
    pub fn kind(&self) -> u32 {
        match self {
            Object::Channel(_) => KIND_CHANNEL,
            Object::Process(_) => KIND_PROCESS,
            Object::Device(_) => KIND_DEVICE,
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
        if let Object::Channel(e) = &h.object {
            // Ao entrar em uma tabela, a ponta deixa de ser "presa" pelo kernel.
            e.pinned.store(false, Ordering::Relaxed);
        }
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

/// Extremidade de canal (`side` 0 ou 1). Ao ser destruída (ou coletada), fecha a extremidade.
pub struct ChannelEnd {
    inner: Arc<IrqLock<ChannelInner>>,
    side: usize,
    /// `true` enquanto só o kernel a segura (antes de entrar em uma tabela): raiz para o coletor.
    pinned: AtomicBool,
    closed_by_me: AtomicBool,
}

static LIVE_ENDS: AtomicU64 = AtomicU64::new(0);
static MESSAGES_SENT: AtomicU64 = AtomicU64::new(0);
static COLLECTED: AtomicU64 = AtomicU64::new(0);
/// Registro de todas as pontas existentes (fracas), para o coletor de ciclos.
static REGISTRY: IrqLock<Vec<Weak<ChannelEnd>>> = IrqLock::new(Vec::new());

/// Extremidades de canal **abertas** (para testes de vazamento).
pub fn live_channel_ends() -> u64 {
    LIVE_ENDS.load(Ordering::Relaxed)
}

/// Pontas fechadas pelo coletor de ciclos desde o boot.
pub fn collected_ends() -> u64 {
    COLLECTED.load(Ordering::Relaxed)
}

/// Fecha pontas de canal que nenhum processo vivo (nem o kernel) consegue mais alcançar.
///
/// Uma mensagem enfileirada pode carregar a handle da própria ponta que a
/// receberia (ou formar ciclos entre canais); nesse caso os `Arc` nunca
/// chegariam a zero. Marca-se tudo que é alcançável a partir das tabelas de
/// handles dos processos vivos e das pontas presas pelo kernel, atravessando
/// as filas; o resto é fechado (o que descarta as mensagens e quebra o ciclo).
pub fn collect_unreachable() -> u64 {
    let ends: Vec<Arc<ChannelEnd>> = {
        let mut reg = REGISTRY.lock();
        reg.retain(|w| w.strong_count() > 0);
        reg.iter().filter_map(|w| w.upgrade()).collect()
    };
    if ends.is_empty() {
        return 0;
    }
    let mut marked: Vec<*const ChannelEnd> = Vec::new();
    let mut work: Vec<Arc<ChannelEnd>> = Vec::new();
    fn root(
        e: &Arc<ChannelEnd>,
        marked: &mut Vec<*const ChannelEnd>,
        work: &mut Vec<Arc<ChannelEnd>>,
    ) {
        let p = Arc::as_ptr(e);
        if !marked.contains(&p) {
            marked.push(p);
            work.push(e.clone());
        }
    }
    for e in &ends {
        if e.pinned.load(Ordering::Relaxed) {
            root(e, &mut marked, &mut work);
        }
    }
    crate::process::for_each_live(|p| {
        let table = p.handles.lock();
        for i in 0..table.slots.len() as u32 {
            if let Ok(Handle {
                object: Object::Channel(e),
                ..
            }) = table.get(i)
            {
                root(&e, &mut marked, &mut work);
            }
        }
    });
    while let Some(e) = work.pop() {
        let g = e.inner.lock();
        for m in g.queues[e.side].iter() {
            for h in &m.handles {
                if let Object::Channel(x) = &h.object {
                    root(x, &mut marked, &mut work);
                }
            }
        }
    }
    let mut closed = 0;
    for e in &ends {
        if !marked.contains(&Arc::as_ptr(e)) && e.close() {
            closed += 1;
        }
    }
    drop(ends);
    if closed > 0 {
        COLLECTED.fetch_add(closed, Ordering::Relaxed);
        kdebug!("ipc: coletor fechou {closed} ponta(s) de canal inalcancavel(is)");
    }
    closed
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
        let a = Arc::new(ChannelEnd {
            inner: inner.clone(),
            side: 0,
            pinned: AtomicBool::new(true),
            closed_by_me: AtomicBool::new(false),
        });
        let b = Arc::new(ChannelEnd {
            inner,
            side: 1,
            pinned: AtomicBool::new(true),
            closed_by_me: AtomicBool::new(false),
        });
        let mut reg = REGISTRY.lock();
        reg.push(Arc::downgrade(&a));
        reg.push(Arc::downgrade(&b));
        (a, b)
    }

    /// `true` se `other` é uma ponta deste mesmo canal (qualquer lado).
    pub fn same_channel(&self, other: &ChannelEnd) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Fecha esta ponta: descarta sua fila, acorda o par. Devolve `true` na primeira vez.
    pub fn close(&self) -> bool {
        if self.closed_by_me.swap(true, Ordering::AcqRel) {
            return false;
        }
        let mut g = self.inner.lock();
        g.closed[self.side] = true;
        let dropped = core::mem::take(&mut g.queues[self.side]);
        let waiters = core::mem::take(&mut g.waiters[1 - self.side]);
        drop(g);
        drop(dropped);
        for w in waiters {
            sched::unpark(w);
        }
        LIVE_ENDS.fetch_sub(1, Ordering::Relaxed);
        true
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
    /// Como [`ChannelEnd::recv`], mas devolve `WouldBlock` em vez de bloquear.
    pub fn try_recv(&self) -> Result<Message, Status> {
        let mut g = self.inner.lock();
        if let Some(m) = g.queues[self.side].pop_front() {
            return Ok(m);
        }
        if g.closed[1 - self.side] {
            return Err(Status::PeerClosed);
        }
        Err(Status::WouldBlock)
    }

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
        self.close();
    }
}

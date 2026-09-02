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
    /// Memória compartilhável (frames físicos possuídos por este objeto).
    Memory(Arc<MemoryObject>),
    /// Capability de depuração (trace de syscalls); sem estado — o valor é possuí-la.
    Debug,
}

/// Objeto de memória compartilhável: possui os quadros físicos e os libera quando ninguém mais
/// o referencia. Vários processos podem mapeá-lo (`map_user_shared`), vendo a mesma memória.
pub struct MemoryObject {
    /// Quadros físicos (zerados na criação), na ordem das páginas.
    pub frames: Vec<nexo_mm::PhysAddr>,
    /// Tamanho em bytes (páginas × 4096).
    pub len: u64,
    /// Criador (para devolver a quota de páginas quando o objeto morrer); `Weak` para não
    /// formar ciclo Processo → tabela → objeto → Processo.
    pub owner: alloc::sync::Weak<crate::process::Process>,
}

impl Drop for MemoryObject {
    fn drop(&mut self) {
        if let Some(p) = self.owner.upgrade() {
            let pages = self.len / nexo_mm::PAGE_SIZE;
            p.shm_pages.fetch_sub(pages, Ordering::AcqRel);
        }
        for f in self.frames.drain(..) {
            let _ = crate::mm::phys::free_frame(f);
        }
    }
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
            Object::Memory(_) => KIND_MEMORY,
            Object::Debug => KIND_DEBUG,
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

/// Handles em mãos do kernel (fora de qualquer tabela ou fila) durante send/recv/spawn.
static IPC_INFLIGHT: AtomicU64 = AtomicU64::new(0);
/// Geração: incrementada a cada nova janela em-trânsito (valida a marcação do coletor).
static IPC_GEN: AtomicU64 = AtomicU64::new(0);

/// Guarda RAII de uma janela em-trânsito: handles retirados de uma fila/tabela mas ainda não
/// inseridos no destino são INVISÍVEIS à marcação do coletor — sem esta guarda, um
/// `collect_unreachable` concorrente (saída de processo em outra CPU) fecharia pontas vivas.
/// Visto em campo: o fs enxergou "cliente desconectou" com o canal em trânsito num runner
/// carregado (corrida entre o recv do destinatário e o exit do remetente).
pub struct InFlight(());

impl InFlight {
    /// Abre uma janela em-trânsito (bump de geração + contador).
    pub fn new() -> InFlight {
        IPC_GEN.fetch_add(1, Ordering::AcqRel);
        IPC_INFLIGHT.fetch_add(1, Ordering::AcqRel);
        InFlight(())
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        IPC_INFLIGHT.fetch_sub(1, Ordering::AcqRel);
    }
}
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
    // Janelas em-trânsito tornam handles invisíveis à marcação: espera zerar e valida a
    // geração após marcar; se uma janela abriu no meio, recomeça (ou desiste — a próxima
    // coleta apanha os ciclos; desistir nunca fecha nada vivo).
    for _tentativa in 0..8 {
        let mut spins = 0u32;
        while IPC_INFLIGHT.load(Ordering::Acquire) != 0 {
            spins += 1;
            if spins > 100_000 {
                return 0; // sistema ocupado: coleta adiada
            }
            core::hint::spin_loop();
        }
        let gen0 = IPC_GEN.load(Ordering::Acquire);
        let closed = collect_marked(gen0);
        if let Some(n) = closed {
            return n;
        }
    }
    0
}

fn collect_marked(gen0: u64) -> Option<u64> {
    let ends: Vec<Arc<ChannelEnd>> = {
        let mut reg = REGISTRY.lock();
        reg.retain(|w| w.strong_count() > 0);
        reg.iter().filter_map(|w| w.upgrade()).collect()
    };
    if ends.is_empty() {
        return Some(0);
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
    // valida: nenhuma janela em-trânsito abriu durante a marcação (senão a marcação pode ter
    // perdido handles vivos que estavam em mãos do kernel)
    if IPC_INFLIGHT.load(Ordering::Acquire) != 0 || IPC_GEN.load(Ordering::Acquire) != gen0 {
        return None;
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
    Some(closed)
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
    /// `true` se a fila do PAR (o outro lado) tem mensagens — usada para coalescer avisos.
    pub fn peer_readable(&self) -> bool {
        let g = self.inner.lock();
        !g.queues[1 - self.side].is_empty()
    }

    /// `true` se um `recv` não bloquearia (mensagem na fila ou par fechado).
    pub fn readable(&self) -> bool {
        let g = self.inner.lock();
        !g.queues[self.side].is_empty() || g.closed[1 - self.side]
    }

    /// Registra `t` para ser acordada quando este lado receber mensagem (ou o par fechar).
    /// Entradas obsoletas são drenadas no próximo `send`/`close` (acordar a mais é inócuo).
    pub fn register_waiter(&self, t: crate::sched::ThreadId) {
        self.inner.lock().waiters[self.side].push(t);
    }

    /// Como [`ChannelEnd::try_recv`], mas devolve também uma guarda [`InFlight`] criada sob o
    /// lock do canal no momento do pop: os handles da mensagem ficam protegidos do coletor até
    /// a guarda cair (depois de inseridos na tabela do destinatário).
    pub fn try_recv_guarded(&self) -> Result<(Message, InFlight), Status> {
        let mut g = self.inner.lock();
        if let Some(m) = g.queues[self.side].pop_front() {
            let guard = InFlight::new();
            return Ok((m, guard));
        }
        if g.closed[1 - self.side] {
            return Err(Status::PeerClosed);
        }
        Err(Status::WouldBlock)
    }

    /// Como [`ChannelEnd::recv`], mas com a guarda de [`ChannelEnd::try_recv_guarded`]. A
    /// guarda NÃO é mantida enquanto bloqueia — só nasce no pop.
    pub fn recv_guarded(&self) -> Result<(Message, InFlight), Status> {
        loop {
            let mut g = self.inner.lock();
            if let Some(m) = g.queues[self.side].pop_front() {
                let guard = InFlight::new();
                return Ok((m, guard));
            }
            if g.closed[1 - self.side] {
                return Err(Status::PeerClosed);
            }
            let me = sched::current().map(|t| t.id).ok_or(Status::Denied)?;
            g.waiters[self.side].push(me);
            sched::park_with(g);
        }
    }

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

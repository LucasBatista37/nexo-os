//! Escalonador preemptivo de threads de kernel (round-robin, quantum fixo, SMP).
//!
//! - Uma fila global de prontas protegida por spinlock (interrupções off).
//! - Cada CPU tem uma thread *idle* que nunca entra na fila.
//! - O timer local de cada CPU chama [`on_tick`]: contabiliza o quantum e
//!   preempta dentro do handler de interrupção (a troca de contexto acontece
//!   com o *trap frame* da thread interrompida em sua própria pilha).
//! - `sleep`, `join` e `exit` bloqueiam a thread e escalonam outra.
//! - O lock do escalonador é mantido através da troca de contexto e solto
//!   pela thread que recebe a CPU ([`finish_switch`]); é aí que uma thread
//!   moribunda (`Dying`) é movida para a lista de mortas, sem que sua pilha
//!   possa ser liberada enquanto ainda executa.
//! - Pilhas vivem em *slots* virtuais com guard page ([`stack`]).

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use nexo_arch_x86_64::context::{nexo_switch_context, prepare_stack};
use nexo_arch_x86_64::cpu;
use nexo_sync::SpinLock;

use crate::acpi::MAX_CPUS;
use crate::x86::percpu::{self, PerCpu};

/// Identificador de thread.
pub type ThreadId = usize;
/// Função de entrada e argumento de uma thread.
pub type Entry = (fn(usize), usize);

/// Quantum em ticks (1 ms cada).
pub const QUANTUM_TICKS: u32 = 10;

/// Estado de uma thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Na fila de prontas.
    Ready,
    /// Executando em alguma CPU.
    Running,
    /// Aguardando `wake_at_ns`.
    Sleeping,
    /// Bloqueada em `join`.
    Blocked,
    /// Chamou `exit`; será recolhida pela próxima thread na mesma CPU.
    Dying,
    /// Terminada; pilha pode ser liberada.
    Dead,
}

struct Inner {
    state: State,
    sp: u64,
    wake_at_ns: u64,
    quantum_left: u32,
    waiters: Vec<Arc<Thread>>,
}

/// Uma thread de kernel.
pub struct Thread {
    /// ID.
    pub id: ThreadId,
    /// Nome.
    pub name: &'static str,
    /// `true` se é a thread idle de uma CPU.
    pub is_idle: bool,
    /// `true` depois de terminar.
    pub finished: AtomicBool,
    /// Vezes que recebeu a CPU.
    pub runs: AtomicU64,
    /// Última CPU em que executou.
    pub last_cpu: AtomicUsize,
    /// Máscara de CPUs permitidas (bit i = CPU i).
    pub affinity: AtomicU64,
    /// Processo dono (threads de kernel: `None`).
    pub process: Option<Arc<crate::process::Process>>,
    stack: Option<stack::Slot>,
    entry: UnsafeCell<Option<Entry>>,
    inner: UnsafeCell<Inner>,
}

// SAFETY: `inner`/`entry` só são acessados com o lock do escalonador detido
// (ou pela própria thread ao iniciar); o restante é atômico/imutável.
unsafe impl Sync for Thread {}
// SAFETY: idem.
unsafe impl Send for Thread {}

impl Thread {
    /// Acesso ao estado mutável. Chamador detém `SCHED`.
    #[allow(clippy::mut_from_ref)]
    unsafe fn inner(&self) -> &mut Inner {
        // SAFETY: contrato da função.
        unsafe { &mut *self.inner.get() }
    }

    /// Estado atual (instantâneo).
    pub fn state(&self) -> State {
        cpu::without_interrupts(|| {
            let _g = SCHED.lock();
            // SAFETY: lock detido.
            unsafe { self.inner().state }
        })
    }

    /// Limites `[base, topo)` da pilha própria (None para threads de boot).
    pub fn stack_bounds(&self) -> Option<(u64, u64)> {
        self.stack.as_ref().map(|s| (s.base, s.top))
    }
}

struct Sched {
    run_queue: Vec<Arc<Thread>>, // FIFO: push no fim, remove do início
    sleepers: Vec<Arc<Thread>>,
    dead: Vec<Arc<Thread>>,
    all: Vec<Arc<Thread>>,
    running: [Option<Arc<Thread>>; MAX_CPUS],
    idle: [Option<Arc<Thread>>; MAX_CPUS],
    prev_for_finish: [Option<Arc<Thread>>; MAX_CPUS],
    switches: u64,
    preemptions: u64,
    spawned: u64,
    reaped: u64,
}

const NONE_THREAD: Option<Arc<Thread>> = None;

static SCHED: SpinLock<Sched> = SpinLock::new(Sched {
    run_queue: Vec::new(),
    sleepers: Vec::new(),
    dead: Vec::new(),
    all: Vec::new(),
    running: [NONE_THREAD; MAX_CPUS],
    idle: [NONE_THREAD; MAX_CPUS],
    prev_for_finish: [NONE_THREAD; MAX_CPUS],
    switches: 0,
    preemptions: 0,
    spawned: 0,
    reaped: 0,
});
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Estatísticas.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Trocas de contexto.
    pub switches: u64,
    /// Preempções pelo timer.
    pub preemptions: u64,
    /// Threads criadas.
    pub spawned: u64,
    /// Threads recolhidas.
    pub reaped: u64,
    /// Threads vivas (inclui idles).
    pub alive: usize,
    /// Prontas na fila.
    pub ready: usize,
    /// Dormindo.
    pub sleeping: usize,
}

/// Estatísticas instantâneas.
pub fn stats() -> Stats {
    cpu::without_interrupts(|| {
        let g = SCHED.lock();
        Stats {
            switches: g.switches,
            preemptions: g.preemptions,
            spawned: g.spawned,
            reaped: g.reaped,
            alive: g.all.len(),
            ready: g.run_queue.len(),
            sleeping: g.sleepers.len(),
        }
    })
}

/// Trocas de contexto realizadas.
pub fn switches() -> u64 {
    stats().switches
}

static KERNEL_PML4: AtomicU64 = AtomicU64::new(0);

/// PML4 do kernel (capturada em `init`).
pub fn kernel_pml4() -> nexo_mm::PhysAddr {
    nexo_mm::PhysAddr::new(KERNEL_PML4.load(Ordering::Relaxed))
}

impl Thread {
    /// Topo da pilha de kernel desta thread (para TSS.RSP0/syscall).
    fn kernel_stack_top(&self, cpu_data: &PerCpu) -> u64 {
        match &self.stack {
            Some(s) => s.top,
            None if self.is_idle => cpu_data.stack_base + cpu_data.stack_size,
            None => nexo_boot_abi::KERNEL_STACK_TOP,
        }
    }
}

fn new_thread(
    name: &'static str,
    is_idle: bool,
    stack: Option<stack::Slot>,
    entry: Option<Entry>,
) -> Arc<Thread> {
    Arc::new(Thread {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        name,
        is_idle,
        finished: AtomicBool::new(false),
        runs: AtomicU64::new(0),
        last_cpu: AtomicUsize::new(usize::MAX),
        affinity: AtomicU64::new(u64::MAX),
        process: None,
        stack,
        entry: UnsafeCell::new(entry),
        inner: UnsafeCell::new(Inner {
            state: State::Ready,
            sp: 0,
            wake_at_ns: 0,
            quantum_left: QUANTUM_TICKS,
            waiters: Vec::new(),
        }),
    })
}

fn set_current(cpu_data: &PerCpu, t: &Arc<Thread>) {
    cpu_data
        .current_thread
        .store(Arc::as_ptr(t) as *mut (), Ordering::Release);
    cpu_data.set_kernel_stack(t.kernel_stack_top(cpu_data));
    t.last_cpu.store(cpu_data.index, Ordering::Relaxed);
    t.runs.fetch_add(1, Ordering::Relaxed);
}

/// Thread atual desta CPU (None antes de `init`).
pub fn current() -> Option<Arc<Thread>> {
    let c = percpu::try_current()?;
    let p = c.current_thread.load(Ordering::Acquire) as *const Thread;
    if p.is_null() {
        return None;
    }
    // SAFETY: a thread atual está viva (mantida em `all`) enquanto ocupa a CPU.
    unsafe {
        Arc::increment_strong_count(p);
        Some(Arc::from_raw(p))
    }
}

/// Nome da thread atual (sem locks; para o caminho de panic).
pub fn current_name() -> &'static str {
    match percpu::try_current() {
        Some(c) => {
            let p = c.current_thread.load(Ordering::Acquire) as *const Thread;
            // SAFETY: ponteiro nulo ou para thread viva.
            if p.is_null() {
                "boot"
            } else {
                // SAFETY: ponteiro para thread viva enquanto ocupa a CPU.
                unsafe { (*p).name }
            }
        }
        None => "boot",
    }
}

/// Registra a thread principal (contexto de boot da BSP) e a idle da BSP; ativa o escalonador.
pub fn init() {
    KERNEL_PML4.store(cpu::read_cr3() & 0x000f_ffff_ffff_f000, Ordering::Relaxed);
    let cpu_data = percpu::current();
    let main = new_thread("main", false, None, None);
    // A thread principal fica na BSP: mantém determinísticos os testes e o tick global.
    main.affinity.store(1, Ordering::Relaxed);
    let idle_stack = stack::alloc().expect("pilha da idle");
    let idle = new_thread("idle/0", true, Some(idle_stack), Some((idle_entry, 0)));
    // SAFETY: pilha recém-alocada e exclusiva.
    unsafe {
        idle.inner().sp = prepare_stack(
            idle_stack_top(&idle),
            thread_main,
            Arc::as_ptr(&idle) as usize,
        );
        main.inner().state = State::Running;
    }
    cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        set_current(cpu_data, &main);
        g.running[cpu_data.index] = Some(main.clone());
        cpu_data
            .idle_thread
            .store(Arc::as_ptr(&idle) as *mut (), Ordering::Release);
        g.idle[cpu_data.index] = Some(idle.clone());
        g.all.push(main);
        g.all.push(idle);
    });
    ACTIVE.store(true, Ordering::Release);
    kinfo!(
        "sched: escalonador preemptivo ativo ({} CPUs, quantum {} ms, pilhas de {} KiB em {:#x})",
        percpu::online_count(),
        QUANTUM_TICKS,
        stack::STACK_SIZE / 1024,
        stack::BASE
    );
}

fn idle_stack_top(t: &Arc<Thread>) -> u64 {
    t.stack.as_ref().map(|s| s.top).unwrap_or(0)
}

/// `true` quando o escalonador está ativo.
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

/// Uma AP torna seu contexto de boot a thread idle da CPU e passa a girar em `hlt`.
pub fn ap_idle_loop() -> ! {
    let cpu_data = percpu::current();
    let idle = new_thread("idle/ap", true, None, None);
    // SAFETY: registro inicial, sem concorrência sobre esta thread.
    unsafe { idle.inner().state = State::Running };
    cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        set_current(cpu_data, &idle);
        g.running[cpu_data.index] = Some(idle.clone());
        cpu_data
            .idle_thread
            .store(Arc::as_ptr(&idle) as *mut (), Ordering::Release);
        g.idle[cpu_data.index] = Some(idle.clone());
        g.all.push(idle);
    });
    idle_body()
}

fn idle_entry(_: usize) {
    idle_body()
}

fn idle_body() -> ! {
    loop {
        cpu::halt();
    }
}

/// Primeira função de toda thread nova: solta o lock herdado da troca e chama a entrada.
extern "C" fn thread_main(arg: usize) -> ! {
    // SAFETY: `arg` é o ponteiro de um `Arc<Thread>` mantido vivo em `all`.
    let me: &Thread = unsafe { &*(arg as *const Thread) };
    finish_switch();
    // SAFETY: IDT e LAPIC prontos; estamos fora de qualquer lock.
    unsafe { cpu::enable_interrupts() };
    // SAFETY: `entry` só é lido aqui, uma única vez, pela própria thread.
    let entry = unsafe { (*me.entry.get()).take() };
    if let Some((f, a)) = entry {
        f(a);
    }
    exit_current()
}

/// Cria uma thread de kernel executando `f(arg)`. Devolve o ID.
pub fn spawn(name: &'static str, f: fn(usize), arg: usize) -> ThreadId {
    spawn_with_affinity(name, f, arg, u64::MAX)
}

/// Cria uma thread restrita às CPUs de `mask` (a afinidade vale desde o primeiro escalonamento).
pub fn spawn_with_affinity(name: &'static str, f: fn(usize), arg: usize, mask: u64) -> ThreadId {
    spawn_full(name, f, arg, mask, None)
}

/// Cria a thread principal de um processo (executa em seu espaço de endereçamento).
pub fn spawn_process_thread(
    name: &'static str,
    f: fn(usize),
    arg: usize,
    process: Arc<crate::process::Process>,
) -> ThreadId {
    spawn_full(name, f, arg, u64::MAX, Some(process))
}

fn spawn_full(
    name: &'static str,
    f: fn(usize),
    arg: usize,
    mask: u64,
    process: Option<Arc<crate::process::Process>>,
) -> ThreadId {
    let slot = stack::alloc().expect("sem slot de pilha");
    let mut t = new_thread(name, false, Some(slot), Some((f, arg)));
    if let Some(p) = process {
        Arc::get_mut(&mut t).expect("thread recem-criada").process = Some(p);
    }
    t.affinity
        .store(if mask == 0 { u64::MAX } else { mask }, Ordering::Relaxed);
    // SAFETY: pilha recém-mapeada e exclusiva.
    unsafe {
        t.inner().sp = prepare_stack(slot.top, thread_main, Arc::as_ptr(&t) as usize);
    }
    let id = t.id;
    cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        g.spawned += 1;
        g.all.push(t.clone());
        g.run_queue.push(t);
        kick_idle_cpu(&g);
    });
    id
}

/// Se alguma outra CPU está ociosa, manda-lhe um RESCHED.
fn kick_idle_cpu(g: &Sched) {
    let me = percpu::try_current().map_or(0, |c| c.index);
    for i in 0..MAX_CPUS {
        if i == me {
            continue;
        }
        if let (Some(r), Some(id)) = (&g.running[i], &g.idle[i])
            && Arc::ptr_eq(r, id)
            && let Some(c) = percpu::get(i)
            && let Some(l) = crate::x86::apic::try_lapic()
        {
            l.send_ipi(c.apic_id, crate::x86::apic::vectors::RESCHED);
            return;
        }
    }
}

/// Escolhe a próxima thread e troca. Chamador: interrupções desabilitadas e `SCHED` detido.
/// `new_state` é o estado que a thread atual assume.
fn schedule_locked(g: nexo_sync::SpinLockGuard<'static, Sched>, new_state: State) {
    let mut g = g;
    let cpu_data = percpu::current();
    let ci = cpu_data.index;
    let cur = g.running[ci].clone().expect("cpu sem thread atual");
    let idle = g.idle[ci].clone().expect("cpu sem idle");
    let next = match g.run_queue.iter().position(|t| allowed_on(t, ci)) {
        Some(i) => g.run_queue.remove(i),
        None => idle.clone(),
    };
    if Arc::ptr_eq(&next, &cur) {
        // Nada melhor para executar: continua.
        // SAFETY: lock detido.
        unsafe {
            let inner = cur.inner();
            inner.state = State::Running;
            inner.quantum_left = QUANTUM_TICKS;
        }
        return;
    }
    // SAFETY: lock detido; `cur` e `next` são distintas.
    unsafe {
        cur.inner().state = new_state;
        let n = next.inner();
        n.state = State::Running;
        n.quantum_left = QUANTUM_TICKS;
    }
    match new_state {
        State::Ready if !cur.is_idle => g.run_queue.push(cur.clone()),
        State::Sleeping => g.sleepers.push(cur.clone()),
        _ => {}
    }
    g.running[ci] = Some(next.clone());
    g.prev_for_finish[ci] = Some(cur.clone());
    set_current(cpu_data, &next);
    g.switches += 1;
    // Espaço de endereçamento: o do processo da próxima thread, ou o do kernel.
    let target = next
        .process
        .as_ref()
        .map_or(kernel_pml4().as_u64(), |p| p.space.root().as_u64());
    if cpu::read_cr3() & 0x000f_ffff_ffff_f000 != target {
        // SAFETY: a metade do kernel é idêntica em todas as PML4s; pilhas e código continuam mapeados.
        unsafe { cpu::write_cr3(target) };
    }
    // SAFETY: lock detido; `sp` só é tocado aqui e na troca.
    let prev_sp = unsafe { &raw mut cur.inner().sp };
    // SAFETY: lock detido; `next` não executa em nenhuma CPU neste instante.
    let next_sp = unsafe { next.inner().sp };
    drop(next);
    drop(cur);
    drop(idle);
    core::mem::forget(g); // o lock atravessa a troca; quem recebe a CPU solta.
    // SAFETY: `prev_sp` aponta para o campo de uma thread viva; `next_sp` foi
    // preparado por `prepare_stack` ou salvo por uma troca anterior.
    unsafe { nexo_switch_context(prev_sp, next_sp) };
    // De volta nesta thread: outra CPU/thread nos entregou a CPU com o lock detido.
    finish_switch();
}

/// Solta o lock herdado e recolhe a thread anterior se ela estava morrendo.
fn finish_switch() {
    let cpu_data = percpu::current();
    // SAFETY: o lock está detido por construção (esquecido antes da troca).
    let g = unsafe { &mut *SCHED.as_ptr() };
    if let Some(prev) = g.prev_for_finish[cpu_data.index].take() {
        // SAFETY: lock detido.
        let st = unsafe { prev.inner().state };
        if st == State::Dying {
            // SAFETY: lock detido.
            let waiters = unsafe {
                let inner = prev.inner();
                inner.state = State::Dead;
                core::mem::take(&mut inner.waiters)
            };
            prev.finished.store(true, Ordering::Release);
            for w in waiters {
                // SAFETY: lock detido.
                unsafe { w.inner().state = State::Ready };
                g.run_queue.push(w);
            }
            g.dead.push(prev);
        }
    }
    // SAFETY: fim da seção crítica iniciada por quem esqueceu o guard.
    unsafe { SCHED.force_unlock() };
}

/// Cede a CPU (a thread continua pronta).
pub fn yield_now() {
    if !is_active() {
        return;
    }
    cpu::without_interrupts(|| {
        let g = SCHED.lock();
        schedule_locked(g, State::Ready);
    });
}

/// Dorme pelo menos `ms` milissegundos de tempo real.
pub fn sleep_ms(ms: u64) {
    if !is_active() || percpu::try_current().is_none() {
        crate::time::sleep_ms(ms);
        return;
    }
    let wake_at = crate::time::monotonic_ns() + ms * 1_000_000;
    cpu::without_interrupts(|| {
        let g = SCHED.lock();
        let cur = g.running[percpu::current().index]
            .clone()
            .expect("thread atual");
        if cur.is_idle {
            drop(g);
            return;
        }
        // SAFETY: lock detido.
        unsafe { cur.inner().wake_at_ns = wake_at };
        schedule_locked(g, State::Sleeping);
    });
    // Garante o tempo mínimo mesmo com granularidade de tick.
    while crate::time::monotonic_ns() < wake_at {
        yield_now();
    }
}

/// Termina a thread atual.
pub fn exit_current() -> ! {
    cpu::disable_interrupts();
    let g = SCHED.lock();
    schedule_locked(g, State::Dying);
    unreachable!("thread morta reescalonada");
}

/// Bloqueia até a thread `id` terminar. Devolve `false` se ela não existe.
pub fn join(id: ThreadId) -> bool {
    loop {
        let (found, done) = cpu::without_interrupts(|| {
            let g = SCHED.lock();
            let Some(t) = g.all.iter().find(|t| t.id == id).cloned() else {
                return (false, true);
            };
            if t.finished.load(Ordering::Acquire) {
                return (true, true);
            }
            let cur = g.running[percpu::current().index]
                .clone()
                .expect("thread atual");
            if cur.is_idle || Arc::ptr_eq(&cur, &t) {
                return (true, false);
            }
            // SAFETY: lock detido.
            unsafe { t.inner().waiters.push(cur.clone()) };
            schedule_locked(g, State::Blocked);
            (true, false)
        });
        if !found {
            return false;
        }
        if done {
            return true;
        }
        // Acordada (ou impossibilitada de bloquear): verifica de novo.
        if is_finished(id) {
            return true;
        }
        yield_now();
    }
}

/// Bloqueia a thread atual liberando `guard` só depois de marcá-la como
/// bloqueada (sob o lock do escalonador): quem chamar [`unpark`] depois de
/// obter o mesmo lock que `guard` protegia nunca perde o wakeup.
pub fn park_with<T>(guard: crate::sync::IrqGuard<'_, T>) {
    let g = SCHED.lock();
    let ci = percpu::current().index;
    let cur = g.running[ci].clone().expect("thread atual");
    if cur.is_idle {
        drop(g);
        return;
    }
    let was_enabled = guard.unlock_keep_irqs_disabled();
    schedule_locked(g, State::Blocked);
    if was_enabled {
        // SAFETY: estado anterior ao bloqueio.
        unsafe { cpu::enable_interrupts() };
    }
}

/// Acorda a thread `id` se estiver bloqueada em `park_with`/`join`.
pub fn unpark(id: ThreadId) {
    cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        let Some(t) = g.all.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        // SAFETY: lock detido.
        let blocked = unsafe { t.inner().state == State::Blocked };
        if blocked {
            // SAFETY: lock detido.
            unsafe { t.inner().state = State::Ready };
            g.run_queue.push(t);
            kick_idle_cpu(&g);
        }
    });
}

/// `true` se a thread terminou ou nunca existiu.
pub fn is_finished(id: ThreadId) -> bool {
    cpu::without_interrupts(|| {
        let g = SCHED.lock();
        g.all
            .iter()
            .find(|t| t.id == id)
            .is_none_or(|t| t.finished.load(Ordering::Acquire))
    })
}

/// Libera pilhas de threads mortas. Devolve quantas.
pub fn reap() -> usize {
    let dead: Vec<Arc<Thread>> = cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        let dead = core::mem::take(&mut g.dead);
        for d in &dead {
            g.all.retain(|t| !Arc::ptr_eq(t, d));
        }
        g.reaped += dead.len() as u64;
        dead
    });
    let n = dead.len();
    for t in dead {
        if let Some(s) = &t.stack {
            stack::free(s);
        }
    }
    n
}

/// Chamado pelo handler do timer em cada CPU (após o EOI, interrupções desabilitadas).
pub fn on_tick() {
    if !is_active() {
        return;
    }
    let Some(cpu_data) = percpu::try_current() else {
        return;
    };
    let mut g = SCHED.lock();
    let ci = cpu_data.index;
    if ci == 0 {
        crate::timer::on_tick();
        let now = crate::time::monotonic_ns();
        let mut i = 0;
        while i < g.sleepers.len() {
            // SAFETY: lock detido.
            let due = unsafe { g.sleepers[i].inner().wake_at_ns <= now };
            if due {
                let t = g.sleepers.remove(i);
                // SAFETY: lock detido.
                unsafe { t.inner().state = State::Ready };
                g.run_queue.push(t);
            } else {
                i += 1;
            }
        }
    }
    let Some(cur) = g.running[ci].clone() else {
        return;
    };
    // SAFETY: lock detido.
    let expired = unsafe {
        let inner = cur.inner();
        inner.quantum_left = inner.quantum_left.saturating_sub(1);
        inner.quantum_left == 0
    };
    let has_work = g.run_queue.iter().any(|t| allowed_on(t, ci));
    if has_work && (cur.is_idle || expired) {
        g.preemptions += 1;
        drop(cur);
        schedule_locked(g, State::Ready);
    }
}

/// Chamado pelo handler da IPI RESCHED (após o EOI).
pub fn on_resched_ipi() {
    if !is_active() {
        return;
    }
    let Some(cpu_data) = percpu::try_current() else {
        return;
    };
    let g = SCHED.lock();
    let ci = cpu_data.index;
    let Some(cur) = g.running[ci].clone() else {
        return;
    };
    if cur.is_idle && g.run_queue.iter().any(|t| allowed_on(t, ci)) {
        drop(cur);
        schedule_locked(g, State::Ready);
    }
}

fn allowed_on(t: &Arc<Thread>, cpu_index: usize) -> bool {
    t.affinity.load(Ordering::Relaxed) & (1u64 << cpu_index.min(63)) != 0
}

/// Restringe a thread `id` às CPUs da máscara. Devolve `false` se não existe.
pub fn set_affinity(id: ThreadId, mask: u64) -> bool {
    if mask == 0 {
        return false;
    }
    cpu::without_interrupts(|| {
        let g = SCHED.lock();
        match g.all.iter().find(|t| t.id == id) {
            Some(t) => {
                t.affinity.store(mask, Ordering::Relaxed);
                true
            }
            None => false,
        }
    })
}

/// Cria uma thread já restrita à CPU `cpu_index`.
pub fn spawn_on(name: &'static str, f: fn(usize), arg: usize, cpu_index: usize) -> ThreadId {
    spawn_with_affinity(name, f, arg, 1u64 << cpu_index.min(63))
}

/// Limites da pilha de thread que contém `addr` (para backtraces).
pub fn stack_bounds_containing(addr: u64) -> Option<(u64, u64)> {
    stack::bounds_containing(addr)
}

/// Pilhas de thread em slots virtuais com guard page.
pub mod stack {
    use super::*;
    use nexo_arch_x86_64::paging::PageFlags;
    use nexo_mm::{PAGE_SIZE, VirtAddr};

    /// Base da região de pilhas.
    pub const BASE: u64 = 0xffff_ffff_b000_0000;
    /// Tamanho do slot (guarda + pilha).
    pub const SLOT: u64 = 0x1_0000;
    /// Bytes mapeados por pilha.
    pub const STACK_SIZE: u64 = 0x8000;
    const SLOTS: usize = 4096;

    /// Um slot alocado.
    #[derive(Clone, Copy, Debug)]
    pub struct Slot {
        /// Índice.
        pub index: usize,
        /// Base mapeada.
        pub base: u64,
        /// Topo (exclusivo, alinhado a 16).
        pub top: u64,
    }

    static USED: SpinLock<[u64; SLOTS / 64]> = SpinLock::new([0; SLOTS / 64]);

    fn slot_base(i: usize) -> u64 {
        BASE + i as u64 * SLOT + (SLOT - STACK_SIZE)
    }

    /// Aloca e mapeia um slot.
    pub fn alloc() -> Option<Slot> {
        let index = cpu::without_interrupts(|| {
            let mut used = USED.lock();
            for (w, word) in used.iter_mut().enumerate() {
                if *word != u64::MAX {
                    let b = word.trailing_ones() as usize;
                    *word |= 1 << b;
                    return Some(w * 64 + b);
                }
            }
            None
        })?;
        let base = slot_base(index);
        let mut off = 0;
        while off < STACK_SIZE {
            if crate::mm::virt::alloc_and_map(VirtAddr::new(base + off), PageFlags::KERNEL_RW)
                .is_err()
            {
                let mut o2 = 0;
                while o2 < off {
                    let _ = crate::mm::virt::unmap_and_free(VirtAddr::new(base + o2));
                    o2 += PAGE_SIZE;
                }
                release(index);
                return None;
            }
            off += PAGE_SIZE;
        }
        Some(Slot {
            index,
            base,
            top: base + STACK_SIZE,
        })
    }

    fn release(index: usize) {
        cpu::without_interrupts(|| {
            let mut used = USED.lock();
            used[index / 64] &= !(1 << (index % 64));
        });
    }

    /// Desmapeia e libera um slot.
    pub fn free(s: &Slot) {
        let mut off = 0;
        while off < STACK_SIZE {
            let _ = crate::mm::virt::unmap_and_free(VirtAddr::new(s.base + off));
            off += PAGE_SIZE;
        }
        release(s.index);
    }

    /// Limites do slot que contém `addr`, se o slot está em uso.
    pub fn bounds_containing(addr: u64) -> Option<(u64, u64)> {
        if !(BASE..BASE + SLOTS as u64 * SLOT).contains(&addr) {
            return None;
        }
        let i = ((addr - BASE) / SLOT) as usize;
        let used = USED.try_lock()?;
        (used[i / 64] & (1 << (i % 64)) != 0).then(|| (slot_base(i), slot_base(i) + STACK_SIZE))
    }

    /// Slots em uso.
    pub fn in_use() -> usize {
        cpu::without_interrupts(|| USED.lock().iter().map(|w| w.count_ones() as usize).sum())
    }
}

//! Tarefas cooperativas de kernel (round-robin, `yield_now`).
//!
//! Preempção, prioridades e SMP pertencem à Fase 1; aqui o objetivo é validar
//! troca de contexto, pilhas separadas e o ciclo de vida (spawn → run → exit →
//! reap) com testes verificáveis por serial.

use alloc::boxed::Box;
use alloc::vec::Vec;
use nexo_arch_x86_64::context::{nexo_switch_context, prepare_stack};
use nexo_arch_x86_64::cpu;
use nexo_sync::SpinLock;

const STACK_SIZE: usize = 32 * 1024;

/// Estado de uma tarefa.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Pronta para executar.
    Ready,
    /// Executando agora.
    Running,
    /// Terminou; pilha pode ser recolhida.
    Finished,
}

struct Task {
    id: usize,
    name: &'static str,
    state: State,
    sp: u64,
    stack: Vec<u8>,
    entry: Option<(fn(usize), usize)>,
}

struct Scheduler {
    /// `Box` garante endereço estável: o trampolim recebe `*mut Task`.
    #[allow(clippy::vec_box)]
    tasks: Vec<Box<Task>>,
    current: usize,
    next_id: usize,
    switches: u64,
}

static SCHED: SpinLock<Option<Scheduler>> = SpinLock::new(None);

/// Registra a tarefa de boot (pilha inicial do kernel) como tarefa 0.
pub fn init() {
    let boot = Box::new(Task {
        id: 0,
        name: "boot",
        state: State::Running,
        sp: 0,
        stack: Vec::new(),
        entry: None,
    });
    *SCHED.lock() = Some(Scheduler {
        tasks: alloc::vec![boot],
        current: 0,
        next_id: 1,
        switches: 0,
    });
    kinfo!(
        "task: escalonador cooperativo pronto (pilhas de {} KiB)",
        STACK_SIZE / 1024
    );
}

extern "C" fn task_main(task_ptr: usize) -> ! {
    let (f, arg) = {
        // SAFETY: `task_ptr` é o endereço estável de um `Box<Task>` vivo.
        let t = unsafe { &mut *(task_ptr as *mut Task) };
        t.entry.take().expect("tarefa sem entrada")
    };
    f(arg);
    exit_current()
}

/// Cria uma tarefa que executa `f(arg)`. Devolve o id.
pub fn spawn(name: &'static str, f: fn(usize), arg: usize) -> usize {
    let mut stack = alloc::vec![0u8; STACK_SIZE + 16];
    let top = (stack.as_mut_ptr() as u64 + stack.len() as u64) & !0xf;
    let mut task = Box::new(Task {
        id: 0,
        name,
        state: State::Ready,
        sp: 0,
        stack,
        entry: Some((f, arg)),
    });
    let task_ptr = &mut *task as *mut Task as usize;
    // SAFETY: `top` está dentro da pilha recém-alocada, alinhado a 16.
    task.sp = unsafe { prepare_stack(top, task_main, task_ptr) };
    cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        let s = g.as_mut().expect("task::init");
        task.id = s.next_id;
        s.next_id += 1;
        let id = task.id;
        s.tasks.push(task);
        kdebug!(
            "task: criada '{}' (id {}, sp {:#x})",
            name,
            id,
            s.tasks.last().unwrap().sp
        );
        id
    })
}

/// Escolhe a próxima tarefa pronta e devolve (ptr para salvar sp, sp de destino).
fn pick_next(s: &mut Scheduler, current_becomes: State) -> Option<(*mut u64, u64)> {
    let n = s.tasks.len();
    let cur = s.current;
    let next = (1..=n)
        .map(|i| (cur + i) % n)
        .find(|&i| s.tasks[i].state == State::Ready)?;
    if next == cur {
        return None;
    }
    s.tasks[cur].state = current_becomes;
    s.tasks[next].state = State::Running;
    s.current = next;
    s.switches += 1;
    Some((&mut s.tasks[cur].sp as *mut u64, s.tasks[next].sp))
}

/// Cede a CPU para a próxima tarefa pronta (se houver).
pub fn yield_now() {
    let switch = cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        let s = g.as_mut()?;
        pick_next(s, State::Ready)
    });
    if let Some((prev, next)) = switch {
        // SAFETY: `prev` aponta para o campo `sp` de um Box estável; `next` é
        // um RSP preparado por `prepare_stack` ou salvo por uma troca anterior.
        unsafe { nexo_switch_context(prev, next) };
    }
}

/// Termina a tarefa atual e nunca retorna.
pub fn exit_current() -> ! {
    let switch = cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        let s = g.as_mut().expect("task::init");
        pick_next(s, State::Finished)
    });
    let (prev, next) = switch.expect("nenhuma tarefa pronta para assumir");
    // SAFETY: ver `yield_now`; esta pilha só é liberada por `reap`, depois.
    unsafe { nexo_switch_context(prev, next) };
    unreachable!("tarefa finalizada foi reescalonada");
}

/// Recolhe tarefas finalizadas (libera pilhas). Devolve quantas.
pub fn reap() -> usize {
    cpu::without_interrupts(|| {
        let mut g = SCHED.lock();
        let s = g.as_mut().expect("task::init");
        let cur = s.current;
        let before = s.tasks.len();
        let mut i = 0;
        let mut removed_before_current = 0;
        s.tasks.retain(|t| {
            let keep = t.state != State::Finished || t.id == 0;
            if !keep && i < cur {
                removed_before_current += 1;
            }
            i += 1;
            keep
        });
        s.current -= removed_before_current;
        before - s.tasks.len()
    })
}

/// `true` se a tarefa `id` terminou (ou já foi recolhida).
pub fn is_finished(id: usize) -> bool {
    cpu::without_interrupts(|| {
        let g = SCHED.lock();
        let Some(s) = g.as_ref() else { return true };
        s.tasks
            .iter()
            .find(|t| t.id == id)
            .is_none_or(|t| t.state == State::Finished)
    })
}

/// Nome da tarefa atual (sem lock quando possível, para o caminho de panic).
pub fn current_name() -> &'static str {
    match SCHED.try_lock() {
        Some(g) => g.as_ref().map_or("boot", |s| s.tasks[s.current].name),
        None => "?",
    }
}

/// Número de trocas de contexto realizadas.
pub fn switches() -> u64 {
    cpu::without_interrupts(|| SCHED.lock().as_ref().map_or(0, |s| s.switches))
}

/// Limites da pilha de tarefa que contém `addr` (para backtraces).
pub fn stack_bounds_containing(addr: u64) -> Option<(u64, u64)> {
    let g = SCHED.try_lock()?;
    let s = g.as_ref()?;
    s.tasks.iter().find_map(|t| {
        let lo = t.stack.as_ptr() as u64;
        let hi = lo + t.stack.len() as u64;
        (lo..hi).contains(&addr).then_some((lo, hi))
    })
}

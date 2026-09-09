//! Processos de usuário: espaço de endereçamento próprio (PML4 com a metade
//! do kernel copiada), carga de ELF, pilha de usuário e thread principal que
//! entra em ring 3. Um processo tem, por ora, uma única thread.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::paging::{MapError, Mapper, PageFlags, PageTableEntry};
use nexo_elf::ElfFile;
use nexo_mm::{FrameAllocator, PAGE_SIZE, PhysAddr, VirtAddr, align_down, align_up};
use nexo_syscall_abi::{EXIT_KILLED, USER_ADDRESS_LIMIT};

use crate::mm::phys;
use crate::mm::virt::{PhysMap, phys_to_virt};
use crate::sched;
use crate::sync::IrqLock;

/// Identificador de processo.
pub type Pid = u64;

/// Topo da pilha de usuário (exclusivo).
pub const USER_STACK_TOP: u64 = 0x0000_7fff_fff0_0000;
/// Tamanho da pilha de usuário.
pub const USER_STACK_SIZE: u64 = 256 * 1024;
/// Janela do ASLR da pilha: o topo real de cada processo fica até 1 GiB abaixo de
/// [`USER_STACK_TOP`], alinhado a página (18 bits de entropia).
pub const USER_STACK_WINDOW: u64 = 1 << 30;
/// Janela do ASLR dos mapeamentos: a região de dispositivos/objetos de memória de cada
/// processo começa até 4 GiB depois de `USER_DEVICE_REGION` (20 bits de entropia).
pub const USER_MAP_WINDOW: u64 = 4 << 30;
/// Base mínima de um PIE e janela do ASLR de código: a base é aleatória (alinhada a página)
/// em `[USER_CODE_BASE, USER_CODE_BASE + USER_CODE_WINDOW)` (28 bits de entropia).
pub const USER_CODE_BASE: u64 = 0x1000_0000;
/// Ver [`USER_CODE_BASE`].
pub const USER_CODE_WINDOW: u64 = 1 << 40;

/// Espaço de endereçamento de um processo.
pub struct AddressSpace {
    root: PhysAddr,
    frames: IrqLock<Vec<PhysAddr>>,
}

struct Recording<'a>(&'a IrqLock<Vec<PhysAddr>>);

impl FrameAllocator for Recording<'_> {
    fn allocate_frame(&mut self) -> Option<PhysAddr> {
        let f = phys::allocate_zeroed_frame()?;
        self.0.lock().push(f);
        Some(f)
    }
    fn deallocate_frame(&mut self, frame: PhysAddr) {
        let _ = phys::free_frame(frame);
    }
}

impl AddressSpace {
    /// Cria um espaço com a metade do kernel compartilhada e a metade do usuário vazia.
    pub fn new() -> Option<AddressSpace> {
        let root = phys::allocate_zeroed_frame()?;
        let kernel_root = sched::kernel_pml4();
        let src = phys_to_virt(kernel_root).as_ptr::<PageTableEntry>();
        let dst = phys_to_virt(root).as_mut_ptr::<PageTableEntry>();
        // SAFETY: ambas as tabelas estão no physmap; copia as 256 entradas da metade alta.
        unsafe { core::ptr::copy_nonoverlapping(src.add(256), dst.add(256), 256) };
        Some(AddressSpace {
            root,
            frames: IrqLock::new(Vec::new()),
        })
    }

    /// Endereço físico da PML4.
    pub fn root(&self) -> PhysAddr {
        self.root
    }

    fn mapper(&self) -> Mapper<PhysMap> {
        // SAFETY: PML4 válida construída em `new`.
        unsafe { Mapper::new(self.root, PhysMap) }
    }

    /// Mapeia um quadro zerado em `virt` (metade do usuário) com `flags | USER`.
    pub fn map_user_page(&self, virt: VirtAddr, flags: PageFlags) -> Result<PhysAddr, MapError> {
        if virt.as_u64() >= USER_ADDRESS_LIMIT {
            return Err(MapError::Unaligned(virt));
        }
        let mut alloc = Recording(&self.frames);
        let frame = alloc.allocate_frame().ok_or(MapError::OutOfFrames)?;
        self.mapper().map_4k(
            virt,
            frame,
            flags | PageFlags::USER | PageFlags::PRESENT,
            &mut alloc,
        )?;
        Ok(frame)
    }

    /// Mapeia um quadro físico **não possuído** (MMIO) em `virt`, sem cache.
    pub fn map_user_mmio(&self, virt: VirtAddr, phys: PhysAddr) -> Result<(), MapError> {
        if virt.as_u64() >= USER_ADDRESS_LIMIT {
            return Err(MapError::Unaligned(virt));
        }
        let mut alloc = Recording(&self.frames);
        let flags =
            PageFlags::KERNEL_RW | PageFlags::USER | PageFlags::NO_CACHE | PageFlags::WRITE_THROUGH;
        self.mapper().map_4k(virt, phys, flags, &mut alloc)
    }

    /// Mapeia um quadro físico **não possuído** (compartilhado) em `virt`, cacheável `USER|RW`.
    pub fn map_user_shared(&self, virt: VirtAddr, phys: PhysAddr) -> Result<(), MapError> {
        if virt.as_u64() >= USER_ADDRESS_LIMIT {
            return Err(MapError::Unaligned(virt));
        }
        let mut alloc = Recording(&self.frames);
        let flags = PageFlags::KERNEL_RW | PageFlags::USER;
        self.mapper().map_4k(virt, phys, flags, &mut alloc)
    }

    /// Desmapeia `[base, base+len)` (páginas **compartilhadas/não possuídas**, mapeadas por
    /// `map_user_shared`/`map_user_mmio`): limpa as PTEs e invalida o TLB, **sem** liberar os
    /// quadros físicos (que pertencem ao objeto de memória ou ao dispositivo). Páginas já
    /// desmapeadas são ignoradas.
    pub fn unmap_user_shared(&self, base: VirtAddr, len: u64) -> Result<(), MapError> {
        if base.as_u64() >= USER_ADDRESS_LIMIT || base.as_u64().checked_add(len).is_none() {
            return Err(MapError::Unaligned(base));
        }
        let mut mapper = self.mapper();
        let pages = align_up(len, PAGE_SIZE) / PAGE_SIZE;
        for i in 0..pages {
            let v = base.add(i * PAGE_SIZE);
            match mapper.unmap_4k(v) {
                // unmap_4k devolve o quadro mas NÃO o libera: é compartilhado.
                Ok(_) => cpu::invlpg(v.as_u64()),
                Err(MapError::NotMapped(_)) => {}
                Err(e) => return Err(e),
            }
        }
        // As threads do processo podem estar em outra CPU: invalida o TLB delas também.
        crate::x86::smp::flush_tlb_others();
        Ok(())
    }

    /// Endereço físico mapeado em `virt` (páginas de 4 KiB).
    pub fn translate(&self, virt: VirtAddr) -> Option<PhysAddr> {
        self.mapper().translate(virt).map(|t| t.phys)
    }

    /// Escreve `data` em `virt` (que deve estar mapeado) pelo physmap.
    pub fn write(&self, virt: VirtAddr, data: &[u8]) -> Result<(), MapError> {
        let mut off = 0usize;
        while off < data.len() {
            let v = virt.add(off as u64);
            let phys = self.translate(v).ok_or(MapError::NotMapped(v))?;
            let chunk = (PAGE_SIZE - v.page_offset()) as usize;
            let n = chunk.min(data.len() - off);
            let dst = phys_to_virt(phys).as_mut_ptr::<u8>();
            // SAFETY: página mapeada e exclusiva do processo; `n` respeita o limite da página.
            unsafe { core::ptr::copy_nonoverlapping(data[off..].as_ptr(), dst, n) };
            off += n;
        }
        Ok(())
    }

    /// Quadros (páginas + tabelas) pertencentes ao espaço.
    pub fn frame_count(&self) -> usize {
        self.frames.lock().len()
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        if cpu::read_cr3() & 0x000f_ffff_ffff_f000 == self.root.as_u64() {
            // SAFETY: a PML4 do kernel mapeia tudo que o kernel usa.
            unsafe { cpu::write_cr3(sched::kernel_pml4().as_u64()) };
        }
        let frames = core::mem::take(&mut *self.frames.lock());
        for f in frames {
            let _ = phys::free_frame(f);
        }
        let _ = phys::free_frame(self.root);
    }
}

/// Um processo.
/// Job: grupo de processos com morte em cascata. Um processo entra num job por `job_attach`
/// ou por herança (os processos que um membro cria nascem no job dele). `kill` mata todos os
/// membros vivos: fecha as tabelas de handles (os pares veem `PeerClosed`), marca `killed` e
/// acorda a thread principal; cada membro sai com [`EXIT_KILLED`] na próxima syscall ou ao
/// acordar de uma espera. Não há hierarquia de jobs (um processo pertence a no máximo um).
pub struct Job {
    members: IrqLock<Vec<alloc::sync::Weak<Process>>>,
}

impl Job {
    /// Job vazio.
    pub fn new() -> Job {
        Job {
            members: IrqLock::new(Vec::new()),
        }
    }

    /// Anexa `p` (e marca nele o job, para a herança).
    pub fn attach(self: &Arc<Job>, p: &Arc<Process>) {
        let mut m = self.members.lock();
        m.retain(|w| w.strong_count() > 0);
        if !m.iter().any(|w| w.as_ptr() == Arc::as_ptr(p)) {
            m.push(Arc::downgrade(p));
        }
        drop(m);
        *p.job.lock() = Some(self.clone());
    }

    /// Mata todos os membros vivos. O chamador, se for membro, morre ao sair da syscall.
    pub fn kill(&self) {
        let members: Vec<Arc<Process>> = self
            .members
            .lock()
            .iter()
            .filter_map(|w| w.upgrade())
            .collect();
        for p in members {
            if p.exited.load(Ordering::Acquire) || p.killed.swap(true, Ordering::AcqRel) {
                continue;
            }
            // fecha os handles já aqui: os pares veem PeerClosed sem esperar o membro sair
            let table = core::mem::take(&mut *p.handles.lock());
            drop(table);
            crate::ipc::collect_unreachable();
            for t in p.threads.lock().iter().copied() {
                sched::unpark(t);
            }
        }
    }
}

impl Default for Job {
    fn default() -> Self {
        Self::new()
    }
}

/// Cria uma thread de usuário em `p`: pilha própria de [`USER_STACK_SIZE`] numa região do
/// processo (endereço aleatório, guard page abaixo), entrada `entry` com `RDI = arg`.
pub fn create_user_thread(
    p: &Arc<Process>,
    entry: u64,
    arg: u64,
) -> Result<crate::sched::ThreadId, &'static str> {
    if entry == 0 || entry >= USER_ADDRESS_LIMIT {
        return Err("entrada fora da faixa de usuario");
    }
    let base = p.reserve_device_region(USER_STACK_SIZE + PAGE_SIZE) + PAGE_SIZE; // guard abaixo
    let top = base + USER_STACK_SIZE;
    let mut v = base;
    while v < top {
        p.space
            .map_user_page(VirtAddr::new(v), PageFlags::KERNEL_RW)
            .map_err(|_| "sem memoria para a pilha da thread")?;
        v += PAGE_SIZE;
    }
    let start = alloc::boxed::Box::new(UserStart {
        entry,
        user_sp: top - 8,
        arg,
    });
    let tid = sched::spawn_process_thread(
        "uthread",
        user_thread_main,
        alloc::boxed::Box::into_raw(start) as usize,
        p.clone(),
    );
    p.threads.lock().push(tid);
    Ok(tid)
}

/// Termina a thread atual; se era a última viva do processo, termina o processo com 0.
pub fn thread_exit_current() -> ! {
    if let Some(p) = current() {
        let me = sched::current().map_or(usize::MAX, |t| t.id);
        let mut threads = p.threads.lock();
        threads.retain(|t| *t != me);
        let last = threads.is_empty();
        drop(threads);
        if last {
            drop(p);
            exit_current(0, None);
        }
    }
    sched::exit_current()
}

/// `true` se o processo atual foi morto pelo job (deve sair na próxima oportunidade).
pub fn killed_current() -> bool {
    current().is_some_and(|p| p.killed.load(Ordering::Acquire))
}

pub struct Process {
    /// PID.
    pub pid: Pid,
    /// Nome.
    pub name: &'static str,
    /// Espaço de endereçamento.
    pub space: AddressSpace,
    /// Thread principal.
    pub main_thread: AtomicUsize,
    /// Código de saída (válido após `exited`).
    pub exit_code: AtomicI64,
    /// `true` depois de encerrar.
    pub exited: AtomicBool,
    /// Syscalls atendidas.
    pub syscalls: AtomicU64,
    /// Páginas de memória compartilhável criadas e ainda vivas (quota; ver ABI).
    pub shm_pages: AtomicU64,
    /// Motivo, se encerrado pelo kernel.
    pub kill_reason: IrqLock<Option<&'static str>>,
    /// Handles do processo.
    pub handles: IrqLock<crate::ipc::HandleTable>,
    /// Threads bloqueadas em `wait` sobre este processo.
    pub exit_waiters: IrqLock<Vec<crate::sched::ThreadId>>,
    /// Próximo endereço livre na região de dispositivos (MMIO/DMA) do processo — começa num
    /// deslocamento aleatório (ASLR dos mapeamentos).
    pub device_next: AtomicU64,
    /// Topo da pilha de usuário deste processo (aleatório — ASLR da pilha).
    pub stack_top: u64,
    /// Job a que pertence (herdado por quem ele criar), se algum.
    pub job: IrqLock<Option<Arc<Job>>>,
    /// Morto pelo job (ou o processo já terminou por outra thread): sai com `EXIT_KILLED` na
    /// próxima syscall ou ao acordar de uma espera.
    pub killed: AtomicBool,
    /// Threads vivas do processo (a principal e as de `thread_create`).
    pub threads: IrqLock<Vec<crate::sched::ThreadId>>,
    /// Tempo de CPU consumido por todas as threads do processo (ns), creditado nas trocas
    /// de contexto. Base das quotas de CPU.
    pub cpu_ns: AtomicU64,
}

static TABLE: IrqLock<Vec<Arc<Process>>> = IrqLock::new(Vec::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);
static USER_LOGS: AtomicU64 = AtomicU64::new(0);
static LIVE_MAX: AtomicUsize = AtomicUsize::new(0);
static SPAWNED: AtomicU64 = AtomicU64::new(0);

/// Máximo de processos vivos simultaneamente desde o boot.
pub fn live_max() -> usize {
    LIVE_MAX.load(Ordering::Relaxed)
}

/// Processos criados desde o boot.
pub fn spawned_total() -> u64 {
    SPAWNED.load(Ordering::Relaxed)
}

/// Cria um processo a partir do membro `name` do initrd.
pub fn spawn_named(
    name: &str,
    arg: u64,
    handles: Vec<crate::ipc::Handle>,
) -> Result<Arc<Process>, &'static str> {
    let elf = crate::initrd::find(name).ok_or("programa nao encontrado no initrd")?;
    // O nome precisa viver para sempre (é `&'static` na thread): usa a cópia do initrd.
    let static_name: &'static str = crate::initrd_name(name).unwrap_or("?");
    spawn_elf_with_handles(static_name, elf, arg, handles)
}

/// Cria um processo a partir de um ELF **em memória** (aplicativos instalados, fora do initrd).
/// A carga é síncrona (os segmentos são copiados para o espaço novo durante a chamada), então o
/// chamador pode descartar os bytes ao voltar. Mesmas validações do initrd (faixa, W^X).
pub fn spawn_bytes(
    elf: &[u8],
    arg: u64,
    handles: Vec<crate::ipc::Handle>,
) -> Result<Arc<Process>, &'static str> {
    spawn_elf_with_handles("app", elf, arg, handles)
}

struct UserStart {
    entry: u64,
    user_sp: u64,
    arg: u64,
}

fn user_thread_main(ptr: usize) {
    // SAFETY: ponteiro de um `Box<UserStart>` vazado por `spawn_elf`.
    let start = unsafe { alloc::boxed::Box::from_raw(ptr as *mut UserStart) };
    let (entry, sp, arg) = (start.entry, start.user_sp, start.arg);
    drop(start); // o Arc<Process> vive em Thread.process
    // SAFETY: entrada e pilha mapeadas com USER; CR3 do processo já ativo; gs:[8] apontando
    // para a pilha de kernel desta thread (set_current).
    unsafe { nexo_arch_x86_64::syscall::enter_user(entry, sp, arg) }
}

/// Cria um processo a partir de um ELF estático (`RDI = arg`), entregando `handles` nos primeiros índices da tabela.
pub fn spawn_elf_with_handles(
    name: &'static str,
    elf_bytes: &[u8],
    arg: u64,
    handles: Vec<crate::ipc::Handle>,
) -> Result<Arc<Process>, &'static str> {
    let elf = ElfFile::parse_any(elf_bytes).map_err(|_| "ELF invalido")?;
    let space = AddressSpace::new().ok_or("sem memoria para o espaco")?;
    // PIE: base aleatória (ASLR de código); ET_EXEC: endereços do arquivo (base 0)
    let base = if elf.is_dyn() {
        USER_CODE_BASE + crate::aslr::page_offset(USER_CODE_WINDOW / PAGE_SIZE)
    } else {
        0
    };
    let (lo, hi) = elf.address_range().ok_or("ELF sem segmentos")?;
    let (lo, hi) = (
        lo.checked_add(base)
            .ok_or("segmentos fora da faixa de usuario")?,
        hi.checked_add(base)
            .ok_or("segmentos fora da faixa de usuario")?,
    );
    if lo < PAGE_SIZE || hi > USER_STACK_TOP - USER_STACK_WINDOW - USER_STACK_SIZE - PAGE_SIZE {
        return Err("segmentos fora da faixa de usuario");
    }
    let entry = elf.entry.wrapping_add(base);
    for ph in elf.load_segments() {
        if ph.writable() && ph.executable() {
            return Err("segmento W+X");
        }
        let data = elf
            .segment_data(&ph)
            .map_err(|_| "segmento fora do arquivo")?;
        let mut flags = PageFlags::PRESENT;
        if ph.writable() {
            flags |= PageFlags::WRITABLE;
        }
        if !ph.executable() {
            flags |= PageFlags::NO_EXECUTE;
        }
        let vstart = align_down(ph.p_vaddr + base, PAGE_SIZE);
        let vend = align_up(ph.p_vaddr + base + ph.p_memsz, PAGE_SIZE);
        let mut v = vstart;
        while v < vend {
            match space.map_user_page(VirtAddr::new(v), flags) {
                Ok(_) => {}
                Err(MapError::AlreadyMapped(_)) => {}
                Err(_) => return Err("falha ao mapear segmento"),
            }
            v += PAGE_SIZE;
        }
        space
            .write(VirtAddr::new(ph.p_vaddr + base), data)
            .map_err(|_| "falha ao copiar segmento")?;
    }
    if !(lo..hi).contains(&entry) {
        return Err("entrada fora dos segmentos");
    }
    // PIE: aplica as relocações (só R_X86_64_RELATIVE existe num PIE estático: base + adendo)
    if elf.is_dyn() {
        let relas = elf
            .relocations()
            .map_err(|_| "tabela de relocacao invalida")?;
        for r in relas {
            if r.kind != nexo_elf::R_X86_64_RELATIVE {
                return Err("relocacao nao suportada");
            }
            let at = r.offset.wrapping_add(base);
            if at < lo || at.checked_add(8).is_none_or(|end| end > hi) {
                return Err("relocacao fora dos segmentos");
            }
            let val = (base as i64).wrapping_add(r.addend) as u64;
            space
                .write(VirtAddr::new(at), &val.to_le_bytes())
                .map_err(|_| "falha ao aplicar relocacao")?;
        }
    }
    // ASLR: o topo da pilha é aleatório (alinhado a página) numa janela de 1 GiB abaixo de
    // `USER_STACK_TOP` — nada no espaço de usuário depende do endereço fixo (a pilha chega em RSP)
    let stack_top = USER_STACK_TOP - crate::aslr::page_offset(USER_STACK_WINDOW / PAGE_SIZE);
    let mut v = stack_top - USER_STACK_SIZE;
    while v < stack_top {
        space
            .map_user_page(VirtAddr::new(v), PageFlags::KERNEL_RW)
            .map_err(|_| "sem memoria para a pilha")?;
        v += PAGE_SIZE;
    }
    let process = Arc::new(Process {
        pid: NEXT_PID.fetch_add(1, Ordering::Relaxed),
        name,
        space,
        main_thread: AtomicUsize::new(0),
        exit_code: AtomicI64::new(0),
        exited: AtomicBool::new(false),
        syscalls: AtomicU64::new(0),
        shm_pages: AtomicU64::new(0),
        kill_reason: IrqLock::new(None),
        handles: IrqLock::new(crate::ipc::HandleTable::new()),
        exit_waiters: IrqLock::new(Vec::new()),
        device_next: AtomicU64::new(
            nexo_syscall_abi::USER_DEVICE_REGION
                + crate::aslr::page_offset(USER_MAP_WINDOW / PAGE_SIZE),
        ),
        stack_top,
        // herda o job de quem cria (um app em segundo plano puxa os filhos para o job dele)
        job: IrqLock::new(current().and_then(|c| c.job.lock().clone())),
        killed: AtomicBool::new(false),
        threads: IrqLock::new(Vec::new()),
        cpu_ns: AtomicU64::new(0),
    });
    // (o guard do `lock()` não pode viver dentro do `if let`: `attach` volta a travar `job`)
    let herdado = process.job.lock().clone();
    if let Some(job) = herdado {
        job.attach(&process);
    }
    {
        let mut table = process.handles.lock();
        for h in handles {
            table.insert(h).map_err(|_| "tabela de handles cheia")?;
        }
    }
    {
        let mut t = TABLE.lock();
        t.push(process.clone());
        LIVE_MAX.fetch_max(t.len(), Ordering::Relaxed);
    }
    SPAWNED.fetch_add(1, Ordering::Relaxed);
    let start = alloc::boxed::Box::new(UserStart {
        entry,
        user_sp: stack_top - 8,
        arg,
    });
    let tid = sched::spawn_process_thread(
        name,
        user_thread_main,
        alloc::boxed::Box::into_raw(start) as usize,
        process.clone(),
    );
    process.main_thread.store(tid, Ordering::Release);
    process.threads.lock().push(tid);
    kinfo!(
        "process: '{}' pid {} entry {:#x} ({} quadros) thread {} arg {} pilha {:#x}",
        name,
        process.pid,
        entry,
        process.space.frame_count(),
        tid,
        arg,
        process.stack_top,
    );
    Ok(process)
}

impl Process {
    /// Reserva `len` bytes (múltiplo de página) na região de dispositivos; devolve a base.
    pub fn reserve_device_region(&self, len: u64) -> u64 {
        self.device_next
            .fetch_add(align_up(len, PAGE_SIZE), Ordering::Relaxed)
    }
}

/// Processo da thread atual, se houver.
pub fn current() -> Option<Arc<Process>> {
    sched::current().and_then(|t| t.process.clone())
}

/// Encerra o processo atual com `code` (e motivo, quando morto pelo kernel). Nunca retorna.
pub fn exit_current(code: i64, reason: Option<&'static str>) -> ! {
    if let Some(p) = current() {
        if p.exited.swap(true, Ordering::AcqRel) {
            // outra thread já encerrou o processo: esta só morre
            sched::exit_current();
        }
        p.exit_code.store(code, Ordering::Release);
        *p.kill_reason.lock() = reason;
        // as outras threads do processo morrem na próxima syscall ou ao acordar
        p.killed.store(true, Ordering::Release);
        let me = sched::current().map_or(usize::MAX, |t| t.id);
        for t in p.threads.lock().iter().copied().filter(|t| *t != me) {
            sched::unpark(t);
        }
        // Fecha os handles já aqui: pares de canal veem PeerClosed sem esperar o reap.
        let table = core::mem::take(&mut *p.handles.lock());
        drop(table);
        TABLE.lock().retain(|q| q.pid != p.pid);
        // Pontas de canal que só as mensagens pendentes referenciam (ciclos) são fechadas aqui.
        crate::ipc::collect_unreachable();
        let waiters = core::mem::take(&mut *p.exit_waiters.lock());
        for w in waiters {
            sched::unpark(w);
        }
        if reason.is_some() {
            kwarn!(
                "process: pid {} '{}' encerrado pelo kernel: {}",
                p.pid,
                p.name,
                reason.unwrap_or("")
            );
        } else {
            kinfo!("process: pid {} '{}' saiu com {}", p.pid, p.name, code);
        }
    }
    sched::exit_current()
}

/// Mata o processo atual por falha em modo usuário.
pub fn kill_current(reason: &'static str) -> ! {
    exit_current(EXIT_KILLED, Some(reason))
}

/// Bloqueia até `p` terminar; devolve o código de saída. Não recolhe threads.
pub fn wait_process(p: &Arc<Process>) -> i64 {
    loop {
        if killed_current() {
            return EXIT_KILLED; // morto pelo job enquanto esperava: sai na volta da syscall
        }
        let mut waiters = p.exit_waiters.lock();
        if p.exited.load(Ordering::Acquire) {
            break;
        }
        let Some(me) = sched::current().map(|t| t.id) else {
            break;
        };
        waiters.push(me);
        sched::park_with(waiters);
    }
    // Garante que TODAS as threads terminaram de sair (pilhas fora de uso).
    let tids: Vec<crate::sched::ThreadId> = p.threads.lock().clone();
    for tid in tids {
        sched::join(tid);
    }
    p.exit_code.load(Ordering::Acquire)
}

/// Aguarda `p` terminar e recolhe a thread principal; devolve o código de saída.
pub fn wait_and_reap(p: &Arc<Process>) -> i64 {
    let code = wait_process(p);
    sched::reap();
    code
}

/// Processos vivos (ainda não terminados).
pub fn count() -> usize {
    TABLE.lock().len()
}

/// Executa `f` para cada processo vivo (sobre um instantâneo da tabela).
pub fn for_each_live(mut f: impl FnMut(&Arc<Process>)) {
    let snapshot: Vec<Arc<Process>> = TABLE.lock().clone();
    for p in &snapshot {
        f(p);
    }
}

/// Mensagens de `SYS_LOG` recebidas.
pub fn user_log_count() -> u64 {
    USER_LOGS.load(Ordering::Relaxed)
}

/// Registra uma linha de log vinda do usuário.
pub fn note_user_log() {
    USER_LOGS.fetch_add(1, Ordering::Relaxed);
}

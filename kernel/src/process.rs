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
pub const USER_STACK_SIZE: u64 = 64 * 1024;

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
    /// Motivo, se encerrado pelo kernel.
    pub kill_reason: IrqLock<Option<&'static str>>,
    /// Handles do processo.
    pub handles: IrqLock<crate::ipc::HandleTable>,
    /// Threads bloqueadas em `wait` sobre este processo.
    pub exit_waiters: IrqLock<Vec<crate::sched::ThreadId>>,
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
    let elf = ElfFile::parse(elf_bytes).map_err(|_| "ELF invalido")?;
    let space = AddressSpace::new().ok_or("sem memoria para o espaco")?;
    let (lo, hi) = elf.address_range().ok_or("ELF sem segmentos")?;
    if lo < PAGE_SIZE || hi > USER_STACK_TOP - USER_STACK_SIZE - PAGE_SIZE {
        return Err("segmentos fora da faixa de usuario");
    }
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
        let vstart = align_down(ph.p_vaddr, PAGE_SIZE);
        let vend = align_up(ph.p_vaddr + ph.p_memsz, PAGE_SIZE);
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
            .write(VirtAddr::new(ph.p_vaddr), data)
            .map_err(|_| "falha ao copiar segmento")?;
    }
    if !(lo..hi).contains(&elf.entry) {
        return Err("entrada fora dos segmentos");
    }
    let mut v = USER_STACK_TOP - USER_STACK_SIZE;
    while v < USER_STACK_TOP {
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
        kill_reason: IrqLock::new(None),
        handles: IrqLock::new(crate::ipc::HandleTable::new()),
        exit_waiters: IrqLock::new(Vec::new()),
    });
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
        entry: elf.entry,
        user_sp: USER_STACK_TOP - 8,
        arg,
    });
    let tid = sched::spawn_process_thread(
        name,
        user_thread_main,
        alloc::boxed::Box::into_raw(start) as usize,
        process.clone(),
    );
    process.main_thread.store(tid, Ordering::Release);
    kinfo!(
        "process: '{}' pid {} entry {:#x} ({} quadros) thread {} arg {}",
        name,
        process.pid,
        elf.entry,
        process.space.frame_count(),
        tid,
        arg
    );
    Ok(process)
}

/// Processo da thread atual, se houver.
pub fn current() -> Option<Arc<Process>> {
    sched::current().and_then(|t| t.process.clone())
}

/// Encerra o processo atual com `code` (e motivo, quando morto pelo kernel). Nunca retorna.
pub fn exit_current(code: i64, reason: Option<&'static str>) -> ! {
    if let Some(p) = current() {
        p.exit_code.store(code, Ordering::Release);
        *p.kill_reason.lock() = reason;
        p.exited.store(true, Ordering::Release);
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
    // Garante que a thread principal terminou de sair (pilha fora de uso).
    let tid = p.main_thread.load(Ordering::Acquire);
    sched::join(tid);
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

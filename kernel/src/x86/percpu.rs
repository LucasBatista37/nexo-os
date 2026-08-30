//! Dados por CPU: GDT/TSS/pilha de #DF próprias, contadores e o ponteiro
//! `self` acessível via `gs:[0]`.

use crate::sync::IrqLock;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::gdt::{GlobalDescriptorTable, TaskStateSegment, load_tss};

use crate::acpi::MAX_CPUS;

const DF_STACK_SIZE: usize = 16 * 1024;

#[repr(C, align(16))]
struct Stack([u8; DF_STACK_SIZE]);

/// Estrutura por CPU. `self_ptr` **deve** ser o primeiro campo (`gs:[0]`).
#[repr(C)]
pub struct PerCpu {
    self_ptr: *const PerCpu,
    /// Índice lógico (0 = BSP).
    pub index: usize,
    /// APIC ID.
    pub apic_id: u32,
    /// `true` quando a CPU terminou a inicialização.
    pub online: AtomicBool,
    /// Interrupções do timer local recebidas.
    pub timer_irqs: AtomicU64,
    /// IPIs recebidas.
    pub ipis: AtomicU64,
    /// Base virtual e tamanho da pilha principal desta CPU.
    pub stack_base: u64,
    /// Tamanho da pilha principal.
    pub stack_size: u64,
    /// Thread em execução nesta CPU (`*const sched::Thread`).
    pub current_thread: AtomicPtr<()>,
    /// Thread idle desta CPU.
    pub idle_thread: AtomicPtr<()>,
    gdt: GlobalDescriptorTable,
    tss: TaskStateSegment,
    df_stack: Box<Stack>,
}

// SAFETY: cada CPU só acessa a própria estrutura de forma mutável durante a
// inicialização; depois disso apenas campos atômicos são alterados.
unsafe impl Sync for PerCpu {}
// SAFETY: a estrutura é vazada para 'static e nunca movida após `allocate`.
unsafe impl Send for PerCpu {}

static CPUS: IrqLock<[Option<&'static PerCpu>; MAX_CPUS]> = IrqLock::new([None; MAX_CPUS]);
static ENABLED: AtomicBool = AtomicBool::new(false);
static ONLINE: AtomicUsize = AtomicUsize::new(0);

impl PerCpu {
    /// Aloca a estrutura (vazada para `'static`) com GDT/TSS ainda não carregadas.
    pub fn allocate(
        index: usize,
        apic_id: u32,
        stack_base: u64,
        stack_size: u64,
    ) -> &'static PerCpu {
        let cpu: &'static mut PerCpu = Box::leak(Box::new(PerCpu {
            self_ptr: core::ptr::null(),
            index,
            apic_id,
            online: AtomicBool::new(false),
            timer_irqs: AtomicU64::new(0),
            ipis: AtomicU64::new(0),
            stack_base,
            stack_size,
            current_thread: AtomicPtr::new(core::ptr::null_mut()),
            idle_thread: AtomicPtr::new(core::ptr::null_mut()),
            gdt: GlobalDescriptorTable::new(),
            tss: TaskStateSegment::new(),
            df_stack: Box::new(Stack([0; DF_STACK_SIZE])),
        }));
        cpu.self_ptr = cpu as *const PerCpu;
        cpu.tss.interrupt_stack_table[0] = cpu.df_stack_bounds().1;
        cpu.tss.privilege_stack_table[0] = stack_base + stack_size;
        cpu.gdt.add_kernel_code();
        cpu.gdt.add_kernel_data();
        let tss: &'static TaskStateSegment = &cpu.tss;
        cpu.gdt.add_tss(tss);
        let mut table = CPUS.lock();
        table[index] = Some(cpu);
        cpu
    }

    /// Limites da pilha de double fault.
    pub fn df_stack_bounds(&self) -> (u64, u64) {
        let base = self.df_stack.0.as_ptr() as u64;
        (base, base + DF_STACK_SIZE as u64)
    }

    /// Carrega GDT/TSS/GS desta CPU. Deve ser chamado pela própria CPU.
    ///
    /// # Safety
    /// Uma única vez por CPU, durante a inicialização dela.
    pub unsafe fn activate(&'static self) {
        // SAFETY: GDT com código 0x08, dados 0x10 e TSS 0x18; estrutura é 'static.
        unsafe {
            self.gdt.load();
            load_tss(nexo_arch_x86_64::gdt::TSS_SELECTOR);
            cpu::write_gs_base(self as *const PerCpu as u64);
        }
    }

    /// Marca online e contabiliza.
    pub fn set_online(&self) {
        if !self.online.swap(true, Ordering::AcqRel) {
            ONLINE.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// Inicializa a estrutura da BSP (usa a pilha inicial do kernel) e liga `gs`.
pub fn init_bsp() {
    let apic_id = crate::acpi::info().bsp_apic_id;
    let cpu = PerCpu::allocate(
        0,
        apic_id,
        nexo_boot_abi::KERNEL_STACK_BASE,
        nexo_boot_abi::KERNEL_STACK_SIZE,
    );
    // SAFETY: primeira ativação na BSP.
    unsafe { cpu.activate() };
    cpu.set_online();
    ENABLED.store(true, Ordering::Release);
    kinfo!(
        "percpu: cpu0 apic_id={} gs={:#x}; GDT/TSS por CPU ativas, #DF em {:#x}",
        apic_id,
        cpu::read_gs_base(),
        cpu.df_stack_bounds().1
    );
}

/// `true` depois de `init_bsp`.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// A CPU atual (via `gs:[0]`).
pub fn current() -> &'static PerCpu {
    debug_assert!(is_enabled());
    let p = cpu::read_gs_self() as *const PerCpu;
    // SAFETY: `gs` aponta para uma estrutura 'static configurada por `activate`.
    unsafe { &*p }
}

/// A CPU atual, se o mecanismo já foi ligado.
pub fn try_current() -> Option<&'static PerCpu> {
    if is_enabled() && cpu::read_gs_base() != 0 {
        Some(current())
    } else {
        None
    }
}

/// Estrutura da CPU `index`, se existir.
pub fn get(index: usize) -> Option<&'static PerCpu> {
    CPUS.lock().get(index).copied().flatten()
}

/// Número de CPUs online.
pub fn online_count() -> usize {
    ONLINE.load(Ordering::Acquire)
}

/// Limites da pilha de #DF que contém `addr` (qualquer CPU).
pub fn df_stack_bounds_containing(addr: u64) -> Option<(u64, u64)> {
    let table = CPUS.try_lock()?;
    table.iter().flatten().find_map(|c| {
        let b = c.df_stack_bounds();
        (b.0..b.1).contains(&addr).then_some(b)
    })
}

/// Limites da pilha principal da CPU que contém `addr`.
pub fn stack_bounds_containing(addr: u64) -> Option<(u64, u64)> {
    let table = CPUS.try_lock()?;
    table.iter().flatten().find_map(|c| {
        let (lo, hi) = (c.stack_base, c.stack_base + c.stack_size);
        (lo..hi).contains(&addr).then_some((lo, hi))
    })
}

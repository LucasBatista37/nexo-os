//! GDT com código/dados do kernel e TSS com pilha IST1 para #DF.

use nexo_arch_x86_64::gdt::{
    GlobalDescriptorTable, KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR, TaskStateSegment, load_tss,
};

use crate::cell::StaticCell;

const DF_STACK_SIZE: usize = 16 * 1024;

#[repr(C, align(16))]
struct Stack([u8; DF_STACK_SIZE]);

static GDT: StaticCell<GlobalDescriptorTable> = StaticCell::new(GlobalDescriptorTable::new());
static TSS: StaticCell<TaskStateSegment> = StaticCell::new(TaskStateSegment::new());
static DF_STACK: StaticCell<Stack> = StaticCell::new(Stack([0; DF_STACK_SIZE]));

/// Limites `[base, topo)` da pilha de double fault.
pub fn double_fault_stack_bounds() -> (u64, u64) {
    let base = DF_STACK.as_ptr() as u64;
    (base, base + DF_STACK_SIZE as u64)
}

/// Monta e carrega GDT + TSS.
pub fn init() {
    // SAFETY: executado uma única vez, em uma única CPU, antes de qualquer
    // outro acesso a GDT/TSS; depois de carregadas só há leituras pela CPU.
    let (tss_sel, gdt_ref) = unsafe {
        let tss = &mut *TSS.as_ptr();
        tss.interrupt_stack_table[0] = double_fault_stack_bounds().1;
        let gdt = &mut *GDT.as_ptr();
        gdt.add_kernel_code();
        gdt.add_kernel_data();
        gdt.add_user_segments();
        let sel = gdt.add_tss(&*TSS.as_ptr());
        (sel, &*GDT.as_ptr())
    };
    // SAFETY: GDT contém código em 0x08, dados em 0x10 e o TSS em `tss_sel`.
    unsafe {
        gdt_ref.load();
        load_tss(tss_sel);
    }
    kinfo!(
        "gdt: carregada (cs={:#x} ds={:#x} tss={:#x}); IST1 para #DF em {:#x}",
        KERNEL_CODE_SELECTOR,
        KERNEL_DATA_SELECTOR,
        tss_sel,
        double_fault_stack_bounds().1
    );
}

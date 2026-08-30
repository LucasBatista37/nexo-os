//! GDT, seletores e TSS para modo longo.

/// Seletor de código do kernel.
pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
/// Seletor de dados do kernel.
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
/// Seletor do TSS.
pub const TSS_SELECTOR: u16 = 0x18;

/// Operando de `lgdt`/`lidt`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DescriptorTablePointer {
    /// Tamanho em bytes menos um.
    pub limit: u16,
    /// Endereço linear da tabela.
    pub base: u64,
}

/// Task State Segment de 64 bits.
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct TaskStateSegment {
    reserved1: u32,
    /// RSP0..RSP2: pilhas para troca de privilégio.
    pub privilege_stack_table: [u64; 3],
    reserved2: u64,
    /// IST1..IST7: pilhas alternativas para exceções.
    pub interrupt_stack_table: [u64; 7],
    reserved3: u64,
    reserved4: u16,
    /// Deslocamento do I/O permission bitmap (= tamanho do TSS se ausente).
    pub iomap_base: u16,
}

impl TaskStateSegment {
    /// TSS vazio.
    pub const fn new() -> Self {
        TaskStateSegment {
            reserved1: 0,
            privilege_stack_table: [0; 3],
            reserved2: 0,
            interrupt_stack_table: [0; 7],
            reserved3: 0,
            reserved4: 0,
            iomap_base: core::mem::size_of::<TaskStateSegment>() as u16,
        }
    }
}

impl Default for TaskStateSegment {
    fn default() -> Self {
        Self::new()
    }
}

const CODE_64: u64 = 0x00af_9a00_0000_0000; // P, DPL0, S, code RX, L
const DATA_64: u64 = 0x00cf_9200_0000_0000; // P, DPL0, S, data RW

/// Tabela global de descritores.
pub struct GlobalDescriptorTable {
    entries: [u64; 8],
    len: usize,
}

impl GlobalDescriptorTable {
    /// GDT apenas com a entrada nula.
    pub const fn new() -> Self {
        GlobalDescriptorTable {
            entries: [0; 8],
            len: 1,
        }
    }

    const fn push(&mut self, v: u64) -> u16 {
        let idx = self.len;
        self.entries[idx] = v;
        self.len += 1;
        (idx * 8) as u16
    }

    /// Adiciona o segmento de código do kernel.
    pub const fn add_kernel_code(&mut self) -> u16 {
        self.push(CODE_64)
    }

    /// Adiciona o segmento de dados do kernel.
    pub const fn add_kernel_data(&mut self) -> u16 {
        self.push(DATA_64)
    }

    /// Adiciona um descritor de TSS (ocupa duas entradas).
    pub fn add_tss(&mut self, tss: &'static TaskStateSegment) -> u16 {
        let base = tss as *const TaskStateSegment as u64;
        let limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u64;
        let low = (limit & 0xffff)
            | ((base & 0x00ff_ffff) << 16)
            | (0x89u64 << 40)
            | (((limit >> 16) & 0xf) << 48)
            | (((base >> 24) & 0xff) << 56);
        let high = base >> 32;
        let sel = self.push(low);
        self.push(high);
        sel
    }

    /// Ponteiro para `lgdt`.
    pub fn pointer(&'static self) -> DescriptorTablePointer {
        DescriptorTablePointer {
            limit: (self.len * 8 - 1) as u16,
            base: self.entries.as_ptr() as u64,
        }
    }

    /// Entradas cruas (para testes/depuração).
    pub fn entries(&self) -> &[u64] {
        &self.entries[..self.len]
    }

    /// Carrega a GDT e recarrega CS/SS/DS/ES/FS/GS.
    ///
    /// # Safety
    /// A tabela deve conter código em 0x08 e dados em 0x10 e viver para sempre.
    #[cfg(target_arch = "x86_64")]
    pub unsafe fn load(&'static self) {
        let ptr = self.pointer();
        // SAFETY: contrato da função; `retfq` recarrega CS com o seletor 0x08.
        unsafe {
            core::arch::asm!(
                "lgdt [{ptr}]",
                "push {code}",
                "lea {tmp}, [rip + 2f]",
                "push {tmp}",
                "retfq",
                "2:",
                "mov ds, {data:x}",
                "mov es, {data:x}",
                "mov ss, {data:x}",
                "mov fs, {data:x}",
                "mov gs, {data:x}",
                ptr = in(reg) &ptr,
                code = const KERNEL_CODE_SELECTOR as u64,
                data = in(reg) KERNEL_DATA_SELECTOR,
                tmp = out(reg) _,
                options(nostack),
            );
        }
    }
}

impl Default for GlobalDescriptorTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Carrega o registrador de tarefa.
///
/// # Safety
/// `selector` deve apontar para um descritor de TSS válido na GDT ativa.
#[cfg(target_arch = "x86_64")]
pub unsafe fn load_tss(selector: u16) {
    // SAFETY: contrato da função.
    unsafe { core::arch::asm!("ltr {0:x}", in(reg) selector, options(nomem, nostack)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        assert_eq!(core::mem::size_of::<TaskStateSegment>(), 104);
        assert_eq!(core::mem::size_of::<DescriptorTablePointer>(), 10);
        let mut g = GlobalDescriptorTable::new();
        assert_eq!(g.add_kernel_code(), KERNEL_CODE_SELECTOR);
        assert_eq!(g.add_kernel_data(), KERNEL_DATA_SELECTOR);
        static TSS: TaskStateSegment = TaskStateSegment::new();
        assert_eq!(g.add_tss(&TSS), TSS_SELECTOR);
        assert_eq!(g.entries().len(), 5);
        let low = g.entries()[3];
        assert_eq!(low & 0xffff, 103); // limite
        assert_eq!((low >> 40) & 0xff, 0x89); // presente, TSS disponível
        let base = &TSS as *const _ as u64;
        assert_eq!((low >> 16) & 0xff_ffff, base & 0xff_ffff);
        assert_eq!(g.entries()[4], base >> 32);
    }
}

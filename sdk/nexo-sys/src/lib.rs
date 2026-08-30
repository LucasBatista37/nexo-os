//! SDK mínimo: invoca syscalls do Nexo OS. Só funciona em x86_64 (em outros
//! hosts as funções devolvem `NotSupported`, para permitir compilar/lintar).
#![no_std]

use abi::Status;
pub use nexo_syscall_abi as abi;

/// Invoca a syscall `n` com até três argumentos. Devolve `(status, valor)`.
///
/// # Safety
/// O kernel valida ponteiros, mas argumentos incoerentes podem encerrar o processo.
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn raw(n: u64, a0: u64, a1: u64, a2: u64) -> (Status, u64) {
    let status: u64;
    let value: u64;
    // SAFETY: convenção da ABI v0; `rcx`/`r11` são destruídos por `syscall`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") n => status,
            in("rdi") a0,
            in("rsi") a1,
            inlateout("rdx") a2 => value,
            out("rcx") _,
            out("r11") _,
            options(nostack)
        );
    }
    (Status::from_u64(status), value)
}

/// Invoca a syscall `n` com até cinco argumentos. Devolve `(status, valor)`.
///
/// # Safety
/// Ver [`raw`].
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn raw5(n: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> (Status, u64) {
    let status: u64;
    let value: u64;
    // SAFETY: convenção da ABI v0 (a3 em r10, a4 em r8).
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") n => status,
            in("rdi") a0,
            in("rsi") a1,
            inlateout("rdx") a2 => value,
            in("r10") a3,
            in("r8") a4,
            out("rcx") _,
            out("r11") _,
            options(nostack)
        );
    }
    (Status::from_u64(status), value)
}

/// Versão de `raw5` para outros hosts (compila, mas não faz nada).
///
/// # Safety
/// Nunca é perigosa: não invoca nada.
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub unsafe fn raw5(_n: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64) -> (Status, u64) {
    (Status::NotSupported, 0)
}

/// Versão para outros hosts (compila, mas não faz nada).
///
/// # Safety
/// Nunca é perigosa: não invoca nada.
#[cfg(not(target_arch = "x86_64"))]
#[inline]
pub unsafe fn raw(_n: u64, _a0: u64, _a1: u64, _a2: u64) -> (Status, u64) {
    (Status::NotSupported, 0)
}

/// Encerra o processo.
pub fn exit(code: i64) -> ! {
    // SAFETY: syscall sem ponteiros.
    unsafe {
        let _ = raw(abi::SYS_EXIT, code as u64, 0, 0);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Escreve no log do kernel.
pub fn log(s: &str) -> Status {
    // SAFETY: ponteiro e tamanho vêm de um `&str` válido.
    unsafe { raw(abi::SYS_LOG, s.as_ptr() as u64, s.len() as u64, 0).0 }
}

/// Relógio monotônico em nanossegundos.
pub fn time_now() -> u64 {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_TIME_NOW, 0, 0, 0).1 }
}

/// Cede a CPU.
pub fn yield_now() {
    // SAFETY: sem ponteiros.
    unsafe {
        let _ = raw(abi::SYS_YIELD, 0, 0, 0);
    }
}

/// Dorme `ns` nanossegundos.
pub fn sleep_ns(ns: u64) {
    // SAFETY: sem ponteiros.
    unsafe {
        let _ = raw(abi::SYS_SLEEP, ns, 0, 0);
    }
}

/// ID do processo.
pub fn get_pid() -> u64 {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_GET_PID, 0, 0, 0).1 }
}

/// Versão da ABI do kernel.
pub fn abi_version() -> u64 {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_ABI_VERSION, 0, 0, 0).1 }
}

/// Informação de depuração (`sel`: 0 CPUs, 1 uptime ms, 2 syscalls do processo).
pub fn debug_info(sel: u64) -> u64 {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_DEBUG_INFO, sel, 0, 0).1 }
}

/// Handle de objeto do kernel (índice na tabela do processo).
pub type Handle = u32;

/// Fecha um handle.
pub fn handle_close(h: Handle) -> Status {
    // SAFETY: sem ponteiros.
    unsafe { raw(abi::SYS_HANDLE_CLOSE, h as u64, 0, 0).0 }
}

/// Duplica um handle com direitos `rights` (subconjunto dos atuais).
pub fn handle_duplicate(h: Handle, rights: u32) -> Result<Handle, Status> {
    // SAFETY: sem ponteiros.
    let (st, v) = unsafe { raw(abi::SYS_HANDLE_DUPLICATE, h as u64, rights as u64, 0) };
    if st.is_ok() { Ok(v as Handle) } else { Err(st) }
}

/// Direitos e tipo de um handle: `(rights, kind)`.
pub fn handle_info(h: Handle) -> Result<(u32, u32), Status> {
    // SAFETY: sem ponteiros.
    let (st, v) = unsafe { raw(abi::SYS_HANDLE_INFO, h as u64, 0, 0) };
    if st.is_ok() {
        Ok((v as u32, (v >> 32) as u32))
    } else {
        Err(st)
    }
}

/// Cria um canal; devolve as duas extremidades.
pub fn channel_create() -> Result<(Handle, Handle), Status> {
    // SAFETY: sem ponteiros.
    let (st, v) = unsafe { raw(abi::SYS_CHANNEL_CREATE, 0, 0, 0) };
    if st.is_ok() {
        Ok((v as u32, (v >> 32) as u32))
    } else {
        Err(st)
    }
}

/// Envia `data` e `handles` pelo canal `h` (os handles enviados saem da tabela).
pub fn channel_send(h: Handle, data: &[u8], handles: &[Handle]) -> Status {
    // SAFETY: ponteiros e tamanhos vêm de slices válidas.
    unsafe {
        raw5(
            abi::SYS_CHANNEL_SEND,
            h as u64,
            data.as_ptr() as u64,
            data.len() as u64,
            handles.as_ptr() as u64,
            handles.len() as u64,
        )
        .0
    }
}

/// Recebe uma mensagem em `buf`/`handles`; devolve `(bytes, handles recebidos)`. Bloqueia.
pub fn channel_recv(
    h: Handle,
    buf: &mut [u8],
    handles: &mut [Handle],
) -> Result<(usize, usize), Status> {
    // SAFETY: ponteiros e capacidades vêm de slices válidas e mutáveis.
    let (st, v) = unsafe {
        raw5(
            abi::SYS_CHANNEL_RECV,
            h as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
            handles.as_mut_ptr() as u64,
            handles.len() as u64,
        )
    };
    if st.is_ok() {
        Ok((v as u32 as usize, (v >> 32) as usize))
    } else {
        Err(st)
    }
}

/// Cria um processo a partir do membro `name` do initrd, com argumento `arg` e `handles` iniciais
/// (que saem da tabela do chamador). Devolve o handle do processo.
pub fn process_spawn(name: &str, arg: u64, handles: &[Handle]) -> Result<Handle, Status> {
    // SAFETY: ponteiros e tamanhos vêm de slices válidas.
    let (st, v) = unsafe {
        raw5(
            abi::SYS_PROCESS_SPAWN,
            name.as_ptr() as u64,
            name.len() as u64,
            arg,
            handles.as_ptr() as u64,
            handles.len() as u64,
        )
    };
    if st.is_ok() { Ok(v as Handle) } else { Err(st) }
}

/// Aguarda o processo terminar; devolve o código de saída.
pub fn process_wait(h: Handle) -> Result<i64, Status> {
    // SAFETY: sem ponteiros.
    let (st, v) = unsafe { raw(abi::SYS_PROCESS_WAIT, h as u64, 0, 0) };
    if st.is_ok() { Ok(v as i64) } else { Err(st) }
}

/// `(pid, terminou)` do processo.
pub fn process_info(h: Handle) -> Result<(u64, bool), Status> {
    // SAFETY: sem ponteiros.
    let (st, v) = unsafe { raw(abi::SYS_PROCESS_INFO, h as u64, 0, 0) };
    if st.is_ok() {
        Ok((
            v & !abi::PROCESS_INFO_EXITED,
            v & abi::PROCESS_INFO_EXITED != 0,
        ))
    } else {
        Err(st)
    }
}

/// Lê uma estrutura `#[repr(C)]` preenchida pelo kernel.
fn zeroed<T: Default>() -> T {
    T::default()
}

/// Enumera funções PCI em `out`; devolve o total existente (pode ser maior que `out.len()`).
pub fn pci_enum(dev: Handle, out: &mut [abi::PciInfo]) -> Result<usize, Status> {
    // SAFETY: buffer válido e mutável; o kernel escreve no máximo `out.len()` entradas.
    let (st, n) = unsafe {
        raw(
            abi::SYS_PCI_ENUM,
            dev as u64,
            out.as_mut_ptr() as u64,
            out.len() as u64,
        )
    };
    if st.is_ok() { Ok(n as usize) } else { Err(st) }
}

/// Lê 32 bits de configuração PCI.
pub fn pci_cfg_read(dev: Handle, bdf: u16, offset: u16) -> Result<u32, Status> {
    // SAFETY: sem ponteiros.
    let (st, v) = unsafe { raw(abi::SYS_PCI_CFG_READ, dev as u64, bdf as u64, offset as u64) };
    if st.is_ok() { Ok(v as u32) } else { Err(st) }
}

/// Escreve 32 bits de configuração PCI.
pub fn pci_cfg_write(dev: Handle, bdf: u16, offset: u16, value: u32) -> Status {
    // SAFETY: sem ponteiros.
    unsafe {
        raw5(
            abi::SYS_PCI_CFG_WRITE,
            dev as u64,
            bdf as u64,
            offset as u64,
            value as u64,
            0,
        )
        .0
    }
}

/// Mapeia MMIO no processo; devolve o endereço virtual.
pub fn mmio_map(dev: Handle, phys: u64, len: u64) -> Result<u64, Status> {
    // SAFETY: sem ponteiros de usuário.
    let (st, v) = unsafe { raw(abi::SYS_MMIO_MAP, dev as u64, phys, len) };
    if st.is_ok() { Ok(v) } else { Err(st) }
}

/// Aloca uma página de DMA.
pub fn dma_alloc(dev: Handle) -> Result<abi::DmaBuffer, Status> {
    let mut b: abi::DmaBuffer = zeroed();
    // SAFETY: `b` é uma estrutura válida e mutável.
    let (st, _) = unsafe {
        raw(
            abi::SYS_DMA_ALLOC,
            dev as u64,
            &mut b as *mut abi::DmaBuffer as u64,
            0,
        )
    };
    if st.is_ok() { Ok(b) } else { Err(st) }
}

/// Reserva um vetor de interrupção (MSI/MSI-X).
pub fn irq_alloc(dev: Handle) -> Result<abi::IrqInfo, Status> {
    let mut i: abi::IrqInfo = zeroed();
    // SAFETY: `i` é uma estrutura válida e mutável.
    let (st, _) = unsafe {
        raw(
            abi::SYS_IRQ_ALLOC,
            dev as u64,
            &mut i as *mut abi::IrqInfo as u64,
            0,
        )
    };
    if st.is_ok() { Ok(i) } else { Err(st) }
}

/// Como [`channel_recv`], mas devolve `Err(WouldBlock)` em vez de bloquear quando não há mensagem.
pub fn channel_try_recv(
    h: Handle,
    buf: &mut [u8],
    handles: &mut [Handle],
) -> Result<(usize, usize), Status> {
    // SAFETY: ponteiros e capacidades vêm de slices válidas e mutáveis.
    let (st, v) = unsafe {
        raw5(
            abi::SYS_CHANNEL_TRY_RECV,
            h as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
            handles.as_mut_ptr() as u64,
            handles.len() as u64,
        )
    };
    if st.is_ok() {
        Ok(((v & 0xffff_ffff) as usize, (v >> 32) as usize))
    } else {
        Err(st)
    }
}

/// Deriva uma concessão restrita à função PCI `bdf` (exige `ADMIN` na concessão raiz).
pub fn device_open(root: Handle, bdf: u16) -> Result<Handle, Status> {
    // SAFETY: sem ponteiros.
    let (st, v) = unsafe { raw(abi::SYS_DEVICE_OPEN, root as u64, bdf as u64, 0) };
    if st.is_ok() { Ok(v as Handle) } else { Err(st) }
}

/// Espera o vetor disparar mais de `seen` vezes; devolve a contagem atual.
pub fn irq_wait(dev: Handle, vector: u32, seen: u64) -> Result<u64, Status> {
    // SAFETY: sem ponteiros.
    let (st, v) = unsafe { raw(abi::SYS_IRQ_WAIT, dev as u64, vector as u64, seen) };
    if st.is_ok() { Ok(v) } else { Err(st) }
}

//! Despacho de syscalls (ABI v0) e cópia segura a partir do espaço do usuário.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::paging::PageFlags;
use nexo_arch_x86_64::trap::TrapFrame;
use nexo_mm::{PAGE_SIZE, VirtAddr};
use nexo_syscall_abi::*;

use crate::process;
use crate::sched;
use crate::sync::IrqLock;

static LAST_LOG: IrqLock<String> = IrqLock::new(String::new());

/// Última mensagem de `SYS_LOG` (para testes).
pub fn last_user_log() -> String {
    LAST_LOG.lock().clone()
}

/// Copia `[ptr, ptr+len)` do espaço do usuário atual, validando faixa e mapeamento.
pub fn copy_from_user(ptr: u64, len: u64) -> Result<Vec<u8>, Status> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let end = ptr.checked_add(len).ok_or(Status::BadAddress)?;
    if end > USER_ADDRESS_LIMIT {
        return Err(Status::BadAddress);
    }
    let mut page = ptr & !(PAGE_SIZE - 1);
    while page < end {
        match crate::mm::virt::translate(VirtAddr::new(page)) {
            Some(t)
                if t.flags.contains(PageFlags::USER) && t.flags.contains(PageFlags::PRESENT) => {}
            _ => return Err(Status::BadAddress),
        }
        page += PAGE_SIZE;
    }
    let mut out = Vec::with_capacity(len as usize);
    // SAFETY: faixa validada como mapeada com USER no espaço atual; leitura de
    // páginas de usuário pelo kernel é permitida (sem SMAP).
    unsafe {
        out.extend_from_slice(core::slice::from_raw_parts(ptr as *const u8, len as usize));
    }
    Ok(out)
}

fn dispatch(f: &mut TrapFrame) -> (Status, u64) {
    let Some(p) = process::current() else {
        return (Status::Denied, 0);
    };
    p.syscalls.fetch_add(1, Ordering::Relaxed);
    let n = f.rax;
    if n == SYS_EXIT {
        // Nenhum `Arc<Process>` pode ficar vivo nesta pilha: a thread nunca retorna.
        let code = f.rdi as i64;
        drop(p);
        process::exit_current(code, None);
    }
    match n {
        SYS_LOG => {
            if f.rsi as usize > LOG_MAX {
                return (Status::InvalidArgs, 0);
            }
            match copy_from_user(f.rdi, f.rsi) {
                Ok(bytes) => match core::str::from_utf8(&bytes) {
                    Ok(s) => {
                        kinfo!("[pid {} {}] {}", p.pid, p.name, s);
                        *LAST_LOG.lock() = String::from(s);
                        process::note_user_log();
                        (Status::Ok, bytes.len() as u64)
                    }
                    Err(_) => (Status::InvalidArgs, 0),
                },
                Err(e) => (e, 0),
            }
        }
        SYS_TIME_NOW => (Status::Ok, crate::time::monotonic_ns()),
        SYS_YIELD => {
            sched::yield_now();
            (Status::Ok, 0)
        }
        SYS_SLEEP => {
            sched::sleep_ms(f.rdi.div_ceil(1_000_000));
            (Status::Ok, 0)
        }
        SYS_GET_PID => (Status::Ok, p.pid),
        SYS_ABI_VERSION => (Status::Ok, ABI_VERSION),
        SYS_DEBUG_INFO => match f.rdi {
            0 => (Status::Ok, crate::x86::percpu::online_count() as u64),
            1 => (Status::Ok, crate::time::uptime_ms()),
            2 => (Status::Ok, p.syscalls.load(Ordering::Relaxed)),
            _ => (Status::InvalidArgs, 0),
        },
        _ => {
            kdebug!("syscall desconhecida {} do pid {}", n, p.pid);
            (Status::NotSupported, 0)
        }
    }
}

/// Chamado pela entrada em assembly com o frame da syscall (interrupções desabilitadas).
#[unsafe(no_mangle)]
extern "C" fn nexo_syscall_dispatch(frame: *mut TrapFrame) {
    // SAFETY: frame empilhado por `nexo_syscall_entry` na pilha de kernel desta thread.
    let f = unsafe { &mut *frame };
    // SAFETY: estamos na pilha de kernel com gs configurado; a syscall pode bloquear.
    unsafe { cpu::enable_interrupts() };
    let (status, value) = dispatch(f);
    cpu::disable_interrupts();
    f.rax = status as u64;
    f.rdx = value;
}

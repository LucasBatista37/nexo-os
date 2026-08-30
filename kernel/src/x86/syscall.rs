//! Despacho de syscalls (ABI v0) e cópia segura a partir do espaço do usuário.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::paging::PageFlags;
use nexo_arch_x86_64::trap::TrapFrame;
use nexo_mm::{PAGE_SIZE, VirtAddr};
use nexo_syscall_abi::*;

use crate::ipc::{ChannelEnd, Handle, Message, Object, Rights};
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

/// Valida que `[ptr, ptr+len)` é gravável pelo usuário no espaço atual.
fn check_user_writable(ptr: u64, len: u64) -> Result<(), Status> {
    if len == 0 {
        return Ok(());
    }
    let end = ptr.checked_add(len).ok_or(Status::BadAddress)?;
    if end > USER_ADDRESS_LIMIT {
        return Err(Status::BadAddress);
    }
    let mut page = ptr & !(PAGE_SIZE - 1);
    while page < end {
        match crate::mm::virt::translate(VirtAddr::new(page)) {
            Some(t)
                if t.flags
                    .contains(PageFlags::USER | PageFlags::PRESENT | PageFlags::WRITABLE) => {}
            _ => return Err(Status::BadAddress),
        }
        page += PAGE_SIZE;
    }
    Ok(())
}

/// Copia `data` para `[ptr, ptr+len)` no espaço do usuário atual.
pub fn copy_to_user(ptr: u64, data: &[u8]) -> Result<(), Status> {
    check_user_writable(ptr, data.len() as u64)?;
    // SAFETY: faixa validada como mapeada USER|WRITABLE no espaço atual.
    unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len()) };
    Ok(())
}

fn sys_channel_send(p: &process::Process, f: &TrapFrame) -> (Status, u64) {
    let (h, ptr, len, hptr, nh) = (f.rdi as u32, f.rsi, f.rdx, f.r10, f.r8 as usize);
    if len as usize > MSG_MAX || nh > MSG_HANDLES_MAX {
        return (Status::TooBig, 0);
    }
    let handle = match p.handles.lock().get(h) {
        Ok(h) => h,
        Err(e) => return (e, 0),
    };
    if !handle.rights.contains(RIGHT_WRITE) {
        return (Status::Denied, 0);
    }
    let Object::Channel(end) = &handle.object;
    let data = match copy_from_user(ptr, len) {
        Ok(d) => d,
        Err(e) => return (e, 0),
    };
    let raw_handles = match copy_from_user(hptr, (nh * 4) as u64) {
        Ok(d) => d,
        Err(e) => return (e, 0),
    };
    let ids: Vec<u32> = raw_handles
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    // Retira os handles do remetente; falha se algum não for transferível.
    let mut moved = Vec::with_capacity(ids.len());
    {
        let mut table = p.handles.lock();
        for id in &ids {
            if *id == h {
                return (Status::InvalidArgs, 0);
            }
            match table.get(*id) {
                Ok(hh) if hh.rights.contains(RIGHT_TRANSFER) => {}
                Ok(_) => return (Status::Denied, 0),
                Err(e) => return (e, 0),
            }
        }
        for id in &ids {
            moved.push(table.take(*id).expect("verificado acima"));
        }
    }
    match end.send(Message {
        data,
        handles: moved,
    }) {
        Ok(()) => (Status::Ok, len),
        Err(e) => (e, 0),
    }
}

fn sys_channel_recv(p: &process::Process, f: &TrapFrame) -> (Status, u64) {
    let (h, buf, cap, hbuf, hcap) = (f.rdi as u32, f.rsi, f.rdx, f.r10, f.r8 as usize);
    let handle = match p.handles.lock().get(h) {
        Ok(h) => h,
        Err(e) => return (e, 0),
    };
    if !handle.rights.contains(RIGHT_READ) {
        return (Status::Denied, 0);
    }
    if let Err(e) = check_user_writable(buf, cap) {
        return (e, 0);
    }
    if let Err(e) = check_user_writable(hbuf, (hcap * 4) as u64) {
        return (e, 0);
    }
    let Object::Channel(end) = &handle.object;
    let msg = match end.recv() {
        Ok(m) => m,
        Err(e) => return (e, 0),
    };
    if msg.data.len() as u64 > cap || msg.handles.len() > hcap {
        // Mensagem descartada por buffer pequeno: comportamento documentado da v0.
        let needed = ((msg.data.len() as u64) & 0xffff_ffff) | ((msg.handles.len() as u64) << 32);
        return (Status::TooBig, needed);
    }
    if let Err(e) = copy_to_user(buf, &msg.data) {
        return (e, 0);
    }
    let mut ids = Vec::with_capacity(msg.handles.len() * 4);
    {
        let mut table = p.handles.lock();
        for hh in msg.handles {
            match table.insert(hh) {
                Ok(i) => ids.extend_from_slice(&i.to_le_bytes()),
                Err(e) => return (e, 0),
            }
        }
    }
    if let Err(e) = copy_to_user(hbuf, &ids) {
        return (e, 0);
    }
    (
        Status::Ok,
        (msg.data.len() as u64) | ((ids.len() as u64 / 4) << 32),
    )
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
            3 => (Status::Ok, p.handles.lock().len() as u64),
            _ => (Status::InvalidArgs, 0),
        },
        SYS_HANDLE_CLOSE => match p.handles.lock().take(f.rdi as u32) {
            Ok(_) => (Status::Ok, 0),
            Err(e) => (e, 0),
        },
        SYS_HANDLE_DUPLICATE => {
            let mut table = p.handles.lock();
            match table.get(f.rdi as u32) {
                Ok(h) => {
                    let wanted = Rights(f.rsi as u32);
                    if !h.rights.contains(RIGHT_DUPLICATE) || !h.rights.is_superset_of(wanted) {
                        return (Status::Denied, 0);
                    }
                    match table.insert(Handle {
                        object: h.object.clone(),
                        rights: wanted,
                    }) {
                        Ok(i) => (Status::Ok, i as u64),
                        Err(e) => (e, 0),
                    }
                }
                Err(e) => (e, 0),
            }
        }
        SYS_HANDLE_INFO => match p.handles.lock().get(f.rdi as u32) {
            Ok(h) => (
                Status::Ok,
                h.rights.0 as u64 | ((h.object.kind() as u64) << 32),
            ),
            Err(e) => (e, 0),
        },
        SYS_CHANNEL_CREATE => {
            let (a, b) = ChannelEnd::create_pair();
            let mut table = p.handles.lock();
            let ha = match table.insert(Handle {
                object: Object::Channel(a),
                rights: Rights(RIGHTS_CHANNEL_DEFAULT),
            }) {
                Ok(i) => i,
                Err(e) => return (e, 0),
            };
            match table.insert(Handle {
                object: Object::Channel(b),
                rights: Rights(RIGHTS_CHANNEL_DEFAULT),
            }) {
                Ok(hb) => (Status::Ok, ha as u64 | ((hb as u64) << 32)),
                Err(e) => {
                    let _ = table.take(ha);
                    (e, 0)
                }
            }
        }
        SYS_CHANNEL_SEND => sys_channel_send(&p, f),
        SYS_CHANNEL_RECV => sys_channel_recv(&p, f),
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

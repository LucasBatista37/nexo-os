//! Despacho de syscalls (ABI v0) e cópia segura a partir do espaço do usuário.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use nexo_arch_x86_64::cpu;
use nexo_arch_x86_64::paging::PageFlags;
use nexo_arch_x86_64::trap::TrapFrame;
use nexo_mm::{PAGE_SIZE, VirtAddr};
use nexo_syscall_abi::*;

use crate::ipc::{ChannelEnd, DeviceGrant, Handle, MemoryObject, Message, Object, Rights};
use crate::process;
use crate::sched;
use crate::sync::IrqLock;
use nexo_arch_x86_64::pci::Bdf;
use nexo_mm::PhysAddr;

static LAST_LOG: IrqLock<String> = IrqLock::new(String::new());
static RESTART_LOGS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Linhas de log de usuário que mencionam um reinício de serviço.
pub fn restart_log_count() -> u64 {
    RESTART_LOGS.load(Ordering::Relaxed)
}

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
    let Object::Channel(end) = &handle.object else {
        return (Status::InvalidArgs, 0);
    };
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
        for (i, id) in ids.iter().enumerate() {
            // Enviar o próprio canal ou repetir um handle na mesma mensagem é inválido.
            if *id == h || ids[..i].contains(id) {
                return (Status::InvalidArgs, 0);
            }
            match table.get(*id) {
                Ok(Handle {
                    object: Object::Channel(x),
                    rights,
                }) => {
                    if !rights.contains(RIGHT_TRANSFER) {
                        return (Status::Denied, 0);
                    }
                    if end.same_channel(&x) {
                        return (Status::InvalidArgs, 0);
                    }
                }
                Ok(hh) if hh.rights.contains(RIGHT_TRANSFER) => {}
                Ok(_) => return (Status::Denied, 0),
                Err(e) => return (e, 0),
            }
        }
        for id in &ids {
            match table.take(*id) {
                Ok(hh) => moved.push(hh),
                Err(e) => return (e, 0),
            }
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

/// Cria um canal de interrupções: o kernel envia 1 byte por disparo do vetor (coalescido).
/// Cria um objeto de memória compartilhável de `a1` páginas (zeradas); devolve o handle.
fn sys_memory_create(p: &process::Process, f: &TrapFrame) -> (Status, u64) {
    let pages = f.rdi;
    if pages == 0 || pages > nexo_syscall_abi::MEMORY_MAX_PAGES {
        return (Status::InvalidArgs, 0);
    }
    let mut frames = Vec::with_capacity(pages as usize);
    for _ in 0..pages {
        match crate::mm::phys::allocate_zeroed_frame() {
            Some(fr) => frames.push(fr),
            None => {
                // libera o que já alocamos
                for fr in frames.drain(..) {
                    let _ = crate::mm::phys::free_frame(fr);
                }
                return (Status::NoMemory, 0);
            }
        }
    }
    let obj = alloc::sync::Arc::new(MemoryObject {
        frames,
        len: pages * PAGE_SIZE,
    });
    let handle = Handle {
        object: Object::Memory(obj),
        rights: Rights(RIGHTS_MEMORY_DEFAULT),
    };
    match p.handles.lock().insert(handle) {
        Ok(i) => (Status::Ok, i as u64),
        Err(e) => (e, 0),
    }
}

/// Mapeia o objeto de memória `a1` no processo; devolve o endereço virtual base.
fn sys_memory_map(p: &Arc<process::Process>, f: &TrapFrame) -> (Status, u64) {
    let obj = match p.handles.lock().get(f.rdi as u32) {
        Ok(Handle {
            object: Object::Memory(m),
            rights,
        }) => {
            if !rights.contains(RIGHT_MAP) {
                return (Status::Denied, 0);
            }
            m
        }
        Ok(_) => return (Status::InvalidArgs, 0),
        Err(e) => return (e, 0),
    };
    let base = p.reserve_device_region(obj.len);
    for (i, fr) in obj.frames.iter().enumerate() {
        let virt = VirtAddr::new(base + (i as u64) * PAGE_SIZE);
        if p.space.map_user_shared(virt, *fr).is_err() {
            return (Status::NoMemory, 0);
        }
    }
    (Status::Ok, base)
}

/// Desmapeia `a1..a1+a2` (região de dispositivos) do processo, sem liberar os quadros.
fn sys_memory_unmap(p: &Arc<process::Process>, f: &TrapFrame) -> (Status, u64) {
    let base = f.rdi;
    let len = f.rsi;
    if base == 0
        || len == 0
        || !base.is_multiple_of(PAGE_SIZE)
        || !len.is_multiple_of(PAGE_SIZE)
        || base < USER_DEVICE_REGION
        || base.checked_add(len).is_none_or(|e| e > USER_ADDRESS_LIMIT)
    {
        return (Status::InvalidArgs, 0);
    }
    match p.space.unmap_user_shared(VirtAddr::new(base), len) {
        Ok(()) => (Status::Ok, 0),
        Err(_) => (Status::InvalidArgs, 0),
    }
}

/// Escreve o layout do framebuffer de boot (40 bytes, `FbInfo`) no ponteiro `a1`. Só informação:
/// o mapeamento continua gated pela concessão do dispositivo de vídeo (`mmio_map` no BAR).
fn sys_fb_info(f: &TrapFrame) -> (Status, u64) {
    let fb = &crate::boot::info().framebuffer;
    if !fb.is_present() {
        return (Status::NotSupported, 0);
    }
    // SAFETY: FramebufferInfo é repr(C) com 40 bytes sem padding (8+8+4×6); lemos seus bytes.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            fb as *const nexo_boot_abi::FramebufferInfo as *const u8,
            core::mem::size_of::<nexo_boot_abi::FramebufferInfo>(),
        )
    };
    match copy_to_user(f.rdi, bytes) {
        Ok(()) => (Status::Ok, 0),
        Err(e) => (e, 0),
    }
}

fn sys_irq_channel(p: &process::Process, f: &TrapFrame) -> (Status, u64) {
    let g = match device_grant(p, f.rdi as u32, RIGHT_SIGNAL) {
        Ok(g) => g,
        Err(e) => return (e, 0),
    };
    let vector = f.rsi;
    if vector > 0xff || !crate::irq::is_user_vector(vector as u8) {
        return (Status::InvalidArgs, 0);
    }
    // So vetores desta concessão podem ser ligados a canais.
    if !g.vectors.lock().contains(&(vector as u8)) {
        return (Status::Denied, 0);
    }
    let (kernel_end, user_end) = ChannelEnd::create_pair();
    crate::irq::attach_channel(vector as u8, kernel_end);
    let handle = Handle {
        object: Object::Channel(user_end),
        rights: Rights(RIGHT_READ),
    };
    match p.handles.lock().insert(handle) {
        Ok(i) => (Status::Ok, i as u64),
        Err(e) => (e, 0),
    }
}

/// Espera múltipla sobre canais: devolve o índice do primeiro pronto (mensagem ou par
/// fechado). Registra a thread como waiter em todos e dorme em tiques curtos — o `send` do
/// par acorda imediatamente; o tique de 10 ms cobre a janela entre a re-checagem e o sono.
fn sys_channel_wait_any(p: &process::Process, f: &TrapFrame) -> (Status, u64) {
    let (ptr, n) = (f.rdi, f.rsi as usize);
    if n == 0 || n > nexo_syscall_abi::WAIT_ANY_MAX {
        return (Status::InvalidArgs, 0);
    }
    let bytes = match copy_from_user(ptr, (n * 4) as u64) {
        Ok(b) => b,
        Err(e) => return (e, 0),
    };
    let mut ends: Vec<Arc<ChannelEnd>> = Vec::with_capacity(n);
    {
        let table = p.handles.lock();
        for i in 0..n {
            let h = u32::from_le_bytes([
                bytes[i * 4],
                bytes[i * 4 + 1],
                bytes[i * 4 + 2],
                bytes[i * 4 + 3],
            ]);
            match table.get(h) {
                Ok(Handle {
                    object: Object::Channel(end),
                    rights,
                }) => {
                    if !rights.contains(RIGHT_READ) {
                        return (Status::Denied, 0);
                    }
                    ends.push(end);
                }
                Ok(_) => return (Status::InvalidArgs, 0),
                Err(e) => return (e, 0),
            }
        }
    }
    let me = match crate::sched::current() {
        Some(t) => t.id,
        None => return (Status::Denied, 0),
    };
    loop {
        for (i, end) in ends.iter().enumerate() {
            if end.readable() {
                return (Status::Ok, i as u64);
            }
        }
        for end in &ends {
            end.register_waiter(me);
        }
        // Re-checa depois de registrar: se ficou pronto nesse meio-tempo, o waiter obsoleto
        // sera drenado no proximo send/close do canal.
        let mut ready = None;
        for (i, end) in ends.iter().enumerate() {
            if end.readable() {
                ready = Some(i);
                break;
            }
        }
        if let Some(i) = ready {
            return (Status::Ok, i as u64);
        }
        crate::sched::sleep_ms(10);
    }
}

fn sys_channel_recv(p: &process::Process, f: &TrapFrame, nonblock: bool) -> (Status, u64) {
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
    let Object::Channel(end) = &handle.object else {
        return (Status::InvalidArgs, 0);
    };
    let msg = match if nonblock { end.try_recv() } else { end.recv() } {
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

/// Retira da tabela do processo os handles listados em `hptr[0..nh]` (todos com `TRANSFER`;
/// repetidos são inválidos) para transferi-los a um filho.
fn take_spawn_handles(
    p: &process::Process,
    hptr: u64,
    nh: usize,
) -> Result<Vec<crate::ipc::Handle>, Status> {
    let raw_handles = copy_from_user(hptr, (nh * 4) as u64)?;
    let ids: Vec<u32> = raw_handles
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let mut moved = Vec::with_capacity(ids.len());
    let mut table = p.handles.lock();
    for (i, id) in ids.iter().enumerate() {
        // Handles repetidos na mesma mensagem: o segundo `take` falharia.
        if ids[..i].contains(id) {
            return Err(Status::InvalidArgs);
        }
        match table.get(*id) {
            Ok(hh) if hh.rights.contains(RIGHT_TRANSFER) => {}
            Ok(_) => return Err(Status::Denied),
            Err(e) => return Err(e),
        }
    }
    for id in &ids {
        moved.push(table.take(*id)?);
    }
    Ok(moved)
}

/// Cria um processo a partir de um ELF na memória do chamador (aplicativos instalados).
fn sys_process_spawn_mem(p: &process::Process, f: &TrapFrame) -> (Status, u64) {
    let (elf_ptr, elf_len, arg, hptr, nh) = (f.rdi, f.rsi, f.rdx, f.r10, f.r8 as usize);
    if elf_len == 0 || elf_len > SPAWN_MEM_MAX || nh > MSG_HANDLES_MAX {
        return (Status::TooBig, 0);
    }
    let elf = match copy_from_user(elf_ptr, elf_len) {
        Ok(b) => b,
        Err(e) => return (e, 0),
    };
    let moved = match take_spawn_handles(p, hptr, nh) {
        Ok(m) => m,
        Err(e) => return (e, 0),
    };
    match process::spawn_bytes(&elf, arg, moved) {
        Ok(child) => {
            let h = Handle {
                object: Object::Process(child),
                rights: Rights(RIGHTS_PROCESS_DEFAULT),
            };
            match p.handles.lock().insert(h) {
                Ok(i) => (Status::Ok, i as u64),
                Err(e) => (e, 0),
            }
        }
        Err(e) => {
            kwarn!("process_spawn_mem pelo pid {}: {e}", p.pid);
            (Status::InvalidArgs, 0)
        }
    }
}

fn sys_process_spawn(p: &process::Process, f: &TrapFrame) -> (Status, u64) {
    let (name_ptr, name_len, arg, hptr, nh) = (f.rdi, f.rsi, f.rdx, f.r10, f.r8 as usize);
    if name_len > nexo_initrd::NAME_MAX as u64 || nh > MSG_HANDLES_MAX {
        return (Status::TooBig, 0);
    }
    let name_bytes = match copy_from_user(name_ptr, name_len) {
        Ok(b) => b,
        Err(e) => return (e, 0),
    };
    let Ok(name) = core::str::from_utf8(&name_bytes) else {
        return (Status::InvalidArgs, 0);
    };
    let moved = match take_spawn_handles(p, hptr, nh) {
        Ok(m) => m,
        Err(e) => return (e, 0),
    };
    match process::spawn_named(name, arg, moved) {
        Ok(child) => {
            let h = Handle {
                object: Object::Process(child),
                rights: Rights(RIGHTS_PROCESS_DEFAULT),
            };
            match p.handles.lock().insert(h) {
                Ok(i) => (Status::Ok, i as u64),
                Err(e) => (e, 0),
            }
        }
        Err(e) => {
            kwarn!("process_spawn({name}) pelo pid {}: {e}", p.pid);
            (Status::NotFound, 0)
        }
    }
}

/// Obtém a concessão de dispositivo do handle `h` com o direito `right`.
fn device_grant(p: &process::Process, h: u32, right: u32) -> Result<Arc<DeviceGrant>, Status> {
    match p.handles.lock().get(h) {
        Ok(Handle {
            object: Object::Device(g),
            rights,
        }) => {
            if !rights.contains(right) {
                return Err(Status::Denied);
            }
            Ok(g)
        }
        Ok(_) => Err(Status::InvalidArgs),
        Err(e) => Err(e),
    }
}

fn sys_device(p: &Arc<process::Process>, f: &TrapFrame) -> (Status, u64) {
    let n = f.rax;
    let h = f.rdi as u32;
    match n {
        SYS_PCI_ENUM => {
            let g = match device_grant(p, h, RIGHT_READ) {
                Ok(g) => g,
                Err(e) => return (e, 0),
            };
            let devs: Vec<PciInfo> = crate::pci::devices()
                .into_iter()
                .filter(|d| g.covers(d.bdf))
                .collect();
            let cap = (f.rdx as usize).min(devs.len());
            let bytes: Vec<u8> = devs[..cap]
                .iter()
                .flat_map(|d| {
                    // SAFETY: PciInfo é repr(C) sem padding interno relevante para leitura como bytes.
                    unsafe {
                        core::slice::from_raw_parts(
                            d as *const PciInfo as *const u8,
                            core::mem::size_of::<PciInfo>(),
                        )
                    }
                    .iter()
                    .copied()
                })
                .collect();
            if let Err(e) = copy_to_user(f.rsi, &bytes) {
                return (e, 0);
            }
            (Status::Ok, devs.len() as u64)
        }
        SYS_PCI_CFG_READ | SYS_PCI_CFG_WRITE => {
            let right = if n == SYS_PCI_CFG_READ {
                RIGHT_READ
            } else {
                RIGHT_WRITE
            };
            let g = match device_grant(p, h, right) {
                Ok(g) => g,
                Err(e) => return (e, 0),
            };
            let bdf = Bdf::from_packed(f.rsi as u16);
            if f.rdx > 0xfc || !f.rdx.is_multiple_of(4) {
                return (Status::InvalidArgs, 0);
            }
            if !g.covers(f.rsi as u16) {
                return (Status::Denied, 0);
            }
            if n == SYS_PCI_CFG_READ {
                (Status::Ok, crate::pci::cfg_read(bdf, f.rdx as u8) as u64)
            } else {
                crate::pci::cfg_write(bdf, f.rdx as u8, f.r10 as u32);
                (Status::Ok, 0)
            }
        }
        SYS_MMIO_MAP => {
            let g = match device_grant(p, h, RIGHT_MAP) {
                Ok(g) => g,
                Err(e) => return (e, 0),
            };
            let (phys, len) = (f.rsi, f.rdx);
            if len == 0 || len > 16 * 1024 * 1024 || !phys.is_multiple_of(PAGE_SIZE) {
                return (Status::InvalidArgs, 0);
            }
            let len = nexo_mm::align_up(len, PAGE_SIZE);
            if !crate::pci::is_mmio_range(g.scope, phys, len) {
                return (Status::Denied, 0);
            }
            let base = p.reserve_device_region(len);
            let mut off = 0;
            while off < len {
                if p.space
                    .map_user_mmio(VirtAddr::new(base + off), PhysAddr::new(phys + off))
                    .is_err()
                {
                    return (Status::NoMemory, 0);
                }
                off += PAGE_SIZE;
            }
            (Status::Ok, base)
        }
        SYS_DMA_ALLOC => {
            if let Err(e) = device_grant(p, h, RIGHT_MAP) {
                return (e, 0);
            }
            let base = p.reserve_device_region(PAGE_SIZE);
            let phys = match p
                .space
                .map_user_page(VirtAddr::new(base), PageFlags::KERNEL_RW)
            {
                Ok(ph) => ph,
                Err(_) => return (Status::NoMemory, 0),
            };
            // SAFETY: quadro recem-alocado, visivel pelo physmap, ainda nao entregue ao usuario.
            unsafe {
                core::ptr::write_bytes(
                    crate::mm::virt::phys_to_virt(phys).as_u64() as *mut u8,
                    0,
                    PAGE_SIZE as usize,
                )
            };
            let b = DmaBuffer {
                virt: base,
                phys: phys.as_u64(),
                len: PAGE_SIZE,
            };
            // SAFETY: DmaBuffer é repr(C) de inteiros.
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    &b as *const DmaBuffer as *const u8,
                    core::mem::size_of::<DmaBuffer>(),
                )
            };
            match copy_to_user(f.rsi, bytes) {
                Ok(()) => (Status::Ok, base),
                Err(e) => (e, 0),
            }
        }
        SYS_IRQ_ALLOC => {
            let g = match device_grant(p, h, RIGHT_SIGNAL) {
                Ok(g) => g,
                Err(e) => return (e, 0),
            };
            let Some(v) = crate::irq::alloc() else {
                return (Status::NoMemory, 0);
            };
            g.vectors.lock().push(v);
            let apic_id = crate::acpi::info().bsp_apic_id as u64;
            let info = IrqInfo {
                vector: v as u32,
                reserved: 0,
                msi_address: 0xfee0_0000 | (apic_id << 12),
                msi_data: v as u32,
                reserved2: 0,
            };
            // SAFETY: IrqInfo é repr(C) de inteiros.
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    &info as *const IrqInfo as *const u8,
                    core::mem::size_of::<IrqInfo>(),
                )
            };
            match copy_to_user(f.rsi, bytes) {
                Ok(()) => (Status::Ok, v as u64),
                Err(e) => (e, 0),
            }
        }
        SYS_IRQ_WAIT => {
            if let Err(e) = device_grant(p, h, RIGHT_SIGNAL) {
                return (e, 0);
            }
            let v = f.rsi;
            if v > 0xff || !crate::irq::is_user_vector(v as u8) {
                return (Status::InvalidArgs, 0);
            }
            (Status::Ok, crate::irq::wait(v as u8, f.rdx))
        }
        SYS_DEVICE_OPEN => {
            let g = match device_grant(p, h, RIGHT_ADMIN) {
                Ok(g) => g,
                Err(e) => return (e, 0),
            };
            let bdf = f.rsi as u16;
            if f.rsi > 0xffff || !g.covers(bdf) {
                return (Status::Denied, 0);
            }
            if !crate::pci::exists(bdf) {
                return (Status::NotFound, 0);
            }
            let handle = Handle {
                object: Object::Device(Arc::new(DeviceGrant::for_device(bdf))),
                rights: Rights(RIGHTS_DEVICE_DEFAULT),
            };
            match p.handles.lock().insert(handle) {
                Ok(i) => (Status::Ok, i as u64),
                Err(e) => (e, 0),
            }
        }
        _ => (Status::NotSupported, 0),
    }
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
                        if s.contains("reiniciando") {
                            RESTART_LOGS.fetch_add(1, Ordering::Relaxed);
                        }
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
            4 => (Status::Ok, process::count() as u64),
            5 => (Status::Ok, crate::mm::phys::stats().free),
            6 => (Status::Ok, crate::mm::phys::stats().total_usable),
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
        SYS_CHANNEL_RECV => sys_channel_recv(&p, f, false),
        SYS_CHANNEL_TRY_RECV => sys_channel_recv(&p, f, true),
        SYS_CHANNEL_WAIT_ANY => sys_channel_wait_any(&p, f),
        SYS_IRQ_CHANNEL => sys_irq_channel(&p, f),
        SYS_MEMORY_CREATE => sys_memory_create(&p, f),
        SYS_MEMORY_MAP => sys_memory_map(&p, f),
        SYS_MEMORY_UNMAP => sys_memory_unmap(&p, f),
        SYS_FB_INFO => sys_fb_info(f),
        SYS_PROCESS_SPAWN => sys_process_spawn(&p, f),
        SYS_PROCESS_SPAWN_MEM => sys_process_spawn_mem(&p, f),
        SYS_PCI_ENUM | SYS_PCI_CFG_READ | SYS_PCI_CFG_WRITE | SYS_MMIO_MAP | SYS_DMA_ALLOC
        | SYS_IRQ_ALLOC | SYS_IRQ_WAIT | SYS_DEVICE_OPEN => sys_device(&p, f),
        SYS_PROCESS_WAIT => {
            let h = p.handles.lock().get(f.rdi as u32);
            match h {
                Ok(Handle {
                    object: Object::Process(target),
                    rights,
                }) => {
                    if !rights.contains(RIGHT_READ) {
                        return (Status::Denied, 0);
                    }
                    if Arc::ptr_eq(&target, &p) {
                        return (Status::InvalidArgs, 0);
                    }
                    let code = process::wait_process(&target);
                    (Status::Ok, code as u64)
                }
                Ok(_) => (Status::InvalidArgs, 0),
                Err(e) => (e, 0),
            }
        }
        SYS_PROCESS_INFO => match p.handles.lock().get(f.rdi as u32) {
            Ok(Handle {
                object: Object::Process(target),
                ..
            }) => {
                let exited = if target.exited.load(Ordering::Acquire) {
                    PROCESS_INFO_EXITED
                } else {
                    0
                };
                (Status::Ok, target.pid | exited)
            }
            Ok(_) => (Status::InvalidArgs, 0),
            Err(e) => (e, 0),
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

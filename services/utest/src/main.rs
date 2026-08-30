//! `utest` — programa de auto-teste de usuário. Recebe um modo em `RDI`:
//! 0 = exercita todas as syscalls e sai com 0;
//! 1 = lê memória do kernel (deve morrer por falta de página);
//! 2 = chama uma syscall inexistente e sai com o status recebido;
//! 3 = executa instrução privilegiada (deve morrer por #GP);
//! 4 = escreve na própria `.rodata` (deve morrer por proteção de escrita);
//! 5 = servidor de IPC (handle 0 = canal): responde "pong:<msg>", recebe um
//!     canal transferido e escreve "hi" nele, sai quando o par fecha;
//! 6 = cliente de IPC (handle 0 = canal): ping/pong, transfere um canal,
//!     testa direitos reduzidos e handles inválidos.
#![no_std]
#![no_main]

use nexo_sys::abi::Status;

static RO: [u64; 4] = [1, 2, 3, 4];

/// Ponto de entrada (pilha alinhada pelo kernel; `RDI` = modo).
#[unsafe(no_mangle)]
pub extern "C" fn _start(mode: u64) -> ! {
    match mode {
        0 => normal(),
        1 => {
            nexo_sys::log("utest: modo 1 — vou ler memoria do kernel");
            // SAFETY: deliberadamente inválido: o kernel deve encerrar este processo.
            let v = unsafe { core::ptr::read_volatile(0xffff_ffff_8000_0000u64 as *const u64) };
            nexo_sys::exit(200 + (v & 1) as i64)
        }
        2 => {
            nexo_sys::log("utest: modo 2 — syscall inexistente");
            // SAFETY: número inválido; o kernel responde com NotSupported.
            let (st, _) = unsafe { nexo_sys::raw(9999, 1, 2, 3) };
            nexo_sys::exit(st as u64 as i64)
        }
        3 => {
            nexo_sys::log("utest: modo 3 — instrucao privilegiada");
            // SAFETY: `cli` em ring 3 gera #GP; o kernel deve encerrar este processo.
            unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
            nexo_sys::exit(201)
        }
        4 => {
            nexo_sys::log("utest: modo 4 — escrita em .rodata");
            // SAFETY: deliberadamente inválido: página somente leitura.
            unsafe { core::ptr::write_volatile(RO.as_ptr() as *mut u64, 99) };
            nexo_sys::exit(202 + (RO[0] & 1) as i64)
        }
        5 => ipc_server(),
        6 => ipc_client(),
        _ => nexo_sys::exit(203),
    }
}

fn ipc_server() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 4];
    let (n, nh) = match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok(v) => v,
        Err(_) => nexo_sys::exit(50),
    };
    if nh != 0 || &buf[..n] != b"ping" {
        nexo_sys::exit(51);
    }
    let mut reply = [0u8; 64];
    reply[..5].copy_from_slice(b"pong:");
    reply[5..5 + n].copy_from_slice(&buf[..n]);
    if nexo_sys::channel_send(ch, &reply[..5 + n], &[]) != Status::Ok {
        nexo_sys::exit(52);
    }
    // Segunda mensagem traz um canal; escreve "hi" nele e o fecha.
    let (n2, nh2) = match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok(v) => v,
        Err(_) => nexo_sys::exit(53),
    };
    if nh2 != 1 || &buf[..n2] != b"take" {
        nexo_sys::exit(54);
    }
    let extra = hs[0];
    if nexo_sys::channel_send(extra, b"hi", &[]) != Status::Ok {
        nexo_sys::exit(55);
    }
    nexo_sys::handle_close(extra);
    // O cliente fecha o seu lado: o próximo recv deve terminar com PeerClosed.
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Err(Status::PeerClosed) => nexo_sys::exit(0),
        _ => nexo_sys::exit(56),
    }
}

fn ipc_client() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 4];
    if nexo_sys::channel_send(ch, b"ping", &[]) != Status::Ok {
        nexo_sys::exit(60);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, 0)) if &buf[..n] == b"pong:ping" => {}
        _ => nexo_sys::exit(61),
    }
    // Transfere uma extremidade de um canal novo ao servidor.
    let (a, b) = match nexo_sys::channel_create() {
        Ok(p) => p,
        Err(_) => nexo_sys::exit(62),
    };
    if nexo_sys::channel_send(ch, b"take", &[b]) != Status::Ok {
        nexo_sys::exit(63);
    }
    if nexo_sys::handle_info(b).is_ok() {
        nexo_sys::exit(64); // o handle enviado deixa a nossa tabela
    }
    match nexo_sys::channel_recv(a, &mut buf, &mut hs) {
        Ok((2, 0)) if &buf[..2] == b"hi" => {}
        _ => nexo_sys::exit(65),
    }
    // Direitos: duplicata somente-leitura não pode enviar; ampliar direitos é proibido.
    let ro = match nexo_sys::handle_duplicate(ch, nexo_sys::abi::RIGHT_READ) {
        Ok(h) => h,
        Err(_) => nexo_sys::exit(66),
    };
    if nexo_sys::channel_send(ro, b"x", &[]) != Status::Denied {
        nexo_sys::exit(67);
    }
    if nexo_sys::handle_duplicate(ro, nexo_sys::abi::RIGHT_WRITE).is_ok() {
        nexo_sys::exit(68);
    }
    if nexo_sys::handle_close(9999) != Status::BadHandle {
        nexo_sys::exit(69);
    }
    if nexo_sys::channel_send(ch, &[0u8; 5000], &[]) != Status::TooBig {
        nexo_sys::exit(70);
    }
    match nexo_sys::handle_info(ch) {
        Ok((rights, kind))
            if kind == nexo_sys::abi::KIND_CHANNEL
                && rights == nexo_sys::abi::RIGHTS_CHANNEL_DEFAULT => {}
        _ => nexo_sys::exit(71),
    }
    nexo_sys::handle_close(ro);
    nexo_sys::handle_close(a);
    nexo_sys::handle_close(ch); // par do servidor vê PeerClosed
    nexo_sys::log("utest: ipc ok");
    nexo_sys::exit(0)
}

fn normal() -> ! {
    let pid = nexo_sys::get_pid();
    if nexo_sys::abi_version() != nexo_sys::abi::ABI_VERSION {
        nexo_sys::exit(11);
    }
    if nexo_sys::log("utest: ola do modo usuario") != Status::Ok {
        nexo_sys::exit(12);
    }
    let t0 = nexo_sys::time_now();
    nexo_sys::yield_now();
    nexo_sys::sleep_ns(2_000_000);
    let t1 = nexo_sys::time_now();
    if t1 < t0 + 2_000_000 {
        nexo_sys::exit(13);
    }
    if nexo_sys::debug_info(0) < 1 {
        nexo_sys::exit(14);
    }
    // Ponteiro inválido em SYS_LOG deve ser recusado, não derrubar nada.
    // SAFETY: o kernel valida o intervalo antes de ler.
    let (st, _) = unsafe { nexo_sys::raw(nexo_sys::abi::SYS_LOG, 0xffff_ffff_8000_0000, 8, 0) };
    if st != Status::BadAddress {
        nexo_sys::exit(15);
    }
    let mut buf = [0u8; 48];
    let msg = fmt_pid(&mut buf, pid);
    nexo_sys::log(msg);
    nexo_sys::exit(0)
}

/// Escreve "init: ok pid=<n> syscalls=<m>" sem alocar.
fn fmt_pid(buf: &mut [u8; 48], pid: u64) -> &str {
    let prefix = b"utest: ok pid=";
    let mut n = 0;
    buf[..prefix.len()].copy_from_slice(prefix);
    n += prefix.len();
    n += write_num(&mut buf[n..], pid);
    let mid = b" syscalls=";
    buf[n..n + mid.len()].copy_from_slice(mid);
    n += mid.len();
    n += write_num(&mut buf[n..], nexo_sys::debug_info(2));
    core::str::from_utf8(&buf[..n]).unwrap_or("utest: ok")
}

fn write_num(out: &mut [u8], mut v: u64) -> usize {
    let mut tmp = [0u8; 20];
    let mut i = tmp.len();
    loop {
        i -= 1;
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let s = &tmp[i..];
    out[..s.len()].copy_from_slice(s);
    s.len()
}

// panic_handler: fornecido por nexo-rt (feature panic-handler).
#[allow(unused_imports)]
use nexo_rt as _;

//! `utest` — programa de auto-teste de usuário. Recebe um modo em `RDI`:
//! 0 = exercita todas as syscalls e sai com 0;
//! 1 = lê memória do kernel (deve morrer por falta de página);
//! 2 = chama uma syscall inexistente e sai com o status recebido;
//! 3 = executa instrução privilegiada (deve morrer por #GP);
//! 4 = escreve na própria `.rodata` (deve morrer por proteção de escrita);
//! 5 = servidor de IPC (handle 0 = canal): responde "pong:<msg>", recebe um
//!     canal transferido e escreve "hi" nele, sai quando o par fecha;
//! 6 = cliente de IPC (handle 0 = canal): ping/pong, transfere um canal,
//!     testa direitos reduzidos e handles inválidos;
//! 7 = fuzz de syscalls: números e argumentos aleatórios (ponteiros nulos,
//!     do kernel, desalinhados, tamanhos absurdos); o processo deve sobreviver.
#![no_std]
#![no_main]

use nexo_sys::abi::Status;

static RO: [u64; 4] = [1, 2, 3, 4];

/// Ponto de entrada (pilha alinhada pelo kernel; `RDI` = modo).
#[unsafe(no_mangle)]
pub extern "C" fn _start(mode: u64) -> ! {
    // Bits 0..8 = modo; bits 8.. = parametro (semente do fuzz no modo 7).
    let param = mode >> 8;
    match mode & 0xff {
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
        7 => syscall_fuzz(param),
        8 => block_client(),
        9 => fs_client(),
        10 => fs_churn(),
        11 => devmgr_client(),
        12 => vfs_client(),
        13 => input_client(),
        14 => net_client(param as u16),
        15 => sock_client(param as u16, (param >> 16) as u16, (param >> 32) as u16),
        16 => wait_any_test(),
        17 => shmem_producer(),
        18 => shmem_consumer(),
        19 => wm_client(),
        20 => wm_multi_client(),
        21 => wm_restack(),
        22 => wm_resize(),
        23 => wm_input(),
        24 => wm_keyboard(),
        25 => wm_alpha(),
        26 => wm_ui(),
        27 => wm_maximize(),
        28 => wm_shortcut(),
        29 => wm_present_client(),
        30 => wm_tile(),
        31 => wm_real_input(),
        32 => wm_grab(),
        33 => wm_displays(),
        34 => greeter_driver(),
        35 => wm_context(),
        36 => wm_clipboard(),
        37 => wm_notify(),
        38 => wm_dnd(),
        39 => wm_a11y(),
        40 => wm_shell(),
        41 => wm_scale(),
        42 => wm_center(),
        43 => shellui_driver(),
        44 => shellcenter_driver(),
        45 => calc_driver(),
        46 => install_client(),
        47 => spawn_mem_client(param as usize),
        48 => launcher_client(param as usize),
        49 => launch_gui_client(param as usize),
        50 => config_driver(),
        51 => monitor_driver(),
        52 => term_driver(),
        53 => visor_driver(),
        54 => wm_real_pointer(),
        55 => wm_merged_input(),
        56 => agenda_driver(),
        57 => consent_driver(param as usize),
        58 => editor_driver(),
        59 => arquivos_driver(),
        60 => portal_driver(),
        61 => handoff_sender(),
        62 => trace_client(),
        63 => block_pipelined_client(),
        64 => shm_quota_client(),
        65 => backup_driver(),
        66 => wm_flip_client(),
        67 => slots_confirm_driver(),
        68 => update_apply_driver(),
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

/// PRNG xorshift (determinístico).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Fuzz de syscalls: nada aqui pode derrubar o kernel nem travar este processo.
fn syscall_fuzz(seed: u64) -> ! {
    use nexo_sys::abi::*;
    let seed = if seed == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        seed
    };
    nexo_rt::log!("utest: fuzz semente {:#x}", seed);
    let mut rng = Rng(seed);
    let buf = [0u8; 4096];
    let mut sink = [0u32; 16];
    let (ch_a, ch_b) = nexo_sys::channel_create().unwrap_or((u32::MAX, u32::MAX));
    let mut ok = 0u64;
    let mut errs = 0u64;
    let iterations = 20_000u64;
    for i in 0..iterations {
        let r = rng.next();
        // Números: válidos (com peso) e inválidos.
        let n = match r % 8 {
            0 => rng.next() % 64,
            1 => rng.next(),
            _ => rng.next() % (SYS_MAX + 1),
        };
        // Bloqueantes/terminais ficam de fora: exit, recv sem par, wait.
        if n == SYS_EXIT
            || n == SYS_CHANNEL_RECV
            || n == SYS_PROCESS_WAIT
            || n == SYS_CHANNEL_WAIT_ANY
            || n == SYS_MEMORY_CREATE
            || n == SYS_MEMORY_UNMAP
        {
            continue;
        }
        let arg = |rng: &mut Rng| -> u64 {
            match rng.next() % 10 {
                0 => 0,
                1 => 0xffff_ffff_8000_0000,
                2 => buf.as_ptr() as u64,
                3 => buf.as_ptr() as u64 + 4095,
                4 => USER_ADDRESS_LIMIT - 8,
                5 => USER_ADDRESS_LIMIT,
                6 => rng.next() % 4097,
                7 => ch_a as u64,
                8 => ch_b as u64,
                _ => rng.next(),
            }
        };
        let (a0, a1, a2, a3, a4) = (
            arg(&mut rng),
            arg(&mut rng),
            arg(&mut rng),
            arg(&mut rng),
            arg(&mut rng),
        );
        // Sleep só com valores curtos.
        let a0 = if n == SYS_SLEEP { a0 % 300_000 } else { a0 };
        // Handles enviados: ponteiro para lista pequena (pode conter lixo).
        let (a3, a4) = if n == SYS_CHANNEL_SEND || n == SYS_PROCESS_SPAWN {
            (sink.as_mut_ptr() as u64, rng.next() % 3)
        } else {
            (a3, a4)
        };
        // SAFETY: o kernel deve validar tudo; e o objetivo do teste.
        let (st, _) = unsafe { nexo_sys::raw5(n, a0, a1, a2, a3, a4) };
        if st.is_ok() {
            ok += 1
        } else {
            errs += 1
        }
        if i % 5000 == 0 {
            nexo_rt::log!(
                "utest: fuzz {}/{} (ok={}, erros={})",
                i,
                iterations,
                ok,
                errs
            );
        }
    }
    // Invariantes finais: a ABI continua respondendo corretamente.
    if nexo_sys::abi_version() != ABI_VERSION || nexo_sys::get_pid() == 0 {
        nexo_sys::exit(90);
    }
    if nexo_sys::log("utest: fuzz ok") != Status::Ok {
        nexo_sys::exit(91);
    }
    nexo_rt::log!(
        "utest: fuzz terminou: ok={} erros={} handles={}",
        ok,
        errs,
        nexo_sys::debug_info(3)
    );
    nexo_sys::exit(0)
}

/// Modo 8: cliente do `blockdev` (handle 0 = canal): escreve e le setores, e
/// verifica/grava um marcador de persistencia no setor 8.
/// Modo 8: cliente do `blockdev` (handle 0 = canal), protocolo tipado `nexo.block`:
/// escreve e le setores na area reservada e verifica/grava o marcador de persistencia.
fn block_client() -> ! {
    use nexo_proto::ProtoError;
    use nexo_proto::block::{
        CapacityRequest, ReadRequest, WriteRequest, decode_capacity_response, decode_read_response,
        decode_write_response,
    };
    let ch: nexo_sys::Handle = 0;
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    fn call<'a>(
        ch: nexo_sys::Handle,
        msg: &'a mut [u8; 4096],
        m: usize,
        hs: &mut [u32; 1],
        code: i64,
    ) -> &'a [u8] {
        if nexo_sys::channel_send(ch, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(code);
        }
        match nexo_sys::channel_recv(ch, msg, hs) {
            Ok((n, _)) => &msg[..n],
            Err(_) => nexo_sys::exit(code + 1),
        }
    }
    // 0. Capacidade: os testes crus usam a area reservada nos ultimos 256 setores
    //    (o `fs` nao a administra), para nao colidir com o volume NexoFS.
    let m = CapacityRequest {}.encode_msg(&mut msg).unwrap_or(0);
    let r = call(ch, &mut msg, m, &mut hs, 108);
    let base = match decode_capacity_response(r) {
        Ok(c) => c.sectors.saturating_sub(256),
        Err(_) => nexo_sys::exit(109),
    };
    // 1. Escreve padrao em 4 setores (2048 bytes) em um pedido.
    let mut w = WriteRequest {
        sector: base,
        count: 4,
        data: [0; 3584],
        data_len: 2048,
    };
    for i in 0..2048usize {
        w.data[i] = (i as u8) ^ 0x5a ^ ((i / 512) as u8);
    }
    let m = w.encode_msg(&mut msg).unwrap_or(0);
    let r = call(ch, &mut msg, m, &mut hs, 110);
    if decode_write_response(r).is_err() {
        nexo_rt::log!("utest: escrita falhou");
        nexo_sys::exit(111);
    }
    // 2. Le de volta e compara.
    let m = ReadRequest {
        sector: base,
        count: 4,
    }
    .encode_msg(&mut msg)
    .unwrap_or(0);
    let r = call(ch, &mut msg, m, &mut hs, 113);
    match decode_read_response(r) {
        Ok(resp) if resp.data().len() == 2048 => {
            for (i, &b) in resp.data().iter().enumerate() {
                if b != (i as u8) ^ 0x5a ^ ((i / 512) as u8) {
                    nexo_rt::log!("utest: dado divergente no byte {}", i);
                    nexo_sys::exit(116);
                }
            }
        }
        _ => nexo_sys::exit(114),
    }
    // 3. Marcador de persistencia no setor base+8.
    let marker = b"NEXO-PERSIST-v1";
    let m = ReadRequest {
        sector: base + 8,
        count: 1,
    }
    .encode_msg(&mut msg)
    .unwrap_or(0);
    let r = call(ch, &mut msg, m, &mut hs, 117);
    match decode_read_response(r) {
        Ok(resp) if resp.data().len() == 512 => {
            if resp.data()[..marker.len()] == marker[..] {
                nexo_sys::log("utest: bloco persistente encontrado (boot anterior)");
            } else {
                let mut w = WriteRequest {
                    sector: base + 8,
                    count: 1,
                    data: [0; 3584],
                    data_len: 512,
                };
                w.data[..marker.len()].copy_from_slice(marker);
                let m = w.encode_msg(&mut msg).unwrap_or(0);
                let r = call(ch, &mut msg, m, &mut hs, 118);
                if decode_write_response(r).is_err() {
                    nexo_sys::exit(119);
                }
                nexo_sys::log("utest: marcador de persistencia gravado");
            }
        }
        _ => nexo_sys::exit(120),
    }
    // 4. Pipelining: 4 leituras encadeadas sem esperar respostas; chegam na ordem e corretas.
    for i in 0..4u64 {
        let m = ReadRequest {
            sector: base + i,
            count: 1,
        }
        .encode_msg(&mut msg)
        .unwrap_or(0);
        if nexo_sys::channel_send(ch, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(122);
        }
    }
    for i in 0..4usize {
        let n = match nexo_sys::channel_recv(ch, &mut msg, &mut hs) {
            Ok((n, _)) => n,
            Err(_) => nexo_sys::exit(123),
        };
        match decode_read_response(&msg[..n]) {
            Ok(resp) if resp.data().len() == 512 => {
                // setores base..base+3 tem o padrao (i^0x5a^(setor)) escrito no passo 1
                for (j, &b) in resp.data().iter().enumerate() {
                    let full = i * 512 + j;
                    if b != (full as u8) ^ 0x5a ^ ((full / 512) as u8) {
                        nexo_rt::log!("utest: pipeline: resposta {} fora de ordem/corrompida", i);
                        nexo_sys::exit(124);
                    }
                }
            }
            _ => nexo_sys::exit(125),
        }
    }
    nexo_sys::log("utest: bloco pipeline ok (4 em voo)");
    // 5. Pedido invalido (alem da capacidade) deve ser recusado (erro tipado 2), nao derrubar o driver.
    let m = ReadRequest {
        sector: u64::MAX / 2,
        count: 1,
    }
    .encode_msg(&mut msg)
    .unwrap_or(0);
    let r = call(ch, &mut msg, m, &mut hs, 121);
    if decode_read_response(r) != Err(ProtoError::Remote(2)) {
        nexo_sys::exit(121);
    }
    nexo_sys::log("utest: bloco ok");
    nexo_sys::exit(0)
}

/// Cliente `nexo.fs` v0 (handle 0 = canal para o `fs`).
struct FsClient {
    ch: nexo_sys::Handle,
    req: [u8; 4096],
    reply: [u8; 4096],
}

impl FsClient {
    /// Envia a operacao pelo protocolo tipado `nexo.fs` e devolve (status, valor, dados em
    /// `reply[12..]`) no formato legado: 0 stat, 1 create, 2 mkdir, 3 unlink, 4 read, 5 write,
    /// 6 list, 7 sync, 8 info, 9 truncate.
    fn call(
        &mut self,
        op: u8,
        ino: u32,
        offset: u64,
        len: u32,
        payload: &[u8],
    ) -> (u8, u64, usize) {
        use nexo_proto::ProtoError;
        use nexo_proto::fs as pfs;
        fn path_of(payload: &[u8]) -> ([u8; 256], u32) {
            let mut p = [0u8; 256];
            let n = payload.len().min(256);
            p[..n].copy_from_slice(&payload[..n]);
            (p, n as u32)
        }
        let m = match op {
            0 | 1 | 2 | 3 | 6 => {
                let (path, path_len) = path_of(payload);
                match op {
                    0 => pfs::StatRequest { path, path_len }.encode_msg(&mut self.req),
                    1 => pfs::CreateRequest { path, path_len }.encode_msg(&mut self.req),
                    2 => pfs::MkdirRequest { path, path_len }.encode_msg(&mut self.req),
                    3 => pfs::UnlinkRequest { path, path_len }.encode_msg(&mut self.req),
                    _ => pfs::ListRequest { path, path_len }.encode_msg(&mut self.req),
                }
            }
            4 => pfs::ReadRequest { ino, offset, len }.encode_msg(&mut self.req),
            5 => {
                let dn = payload.len().min(3900);
                let mut rq = pfs::WriteRequest {
                    ino,
                    offset,
                    data: [0; 3900],
                    data_len: dn as u32,
                };
                rq.data[..dn].copy_from_slice(&payload[..dn]);
                rq.encode_msg(&mut self.req)
            }
            7 => pfs::SyncRequest {}.encode_msg(&mut self.req),
            8 => pfs::InfoRequest {}.encode_msg(&mut self.req),
            9 => pfs::TruncateRequest { ino, size: offset }.encode_msg(&mut self.req),
            _ => return (0xfe, 0, 0),
        }
        .unwrap_or(0);
        if nexo_sys::channel_send(self.ch, &self.req[..m], &[]) != Status::Ok {
            nexo_sys::exit(130);
        }
        let mut hs = [0u32; 1];
        let n = match nexo_sys::channel_recv(self.ch, &mut self.reply, &mut hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(131),
        };
        fn remote(e: ProtoError) -> (u8, u64, usize) {
            match e {
                ProtoError::Remote(c) => (c as u8, 0, 0),
                _ => (0xfd, 0, 0),
            }
        }
        let copy: [u8; 4096] = self.reply;
        let msg = &copy[..n];
        match op {
            0 => match pfs::decode_stat_response(msg) {
                Ok(r) => {
                    self.reply[12] = r.kind;
                    self.reply[13..21].copy_from_slice(&r.size.to_le_bytes());
                    (0, r.ino as u64, 9)
                }
                Err(e) => remote(e),
            },
            1 => match pfs::decode_create_response(msg) {
                Ok(r) => (0, r.ino as u64, 0),
                Err(e) => remote(e),
            },
            2 => match pfs::decode_mkdir_response(msg) {
                Ok(r) => (0, r.ino as u64, 0),
                Err(e) => remote(e),
            },
            3 => match pfs::decode_unlink_response(msg) {
                Ok(_) => (0, 0, 0),
                Err(e) => remote(e),
            },
            4 => match pfs::decode_read_response(msg) {
                Ok(r) => {
                    let dl = r.data().len();
                    self.reply[12..12 + dl].copy_from_slice(r.data());
                    (0, dl as u64, dl)
                }
                Err(e) => remote(e),
            },
            5 => match pfs::decode_write_response(msg) {
                Ok(r) => (0, r.written as u64, 0),
                Err(e) => remote(e),
            },
            6 => match pfs::decode_list_response(msg) {
                Ok(r) => {
                    let dl = r.entries().len();
                    self.reply[12..12 + dl].copy_from_slice(r.entries());
                    (0, r.count as u64, dl)
                }
                Err(e) => remote(e),
            },
            7 => match pfs::decode_sync_response(msg) {
                Ok(_) => (0, 0, 0),
                Err(e) => remote(e),
            },
            8 => match pfs::decode_info_response(msg) {
                Ok(r) => {
                    self.reply[12..20].copy_from_slice(&r.total_blocks.to_le_bytes());
                    self.reply[20..28].copy_from_slice(&r.free_blocks.to_le_bytes());
                    self.reply[28..36].copy_from_slice(&r.repairs.to_le_bytes());
                    self.reply[36..44].copy_from_slice(&r.generation.to_le_bytes());
                    (0, 0, 32)
                }
                Err(e) => remote(e),
            },
            _ => match pfs::decode_truncate_response(msg) {
                Ok(_) => (0, 0, 0),
                Err(e) => remote(e),
            },
        }
    }
    fn ok(
        &mut self,
        op: u8,
        ino: u32,
        offset: u64,
        len: u32,
        payload: &[u8],
        code: i64,
    ) -> (u64, usize) {
        let (st, v, n) = self.call(op, ino, offset, len, payload);
        if st != 0 {
            nexo_rt::log!("utest: fs: op {} falhou com status {}", op, st);
            nexo_sys::exit(code);
        }
        (v, n)
    }
    fn data(&self, n: usize) -> &[u8] {
        &self.reply[12..12 + n]
    }
}

/// Modo 50: driver das Configuracoes. Handle 0 = sessao nexo.wm (bootstrap), handle 1 = canal com
/// o app config. Clica nos toggles e confere os efeitos REAIS: prefs{} reflete o movimento
/// reduzido; com nao-perturbe ligado, um aviso nao desenha banner (pixel intacto).
/// Modo 51: monitor de sistema. Abre uma sessao para o monitor, espera "pronto" e verifica na
/// saida composta: as quatro celulas de estatistica verdes (kernel respondeu com valores saos) e
/// o heartbeat alternando entre branco e magenta (o monitor esta vivo, relendo e recomitando).
fn monitor_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    // fundo azul cobrindo a tela
    let (bg, bg_base) = wm_create(s1, 0, 0, 64, 48, 0);
    wm_fill(bg_base, 64, 48, 0, 0, 255);
    wm_commit(s1, bg);

    // sessao para o monitor
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1360));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1361));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1362);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1363),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1364);
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"pronto" => {}
        _ => nexo_sys::exit(1365),
    }

    // saida composta
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1366));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1367);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1368));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1369));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1370));
    let stride = outp.w;

    // janela do monitor em (8,8); celula k em (10+8k, 10); heartbeat em (42,10)
    for k in 0..4 {
        wm_wait_px(ob, stride, 10 + 8 * k, 10, (0, 200, 0), 1371 + k as i64);
    }
    wm_wait_px(ob, stride, 42, 10, (255, 255, 255), 1375);
    wm_wait_px(ob, stride, 42, 10, (255, 0, 255), 1376);
    wm_wait_px(ob, stride, 42, 10, (255, 255, 255), 1377);

    nexo_sys::log("utest: monitor ok — estatisticas do kernel saas e heartbeat vivo");
    nexo_sys::exit(0)
}

/// Espelho da grade do terminal (mesma semantica de `services/term`): e a especificacao do
/// que deve aparecer na tela, alimentada com o fluxo de bytes que o shell comprovadamente emite.
const TCOLS: usize = 8;
const TROWS: usize = 6;
struct TGrid {
    cells: [[u8; TCOLS]; TROWS],
    cx: usize,
    cy: usize,
}
impl TGrid {
    fn new() -> Self {
        TGrid {
            cells: [[b' '; TCOLS]; TROWS],
            cx: 0,
            cy: 0,
        }
    }
    fn newline(&mut self) {
        self.cy += 1;
        if self.cy == TROWS {
            self.cells.copy_within(1.., 0);
            self.cells[TROWS - 1] = [b' '; TCOLS];
            self.cy = TROWS - 1;
        }
    }
    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match b {
                b'\r' => self.cx = 0,
                b'\n' => self.newline(),
                0x08 => self.cx = self.cx.saturating_sub(1),
                0x20..=0x7e => {
                    self.cells[self.cy][self.cx] = b;
                    self.cx += 1;
                    if self.cx == TCOLS {
                        self.cx = 0;
                        self.newline();
                    }
                }
                _ => {}
            }
        }
    }
    fn find_row(&self, want: &[u8; TCOLS]) -> Option<usize> {
        (0..TROWS).rev().find(|&r| &self.cells[r] == want)
    }
}

/// Primeiro pixel aceso do glifo de `want` que esteja apagado no glifo de `prev` — um pixel que
/// comprovadamente muda quando a celula troca de `prev` para `want`.
fn glyph_diff_pixel(want: u8, prev: u8) -> Option<(i32, i32)> {
    let gw = nexo_font::glyph(want as char);
    let gp = if (0x20..0x7f).contains(&prev) {
        *nexo_font::glyph(prev as char)
    } else {
        [0u8; 8]
    };
    for row in 0..8 {
        for col in 0..8 {
            let bit = 0x80u8 >> col;
            if gw[row] & bit != 0 && gp[row] & bit == 0 {
                return Some((col, row as i32));
            }
        }
    }
    None
}

/// Digita uma linha no canal de entrada sintetico (pressao+soltura por tecla) terminando em Enter.
fn term_type(inj: nexo_sys::Handle, text: &[u8]) {
    for &ch in text {
        let code: u16 = match ch {
            b'a'..=b'z' => {
                const QW: &[u8] = b"qwertyuiop";
                const AS: &[u8] = b"asdfghjkl";
                const ZX: &[u8] = b"zxcvbnm";
                if let Some(i) = QW.iter().position(|&c| c == ch) {
                    16 + i as u16
                } else if let Some(i) = AS.iter().position(|&c| c == ch) {
                    30 + i as u16
                } else {
                    44 + ZX.iter().position(|&c| c == ch).unwrap_or(0) as u16
                }
            }
            b' ' => 57,
            _ => continue,
        };
        wm_key(inj, code, 1);
        wm_key(inj, code, 0);
    }
    wm_key(inj, 28, 1);
    wm_key(inj, 28, 0);
}

/// Modo 52: terminal grafico hospedando o shell de diagnostico real. Abre a sessao do terminal,
/// injeta teclas pelo canal de entrada do compositor (so a janela em foco as recebe), e confere
/// na saida composta os glifos que o shell mandou desenhar: "ola" do `eco` e o "ate mais" do
/// `sair` — fim de linha a fim de linha, do teclado ao pixel.
fn term_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    // sessao para o terminal (a janela dele e a primeira: recebe o foco ao nascer)
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1380));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1381));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1382);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1383),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1384);
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"pronto" => {}
        _ => nexo_sys::exit(1385),
    }

    // saida composta + canal de entrada sintetico
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1386));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1387);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1388));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1389));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1390));
    let stride = outp.w;
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1391));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1392));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1393);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1394),
    }

    // fase 1: "eco ola" — o fluxo que o shell emite e deterministico
    let mut g = TGrid::new();
    g.feed(b"\r\nNexo OS - shell de diagnostico (digite 'ajuda')\r\n> ");
    g.feed(b"eco ola\r\nola\r\n> ");
    let r = g
        .find_row(b"ola     ")
        .unwrap_or_else(|| nexo_sys::exit(1395));
    if g.cells[r + 1][0] != b'>' {
        nexo_sys::exit(1396);
    }
    term_type(inj, b"eco ola");
    for (c, ch) in (*b"ola").into_iter().enumerate() {
        let (px, py) = glyph_diff_pixel(ch, b' ').unwrap_or_else(|| nexo_sys::exit(1397));
        wm_wait_px(
            ob,
            stride,
            (c * 8) as i32 + px,
            (r * 8) as i32 + py,
            (255, 255, 255),
            1398,
        );
    }
    let (px, py) = glyph_diff_pixel(b'>', b' ').unwrap_or_else(|| nexo_sys::exit(1399));
    wm_wait_px(
        ob,
        stride,
        px,
        ((r + 1) * 8) as i32 + py,
        (255, 255, 255),
        1400,
    );

    // fase 2: "sair" — o shell despede-se e sai; o term detecta o console fechado e avisa
    // "fim" pelo pipe (pixels rolam durante os ecos e nao servem de sinal de encerramento)
    term_type(inj, b"sair");
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"fim" => {}
        _ => nexo_sys::exit(1401),
    }

    nexo_sys::log("utest: term ok — shell real numa janela, do teclado ao pixel");
    nexo_sys::exit(0)
}

/// Modo 53: visualizador de imagens. Escreve um PPM P6 com quadrantes coloridos no NexoFS
/// real (idempotente entre boots), entrega ao visor uma sessao do compositor e o canal do fs
/// com "abre <caminho>", e confere os quatro quadrantes na saida composta.
fn visor_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    // imagem de teste: 16x12, quadrantes TL vermelho / TR verde / BL azul / BR branco
    const IW: u32 = 16;
    const IH: u32 = 12;
    let mut ppm = [0u8; 16 + (IW * IH * 3) as usize];
    let hdr = b"P6\n16 12\n255\n";
    ppm[..hdr.len()].copy_from_slice(hdr);
    let mut o = hdr.len();
    for y in 0..IH {
        for x in 0..IW {
            let c: (u8, u8, u8) = match (x >= IW / 2, y >= IH / 2) {
                (false, false) => (200, 0, 0),
                (true, false) => (0, 200, 0),
                (false, true) => (0, 0, 200),
                (true, true) => (255, 255, 255),
            };
            ppm[o] = c.0;
            ppm[o + 1] = c.1;
            ppm[o + 2] = c.2;
            o += 3;
        }
    }
    let mut fsc = FsClient {
        ch: 2,
        req: [0; 4096],
        reply: [0; 4096],
    };
    {
        use nexo_inst::AppFs;
        let mut afs = InstFs { c: &mut fsc };
        afs.write_file("/visor-teste.ppm", &ppm[..o])
            .unwrap_or_else(|_| nexo_sys::exit(1410));
    }

    // sessao para o visor + canal do fs com o caminho
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1411));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1412));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1413);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1414),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1415);
    }
    if nexo_sys::channel_send(pipe, b"abre /visor-teste.ppm", &[2]) != Status::Ok {
        nexo_sys::exit(1416);
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"pronto" => {}
        _ => nexo_sys::exit(1417),
    }

    // saida composta: janela em (8,8) — um ponto no miolo de cada quadrante
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1418));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1419);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1420));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1421));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1422));
    let stride = outp.w;
    wm_wait_px(ob, stride, 8 + 3, 8 + 3, (200, 0, 0), 1423);
    wm_wait_px(ob, stride, 8 + 12, 8 + 3, (0, 200, 0), 1424);
    wm_wait_px(ob, stride, 8 + 3, 8 + 9, (0, 0, 200), 1425);
    wm_wait_px(ob, stride, 8 + 12, 8 + 9, (255, 255, 255), 1426);

    nexo_sys::log("utest: visor ok — PPM do NexoFS decodificado e apresentado");
    nexo_sys::exit(0)
}

/// Modo 56: calendario. Le o relogio de parede do kernel (debug_info 7), computa com a
/// nexo-cal a mesma grade que a agenda deve pintar e confere na saida composta: hoje em acento,
/// o dia 1 em cinza e o slot alem do ultimo dia vazio (fundo).
fn agenda_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    let epoch = nexo_sys::debug_info(7);
    if epoch == 0 {
        nexo_sys::exit(1430); // ambiente de teste tem RTC
    }
    let today = nexo_cal::civil_from_epoch(epoch);
    let first_slot =
        nexo_cal::weekday_from_days(nexo_cal::days_from_civil(today.year, today.month, 1));
    let ndays = nexo_cal::days_in_month(today.year, today.month);

    // sessao para a agenda
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1431));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1432));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1433);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1434),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1435);
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"pronto" => {}
        _ => nexo_sys::exit(1436),
    }

    // saida composta
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1437));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1438);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1439));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1440));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1441));
    let stride = outp.w;

    // centro da celula do slot k: (1 + col*9 + 4, 1 + row*7 + 3), janela da agenda em (0,0)
    let center = |slot: u8| -> (i32, i32) {
        let (col, row) = (slot as i32 % 7, slot as i32 / 7);
        (1 + col * 9 + 4, 1 + row * 7 + 3)
    };
    let today_slot = first_slot + today.day - 1;
    let (tx, ty) = center(today_slot);
    wm_wait_px(ob, stride, tx, ty, (0x6f, 0x9f, 0xff), 1442); // hoje: acento
    if today.day != 1 {
        let (fx, fy) = center(first_slot);
        wm_wait_px(ob, stride, fx, fy, (0x50, 0x55, 0x60), 1443); // dia 1: cinza
    }
    let after = first_slot + ndays;
    if after < 42 {
        let (ax, ay) = center(after);
        wm_wait_px(ob, stride, ax, ay, (0x14, 0x15, 0x18), 1444); // depois do fim: fundo
    }

    nexo_sys::log("utest: agenda ok — mes real do RTC, hoje em acento na grade");
    nexo_sys::exit(0)
}

/// Modo 58: editor de texto. Escreve /nota.txt, entrega ao editor a sessao e o canal do fs,
/// digita "mundo" (com um typo corrigido por backspace) pela entrada sintetica do compositor,
/// salva com F2 e — depois que o editor DEVOLVE o canal do fs no "fecha" — re-le o arquivo e
/// confere o conteudo salvo. Handles: 0 wm (bootstrap), 1 pipe do editor, 2 fs.
fn editor_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    {
        let mut c = FsClient {
            ch: 2,
            req: [0; 4096],
            reply: [0; 4096],
        };
        let mut fs = InstFs { c: &mut c };
        nexo_inst::AppFs::write_file(&mut fs, "/nota.txt", b"ola\n")
            .unwrap_or_else(|_| nexo_sys::exit(1480));
    }

    // sessao para o editor + entrada sintetica
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1481));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1482));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1483);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1484),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1485);
    }
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1486));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1487));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1488);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1489),
    }
    if nexo_sys::channel_send(pipe, b"abre /nota.txt", &[2]) != Status::Ok {
        nexo_sys::exit(1490);
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"pronto" => {}
        _ => nexo_sys::exit(1491),
    }

    // saida composta: o conteudo inicial aparece ('o' de "ola" na celula (0,0))
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1492));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1493);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1494));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1495));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1496));
    let stride = outp.w;
    let (px, py) = glyph_diff_pixel(b'o', b' ').unwrap_or_else(|| nexo_sys::exit(1497));
    wm_wait_px(ob, stride, px, py, (255, 255, 255), 1498);

    // digita "mundoq" + backspace (typo corrigido) e confere a 2a linha: "mundo"
    for code in [50u16, 22, 49, 32, 24, 16, 14] {
        wm_key(inj, code, 1);
        wm_key(inj, code, 0);
    }
    let (mx, my) = glyph_diff_pixel(b'm', b' ').unwrap_or_else(|| nexo_sys::exit(1499));
    wm_wait_px(ob, stride, mx, 8 + my, (255, 255, 255), 1500);
    let (ox, oy) = glyph_diff_pixel(b'o', b' ').unwrap_or_else(|| nexo_sys::exit(1501));
    wm_wait_px(ob, stride, 4 * 8 + ox, 8 + oy, (255, 255, 255), 1502);

    // F2 salva; o editor confirma
    wm_key(inj, 60, 1);
    wm_key(inj, 60, 0);
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"salvo" => {}
        _ => nexo_sys::exit(1503),
    }

    // fecha: o canal do fs volta e o arquivo salvo e conferido de fora
    if nexo_sys::channel_send(pipe, b"fecha", &[]) != Status::Ok {
        nexo_sys::exit(1504);
    }
    let fs_back = match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"fs" => hs[0],
        _ => nexo_sys::exit(1505),
    };
    let mut c = FsClient {
        ch: fs_back,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut fs = InstFs { c: &mut c };
    let mut back = [0u8; 64];
    let n = nexo_inst::AppFs::read_file(&mut fs, "/nota.txt", &mut back)
        .unwrap_or_else(|_| nexo_sys::exit(1506));
    if &back[..n] != b"ola\nmundo" {
        nexo_sys::exit(1507);
    }

    nexo_sys::log("utest: editor ok — digitado, salvo e conferido no NexoFS real");
    nexo_sys::exit(0)
}

/// Modo 59: gerenciador de arquivos. Prepara /fm-teste (a.txt + sub/c.txt), lista o diretorio por
/// conta propria para saber a ordem, entrega o fs ao app e navega CLICANDO: entrar em "sub"
/// emite "pasta /fm-teste/sub" e a listagem muda; clicar em "c.txt" emite "abrir /fm-teste/sub/c.txt"
/// — o gerenciador aponta, quem abre e o orquestrador. Handles: 0 wm, 1 pipe, 2 fs.
fn arquivos_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    // prepara o diretorio e memoriza a ordem das entradas de /fm-teste
    let mut names = [[0u8; 24]; 6];
    let mut lens = [0usize; 6];
    let mut kinds = [0u8; 6];
    let mut count = 0usize;
    {
        let mut c = FsClient {
            ch: 2,
            req: [0; 4096],
            reply: [0; 4096],
        };
        {
            let mut fs = InstFs { c: &mut c };
            use nexo_inst::AppFs;
            fs.mkdir("/fm-teste")
                .unwrap_or_else(|_| nexo_sys::exit(1510));
            fs.write_file("/fm-teste/a.txt", b"A")
                .unwrap_or_else(|_| nexo_sys::exit(1511));
            fs.mkdir("/fm-teste/sub")
                .unwrap_or_else(|_| nexo_sys::exit(1512));
            fs.write_file("/fm-teste/sub/c.txt", b"C")
                .unwrap_or_else(|_| nexo_sys::exit(1513));
        }
        let (st, _, dl) = c.call(6, 0, 0, 0, b"/fm-teste");
        if st != 0 {
            nexo_sys::exit(1514);
        }
        let entries: [u8; 4096] = c.reply;
        let mut pos = 12usize;
        let end = 12 + dl;
        while pos + 6 <= end && count < 6 {
            let kind = entries[pos + 4];
            let nl = entries[pos + 5] as usize;
            if pos + 6 + nl > end {
                break;
            }
            lens[count] = nl.min(24);
            names[count][..lens[count]].copy_from_slice(&entries[pos + 6..pos + 6 + lens[count]]);
            kinds[count] = kind;
            count += 1;
            pos += 6 + nl;
        }
        if count < 2 {
            nexo_sys::exit(1515);
        }
    }
    let row_of = |want: &[u8]| -> usize {
        for r in 0..count {
            if &names[r][..lens[r]] == want {
                return r;
            }
        }
        nexo_sys::exit(1516)
    };

    // sessao para o app + entrada sintetica
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1517));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1518));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1519);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1520),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1521);
    }
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1522));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1523));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1524);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1525),
    }
    if nexo_sys::channel_send(pipe, b"abre /fm-teste", &[2]) != Status::Ok {
        nexo_sys::exit(1526);
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"pronto" => {}
        _ => nexo_sys::exit(1527),
    }

    // saida composta: a primeira entrada esta na linha 0, na cor do seu tipo
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1528));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1529);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1530));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1531));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1532));
    let stride = outp.w;
    let first_color = if kinds[0] == 2 {
        (0x6f, 0x9f, 0xff)
    } else {
        (255, 255, 255)
    };
    let (gx, gy) = glyph_diff_pixel(names[0][0], b' ').unwrap_or_else(|| nexo_sys::exit(1533));
    wm_wait_px(ob, stride, gx, gy, first_color, 1534);

    // clica em "sub": navegacao emite "pasta /fm-teste/sub" e a listagem passa a mostrar c.txt
    let r_sub = row_of(b"sub");
    wm_click(inj, 4, (r_sub as i32) * 8 + 4);
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"pasta /fm-teste/sub" => {}
        _ => nexo_sys::exit(1535),
    }
    let (cx, cy) = glyph_diff_pixel(b'c', b' ').unwrap_or_else(|| nexo_sys::exit(1536));
    wm_wait_px(ob, stride, cx, cy, (255, 255, 255), 1537);

    // clica em "c.txt": o app pede ao orquestrador que abra
    wm_click(inj, 4, 4);
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"abrir /fm-teste/sub/c.txt" => {}
        _ => nexo_sys::exit(1538),
    }

    nexo_sys::log("utest: arquivos ok — navegou por clique e delegou a abertura");
    nexo_sys::exit(0)
}

/// Modo 60: portal de arquivos. O driver faz DOIS papeis: o app (que so tem o canal do
/// portal e pede "escolhe") e o usuario (que clica na lista do portal). O app recebe apenas o
/// CONTEUDO do arquivo escolhido — nunca o fs, nunca o caminho. Handles: 0 wm, 1 pipe, 2 fs.
fn portal_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 4096];
    let mut hs = [0u32; 1];

    // conteudo conhecido + ordem das entradas de /portal-teste (diretorio exclusivo deste teste)
    let mut names = [[0u8; 24]; 6];
    let mut lens = [0usize; 6];
    let mut count = 0usize;
    {
        let mut c = FsClient {
            ch: 2,
            req: [0; 4096],
            reply: [0; 4096],
        };
        {
            let mut fs = InstFs { c: &mut c };
            use nexo_inst::AppFs;
            fs.mkdir("/portal-teste")
                .unwrap_or_else(|_| nexo_sys::exit(1550));
            fs.write_file("/portal-teste/a.txt", b"portal-conteudo")
                .unwrap_or_else(|_| nexo_sys::exit(1551));
        }
        let (st, _, dl) = c.call(6, 0, 0, 0, b"/portal-teste");
        if st != 0 {
            nexo_sys::exit(1552);
        }
        let entries: [u8; 4096] = c.reply;
        let mut pos = 12usize;
        let end = 12 + dl;
        while pos + 6 <= end && count < 6 {
            let nl = entries[pos + 5] as usize;
            if pos + 6 + nl > end {
                break;
            }
            lens[count] = nl.min(24);
            names[count][..lens[count]].copy_from_slice(&entries[pos + 6..pos + 6 + lens[count]]);
            count += 1;
            pos += 6 + nl;
        }
    }
    let mut r_a = usize::MAX;
    for r in 0..count {
        if &names[r][..lens[r]] == b"a.txt" {
            r_a = r;
        }
    }
    if r_a == usize::MAX {
        nexo_sys::exit(1553);
    }

    // sessao + entrada sintetica + fiacao do portal
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1554));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1555));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1556);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1557),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1558);
    }
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1559));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1560));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1561);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1562),
    }
    if nexo_sys::channel_send(pipe, b"serve /portal-teste", &[2]) != Status::Ok {
        nexo_sys::exit(1563);
    }
    // o "app": um par de canais; o portal fica com uma ponta
    let (app, portal_end) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1564));
    if nexo_sys::channel_send(pipe, b"cliente", &[portal_end]) != Status::Ok {
        nexo_sys::exit(1565);
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"pronto" => {}
        _ => nexo_sys::exit(1566),
    }

    // papel de app: pede um arquivo
    if nexo_sys::channel_send(app, b"escolhe", &[]) != Status::Ok {
        nexo_sys::exit(1567);
    }

    // papel de usuario: espera a lista aparecer e clica em a.txt
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1568));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1569);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1570));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1571));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1572));
    let stride = outp.w;
    let (gx, gy) = glyph_diff_pixel(b'a', b' ').unwrap_or_else(|| nexo_sys::exit(1573));
    wm_wait_px(ob, stride, gx, (r_a as i32) * 8 + gy, (255, 255, 255), 1574);
    wm_click(inj, 4, (r_a as i32) * 8 + 4);

    // papel de app: recebe SO o conteudo
    match nexo_sys::channel_recv(app, &mut buf, &mut hs) {
        Ok((n, 0)) if &buf[..n] == b"portal-conteudo" => {}
        _ => nexo_sys::exit(1575),
    }

    nexo_sys::log("utest: portal ok — o app recebeu o conteudo; o fs ficou no portal");
    nexo_sys::exit(0)
}

/// Modo 61: remetente do handoff de handle. Envia a ponta (handle 1) pelo pipe (handle 0) e
/// SAI IMEDIATAMENTE — a mensagem (com o handle dentro) fica em transito durante o exit, e o
/// coletor de pontas que roda na saida do processo NAO pode fecha-la (regressao do bug de
/// campo: fs via "cliente desconectou" com o canal em transito).
fn handoff_sender() -> ! {
    if nexo_sys::channel_send(0, b"h", &[1]) != Status::Ok {
        nexo_sys::exit(770);
    }
    nexo_sys::exit(0)
}

/// Modo 62: trace de syscalls. Liga o trace, executa um numero conhecido de yields, le o anel
/// e confere que os proprios yields aparecem (pid proprio, nr correto, TSC crescente); despeja
/// os ultimos eventos em texto no log serial (formato do tools/nexo-trace) e desliga.
fn trace_client() -> ! {
    use nexo_sys::abi::SYS_YIELD;
    let pid = nexo_sys::get_pid();
    const DEBUG: nexo_sys::Handle = 0; // capability de depuracao entregue pelo kernel
    // sem a capability (handle inexistente): NEGADO — o anel e global e nao vaza de graca
    if nexo_sys::trace_enable(true, 999) != Status::Denied {
        nexo_sys::exit(786);
    }
    if nexo_sys::trace_enable(true, DEBUG) != Status::Ok {
        nexo_sys::exit(780);
    }
    const N: usize = 50;
    for _ in 0..N {
        nexo_sys::yield_now();
    }
    static mut EVS: [nexo_sys::TraceEvent; 4096] = [nexo_sys::TraceEvent {
        tsc: 0,
        pid: 0,
        nr: 0,
        reserved: 0,
    }; 4096];
    // SAFETY: unico acesso, processo de uma so thread; buffer estatico (64 KiB nao cabem na pilha).
    let evs = unsafe { &mut *core::ptr::addr_of_mut!(EVS) };
    if nexo_sys::trace_read(evs, 999).is_ok() {
        nexo_sys::exit(787); // leitura sem capability tem de falhar
    }
    let got = nexo_sys::trace_read(evs, DEBUG).unwrap_or_else(|_| nexo_sys::exit(781));
    if nexo_sys::trace_enable(false, DEBUG) != Status::Ok {
        nexo_sys::exit(782);
    }
    if got == 0 || nexo_sys::trace_recorded() == 0 {
        nexo_sys::exit(783);
    }
    let mut meus = 0usize;
    let mut last_tsc = 0u64;
    let mut monotonico = true;
    for e in &evs[..got] {
        if e.pid as u64 == pid && e.nr as u64 == SYS_YIELD {
            meus += 1;
            if e.tsc < last_tsc {
                monotonico = false;
            }
            last_tsc = e.tsc;
        }
    }
    if meus < N {
        nexo_rt::log!(
            "utest: trace: so {} yields proprios (esperava >= {})",
            meus,
            N
        );
        nexo_sys::exit(784);
    }
    if !monotonico {
        nexo_sys::exit(785);
    }
    // amostra em texto para o visualizador do host (tools/nexo-trace)
    for e in &evs[got.saturating_sub(5)..got] {
        nexo_rt::log!("[TRACE] tsc={} pid={} nr={}", e.tsc, e.pid, e.nr);
    }
    nexo_rt::log!(
        "utest: trace ok — {} eventos lidos, {} yields proprios, tsc monotonico",
        got,
        meus
    );
    nexo_sys::exit(0)
}

/// Modo 63: cliente PIPELINED do nexo.block. Dispara 4 escritas + capacidade + 4 leituras
/// SEM esperar respostas (o canal enfileira; o driver mantem varios pedidos em voo) e depois
/// colhe as 9 respostas — que devem chegar exatamente na ordem dos pedidos, com os dados certos.
fn block_pipelined_client() -> ! {
    use nexo_proto::block::{
        CapacityRequest, ReadRequest, WriteRequest, decode_capacity_response, decode_read_response,
        decode_write_response,
    };
    let ch: nexo_sys::Handle = 0;
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];

    // capacidade (sincrona) para achar a area reservada
    let m = CapacityRequest {}.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(ch, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(790);
    }
    let base = match nexo_sys::channel_recv(ch, &mut msg, &mut hs) {
        Ok((n, _)) => match decode_capacity_response(&msg[..n]) {
            Ok(c) => c.sectors.saturating_sub(240),
            Err(_) => nexo_sys::exit(791),
        },
        Err(_) => nexo_sys::exit(792),
    };

    // fase de disparo: 4 escritas + capacidade + 4 leituras, tudo sem esperar
    for i in 0..4u64 {
        let mut w = WriteRequest {
            sector: base + i * 2,
            count: 2,
            data: [0; 3584],
            data_len: 1024,
        };
        for (k, b) in w.data[..1024].iter_mut().enumerate() {
            *b = (k as u8) ^ (0xa0 + i as u8);
        }
        let m = w.encode_msg(&mut msg).unwrap_or(0);
        if nexo_sys::channel_send(ch, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(793);
        }
    }
    let m = CapacityRequest {}.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(ch, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(794);
    }
    for i in 0..4u64 {
        let m = ReadRequest {
            sector: base + i * 2,
            count: 2,
        }
        .encode_msg(&mut msg)
        .unwrap_or(0);
        if nexo_sys::channel_send(ch, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(795);
        }
    }

    // colheita: exatamente na ordem dos pedidos
    for _ in 0..4 {
        match nexo_sys::channel_recv(ch, &mut msg, &mut hs) {
            Ok((n, _)) if decode_write_response(&msg[..n]).is_ok() => {}
            _ => nexo_sys::exit(796),
        }
    }
    match nexo_sys::channel_recv(ch, &mut msg, &mut hs) {
        Ok((n, _)) if decode_capacity_response(&msg[..n]).is_ok() => {}
        _ => nexo_sys::exit(797),
    }
    for i in 0..4u64 {
        let (n, _) = match nexo_sys::channel_recv(ch, &mut msg, &mut hs) {
            Ok(v) => v,
            Err(_) => nexo_sys::exit(798),
        };
        match decode_read_response(&msg[..n]) {
            Ok(r) if r.data().len() == 1024 => {
                for (k, &b) in r.data().iter().enumerate() {
                    if b != (k as u8) ^ (0xa0 + i as u8) {
                        nexo_rt::log!("utest: pipeline: byte {} da leitura {} divergente", k, i);
                        nexo_sys::exit(799);
                    }
                }
            }
            _ => nexo_sys::exit(800),
        }
    }
    nexo_sys::log("utest: nexo.block pipelined ok — 9 respostas em ordem, dados conferem");
    nexo_sys::exit(0)
}

/// Modo 64: quota de memoria compartilhavel por processo. Cria objetos de 256 paginas ate a
/// quota (4096 paginas = 16 objetos), confere que o 17o falha com NoMemory, fecha tudo e
/// confere que a quota VOLTA (criar de novo funciona).
fn shm_quota_client() -> ! {
    use nexo_sys::abi::{MEMORY_MAX_PAGES, SHM_PAGES_MAX_PER_PROCESS};
    let cabem = (SHM_PAGES_MAX_PER_PROCESS / MEMORY_MAX_PAGES) as usize; // 16
    let mut handles = [0u32; 32];
    for h in handles.iter_mut().take(cabem) {
        *h = nexo_sys::memory_create(MEMORY_MAX_PAGES).unwrap_or_else(|_| nexo_sys::exit(810));
    }
    match nexo_sys::memory_create(MEMORY_MAX_PAGES) {
        Err(Status::NoMemory) => {}
        _ => nexo_sys::exit(811), // acima da quota tinha de falhar
    }
    for &h in handles.iter().take(cabem) {
        if nexo_sys::handle_close(h) != Status::Ok {
            nexo_sys::exit(812);
        }
    }
    // quota devolvida: cria e fecha de novo
    let h = nexo_sys::memory_create(MEMORY_MAX_PAGES).unwrap_or_else(|_| nexo_sys::exit(813));
    let _ = nexo_sys::handle_close(h);
    nexo_sys::log("utest: quota de memoria ok — 16 MiB por processo, devolvida ao fechar");
    nexo_sys::exit(0)
}

/// Fala o `nexo.svc` tipado com um servico de eco: `serve{chan}` no controle, `echo{text}` no
/// canal do cliente; devolve `true` se a resposta foi `echo: <text>`.
fn svc_echo_round(ctl: nexo_sys::Handle, text: &[u8]) -> bool {
    use nexo_proto::svc::{EchoRequest, ServeRequest, decode_echo_response};
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];
    let (cli, cli_child) = match nexo_sys::channel_create() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let m = ServeRequest { chan: cli_child }
        .encode_msg(&mut out)
        .unwrap_or(0);
    if nexo_sys::channel_send(ctl, &out[..m], &[cli_child]) != Status::Ok {
        return false;
    }
    let mut rq = EchoRequest {
        text: [0; 64],
        text_len: text.len().min(64) as u32,
    };
    rq.text[..text.len().min(64)].copy_from_slice(&text[..text.len().min(64)]);
    let m = rq.encode_msg(&mut out).unwrap_or(0);
    if nexo_sys::channel_send(cli, &out[..m], &[]) != Status::Ok {
        return false;
    }
    let ok = match nexo_sys::channel_recv(cli, &mut buf, &mut hs) {
        Ok((n, _)) => match decode_echo_response(&buf[..n]) {
            Ok(r) => r.text().starts_with(b"echo: ") && r.text()[6..] == text[..text.len().min(64)],
            Err(_) => false,
        },
        Err(_) => false,
    };
    let _ = nexo_sys::handle_close(cli);
    ok
}

/// Modo 65: backup e restauracao entre DOIS discos fisicos. Cria arquivos no volume principal,
/// espelha para o volume de backup (outro disco), APAGA um original e corrompe outro, restaura
/// do backup e confere que o conteudo original voltou. Handles: 0 pipe do backup, 1 fs origem.
/// Modo 66: duplo buffer com seqlock de frame na saida composta. Confere o cabecalho do layout
/// `nexo_wm::frame` (magic/dimensoes), que cada recomposicao PUBLICA trocando o buffer da frente
/// (front alterna, frames avanca, seq fica par) e a garantia anti-rasgo: compor o frame seguinte
/// nao toca o frame publicado — o buffer da frente antiga continua integro apos um novo commit.
fn wm_flip_client() -> ! {
    use nexo_wm::frame;
    let ch: nexo_sys::Handle = 0;
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];

    // superficie cobrindo a saida inteira; frame 1: vermelho
    let (id, sbase) = wm_create(ch, 0, 0, 64, 48, 0);
    wm_fill(sbase, 64, 48, 255, 0, 0);
    wm_commit(ch, id);

    // saida composta
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1600));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1601);
    }
    let (n, nh) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1602));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1603));
    if nh != 1 {
        nexo_sys::exit(1604);
    }
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1605));

    // cabecalho: layout reconhecido, dimensoes coerentes, seq par (nenhuma troca em andamento)
    if wm_hdr(ob, frame::OFF_MAGIC) != frame::MAGIC {
        nexo_sys::exit(1606);
    }
    if wm_hdr(ob, frame::OFF_W) != outp.w as u32 || wm_hdr(ob, frame::OFF_H) != outp.h as u32 {
        nexo_sys::exit(1607);
    }
    if wm_hdr(ob, frame::OFF_SEQ) & 1 != 0 {
        nexo_sys::exit(1608);
    }
    let f1 = wm_hdr(ob, frame::OFF_FRONT);
    let n1 = wm_hdr(ob, frame::OFF_FRAMES);
    if n1 == 0 {
        nexo_sys::exit(1609); // o commit tem que ter publicado ao menos um frame
    }
    if wm_px(ob, outp.w, 5, 5) != (255, 0, 0) {
        nexo_sys::exit(1610);
    }

    // frame 2: verde — deve TROCAR a frente (nao copiar por cima do frame publicado)
    wm_fill(sbase, 64, 48, 0, 255, 0);
    wm_commit(ch, id);
    if wm_hdr(ob, frame::OFF_FRONT) != 1 - f1 {
        nexo_sys::exit(1611); // a frente nao alternou: sobrescreveu em vez de trocar
    }
    if wm_hdr(ob, frame::OFF_FRAMES) <= n1 {
        nexo_sys::exit(1612);
    }
    if wm_hdr(ob, frame::OFF_SEQ) & 1 != 0 {
        nexo_sys::exit(1613);
    }
    if wm_px(ob, outp.w, 5, 5) != (0, 255, 0) {
        nexo_sys::exit(1614);
    }

    // anti-rasgo: o frame publicado ANTERIOR (buffer f1) segue integro — a composicao verde
    // aconteceu no outro buffer. Amostra os quatro cantos e o centro do buffer antigo.
    let old = ob + frame::buf_offset(outp.w as u32, outp.h as u32, f1);
    for (x, y) in [(0, 0), (63, 0), (0, 47), (63, 47), (32, 24)] {
        // SAFETY: leitura dentro do buffer f1 da saida mapeada (w*h*4 bytes).
        let px = unsafe {
            let p = (old as *const u8).add(((y * outp.w + x) * 4) as usize);
            (p.read(), p.add(1).read(), p.add(2).read())
        };
        if px != (255, 0, 0) {
            nexo_sys::exit(1615);
        }
    }
    nexo_sys::log(
        "utest: wm flip ok — frame publicado intacto; compor troca de buffer sob seqlock",
    );
    nexo_sys::exit(0)
}

/// Modo 67: health check pos-boot do layout A/B. Pede "confirma" ao `upd` — que marca o slot
/// arrancado como saudavel no `\nexo\slots.bin` do disco de boot REAL — e confere pelo
/// "estado" (que RELE o setor do disco) que o slot ficou com sucesso=1 e tentativas repostas.
fn slots_confirm_driver() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut buf = [0u8; 64];
    let mut hs = [0u32; 1];
    if nexo_sys::channel_send(ch, b"confirma", &[]) != Status::Ok {
        nexo_sys::exit(1640);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1641));
    if n != 4 || &buf[..3] != b"ok " {
        nexo_rt::log!(
            "utest: slots: confirma respondeu '{}'",
            core::str::from_utf8(&buf[..n]).unwrap_or("?")
        );
        nexo_sys::exit(1642);
    }
    let slot = buf[3];
    if nexo_sys::channel_send(ch, b"estado", &[]) != Status::Ok {
        nexo_sys::exit(1643);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1644));
    // formato: "sel X A p_ t_ s_ B p_ t_ s_" — confere o slot confirmado (s=1, t=3)
    let (t_at, s_at) = if slot == b'A' { (12, 15) } else { (23, 26) };
    if n != 28 || buf[4] != slot || buf[s_at] != b'1' || buf[t_at] != b'3' {
        nexo_rt::log!(
            "utest: slots: estado respondeu '{}'",
            core::str::from_utf8(&buf[..n]).unwrap_or("?")
        );
        nexo_sys::exit(1645);
    }
    nexo_rt::log!(
        "utest: slots ok — health check confirmou o slot {} no disco (s1 t3)",
        slot as char
    );
    nexo_sys::exit(0)
}

/// Modo 68: atualizacao atomica A/B. Pede "aplica" ao `upd` — que copia kernel+initrd do slot
/// ATIVO para o INATIVO por dentro do FAT (reescrita a prova de cortes) e o marca pendente
/// (prioridade 3, 3 tentativas, sem sucesso) — e confere: "verifica" compara os dois slots
/// byte a byte, e "estado" (relido do disco) mostra o inativo pendente.
fn update_apply_driver() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut buf = [0u8; 64];
    let mut hs = [0u32; 1];
    if nexo_sys::channel_send(ch, b"aplica", &[]) != Status::Ok {
        nexo_sys::exit(1650);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1651));
    if n != 10 || &buf[..9] != b"aplicado " {
        nexo_rt::log!(
            "utest: update: aplica respondeu '{}'",
            core::str::from_utf8(&buf[..n]).unwrap_or("?")
        );
        nexo_sys::exit(1652);
    }
    let to = buf[9]; // o slot que recebeu a atualizacao (o inativo)
    if nexo_sys::channel_send(ch, b"verifica", &[]) != Status::Ok {
        nexo_sys::exit(1653);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1654));
    if &buf[..n] != b"igual" {
        nexo_rt::log!(
            "utest: update: verifica respondeu '{}'",
            core::str::from_utf8(&buf[..n]).unwrap_or("?")
        );
        nexo_sys::exit(1655);
    }
    if nexo_sys::channel_send(ch, b"estado", &[]) != Status::Ok {
        nexo_sys::exit(1656);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1657));
    // o destino tem que estar pendente: p=3, t=3, s=0 (e o ativo continua sendo o outro)
    let (p_at, t_at, s_at) = if to == b'A' {
        (9, 12, 15)
    } else {
        (20, 23, 26)
    };
    if n != 28 || buf[4] == to || buf[p_at] != b'3' || buf[t_at] != b'3' || buf[s_at] != b'0' {
        nexo_rt::log!(
            "utest: update: estado respondeu '{}'",
            core::str::from_utf8(&buf[..n]).unwrap_or("?")
        );
        nexo_sys::exit(1658);
    }
    nexo_rt::log!(
        "utest: update ok — slot {} atualizado por dentro do FAT, identico ao ativo, pendente p3 t3",
        to as char
    );
    nexo_sys::exit(0)
}

fn backup_driver() -> ! {
    use nexo_inst::AppFs;
    let pipe: nexo_sys::Handle = 0;
    let mut c = FsClient {
        ch: 1,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut fs = InstFs { c: &mut c };
    fs.mkdir("/bk-teste")
        .unwrap_or_else(|_| nexo_sys::exit(1580));
    fs.write_file("/bk-teste/a.txt", b"conteudo-a-original")
        .unwrap_or_else(|_| nexo_sys::exit(1581));
    fs.write_file("/bk-teste/b.txt", b"conteudo-b-original")
        .unwrap_or_else(|_| nexo_sys::exit(1582));

    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    // empresta o fs de origem ao backup; ele volta junto com o "ok"
    if nexo_sys::channel_send(pipe, b"espelha /bk-teste", &[1]) != Status::Ok {
        nexo_sys::exit(1583);
    }
    let src = match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"ok 2" => hs[0],
        _ => nexo_sys::exit(1584),
    };
    let mut c2 = FsClient {
        ch: src,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut fs = InstFs { c: &mut c2 };

    // desastre no volume principal: um arquivo apagado, outro adulterado
    fs.unlink("/bk-teste/a.txt")
        .unwrap_or_else(|_| nexo_sys::exit(1585));
    fs.write_file("/bk-teste/b.txt", b"CORROMPIDO")
        .unwrap_or_else(|_| nexo_sys::exit(1586));

    if nexo_sys::channel_send(pipe, b"restaura /bk-teste", &[src]) != Status::Ok {
        nexo_sys::exit(1587);
    }
    let src = match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"ok 2" => hs[0],
        _ => nexo_sys::exit(1588),
    };
    let mut c3 = FsClient {
        ch: src,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut fs = InstFs { c: &mut c3 };

    let mut back = [0u8; 64];
    let n = fs
        .read_file("/bk-teste/a.txt", &mut back)
        .unwrap_or_else(|_| nexo_sys::exit(1589));
    if &back[..n] != b"conteudo-a-original" {
        nexo_sys::exit(1590);
    }
    let n = fs
        .read_file("/bk-teste/b.txt", &mut back)
        .unwrap_or_else(|_| nexo_sys::exit(1591));
    if &back[..n] != b"conteudo-b-original" {
        nexo_sys::exit(1592);
    }
    nexo_sys::log("utest: backup ok — espelhado em outro disco, desastre revertido");
    nexo_sys::exit(0)
}

fn config_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    // fundo azul cobrindo a tela (inclusive a regiao do banner)
    let (bg, bg_base) = wm_create(s1, 0, 0, 64, 48, 0);
    wm_fill(bg_base, 64, 48, 0, 0, 255);
    wm_commit(s1, bg);
    let _ = bg;

    // sessao para o config
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1320));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1321));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1322);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1323),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1324);
    }
    let expect_pipe =
        |pipe: nexo_sys::Handle, want: &[u8], code: i64, buf: &mut [u8; 256], hs: &mut [u32; 1]| {
            match nexo_sys::channel_recv(pipe, buf, hs) {
                Ok((n, _)) if &buf[..n] == want => {}
                _ => nexo_sys::exit(code),
            }
        };
    expect_pipe(pipe, b"pronto", 1325, &mut buf, &mut hs);

    // saida + entrada sintetica
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1326));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1327);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1328));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1329));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1330));
    let stride = outp.w;
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1331));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1332));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1333);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1334),
    }
    let prefs =
        |s1: nexo_sys::Handle, out: &mut [u8; 256], buf: &mut [u8; 256], hs: &mut [u32; 1]| -> u8 {
            let m = nexo_proto::wm::PrefsRequest {}
                .encode_msg(out)
                .unwrap_or_else(|_| nexo_sys::exit(1335));
            if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
                nexo_sys::exit(1336);
            }
            match nexo_sys::channel_recv(s1, buf, hs) {
                Ok((n, _)) => {
                    nexo_proto::wm::decode_prefs_response(&buf[..n])
                        .unwrap_or_else(|_| nexo_sys::exit(1337))
                        .reduce_motion
                }
                _ => nexo_sys::exit(1338),
            }
        };
    let notify =
        |s1: nexo_sys::Handle, out: &mut [u8; 256], buf: &mut [u8; 256], hs: &mut [u32; 1]| {
            let mut rq = nexo_proto::wm::NotifyRequest {
                title: [0; 64],
                title_len: 1,
            };
            rq.title[0] = b'x';
            let m = rq.encode_msg(out).unwrap_or_else(|_| nexo_sys::exit(1339));
            if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
                nexo_sys::exit(1340);
            }
            match nexo_sys::channel_recv(s1, buf, hs) {
                Ok((n, _)) if nexo_proto::wm::decode_notify_response(&buf[..n]).is_ok() => {}
                _ => nexo_sys::exit(1341),
            }
        };

    // toggle RM: liga (prefs = 1) e desliga (prefs = 0)
    wm_click(inj, 16, 16);
    expect_pipe(pipe, b"rm1", 1342, &mut buf, &mut hs);
    if prefs(s1, &mut out, &mut buf, &mut hs) != 1 {
        nexo_sys::exit(1343);
    }
    wm_click(inj, 16, 16);
    expect_pipe(pipe, b"rm0", 1344, &mut buf, &mut hs);
    if prefs(s1, &mut out, &mut buf, &mut hs) != 0 {
        nexo_sys::exit(1345);
    }

    // toggle NP: com DND, um aviso NAO desenha o banner. As leituras usam espera com timeout:
    // a saida e memoria compartilhada e o wm pode estar NO MEIO de uma recomposicao (o composite
    // pinta o fundo antes do banner), entao uma leitura unica pode pegar um estado transiente.
    wm_click(inj, 32, 16);
    expect_pipe(pipe, b"np1", 1346, &mut buf, &mut hs);
    notify(s1, &mut out, &mut buf, &mut hs);
    wm_wait_px(ob, stride, 60, 4, (0, 0, 255), 1347); // converge para o fundo (sem banner)
    wm_click(inj, 32, 16);
    expect_pipe(pipe, b"np0", 1348, &mut buf, &mut hs);
    notify(s1, &mut out, &mut buf, &mut hs);
    wm_wait_px(ob, stride, 60, 4, (40, 80, 200), 1349); // o banner aparece (fundo do banner)
    nexo_sys::log("utest: config ok — toggles de RM e nao-perturbe com efeito real");
    nexo_sys::exit(0)
}

/// Modo 49: lanca um app GRAFICO instalado. Handles: 0 = canal nexo.fs, 1 = MemoryObject com o
/// ELF real da calculadora (`param` = tamanho), 2 = sessao bootstrap nexo.wm (o driver e o shell).
/// Instala a calc com perms=janelas e a lanca: o lancador abre uma sessao do compositor SO porque
/// a permissao foi declarada e a entrega pelo canal do app; a janela "calc" aparece (conferida por
/// surface_info). O mesmo binario sem a permissao nasce sem sessao e sai com o proprio erro.
/// Modo 57: consentimento no lancador. Instala dois apps (ambos declarando "janelas"), pede ao
/// lanc que os abra e decide CLICANDO na janela de consentimento: Permitir lanca o app (a
/// janela "calc" aparece) e Negar nao executa nada. Handles: 0 fs, 1 ELF, 2 wm (bootstrap),
/// 3 pipe do lanc.
fn consent_driver(elf_len: usize) -> ! {
    let mem: nexo_sys::Handle = 1;
    let wm_ch: nexo_sys::Handle = 2;
    let pipe: nexo_sys::Handle = 3;
    let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| nexo_sys::exit(1450));
    // SAFETY: base .. base+elf_len esta dentro do MemoryObject mapeado.
    let elf = unsafe { core::slice::from_raw_parts(base as *const u8, elf_len) };
    static mut PKG4: [u8; 49152] = [0; 49152];
    // SAFETY: utest tem uma unica thread; buffer estatico evita estourar a pilha.
    let pkg = unsafe { &mut *core::ptr::addr_of_mut!(PKG4) };
    let mut out = [0u8; 256];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    {
        let mut c = FsClient {
            ch: 0,
            req: [0; 4096],
            reply: [0; 4096],
        };
        let mut fs = InstFs { c: &mut c };
        let n = build_app_pkg(
            b"name=app-sim\nversion=1.0\nentry=app.elf\nperms=janelas\n",
            elf,
            pkg,
        );
        nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1451));
        let n = build_app_pkg(
            b"name=app-nao\nversion=1.0\nentry=app.elf\nperms=janelas\n",
            elf,
            pkg,
        );
        nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1452));
    }

    // sessao para o lanc + entrada sintetica no compositor
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1453));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1454));
    if nexo_sys::channel_send(wm_ch, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1455);
    }
    match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1456),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1457);
    }
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1458));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1459));
    if nexo_sys::channel_send(wm_ch, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1460);
    }
    match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1461),
    }
    let expect_pipe = |want: &[u8], code: i64, buf: &mut [u8; 384], hs: &mut [u32; 1]| {
        match nexo_sys::channel_recv(pipe, buf, hs) {
            Ok((n, _)) if &buf[..n] == want => {}
            _ => nexo_sys::exit(code),
        }
    };
    // procura uma janela em uso com o titulo dado; devolve se achou
    let find_title = |want: &[u8], out: &mut [u8; 256], buf: &mut [u8; 384], hs: &mut [u32; 1]| {
        for idx in 0..8u32 {
            let m = nexo_proto::wm::SurfaceInfoRequest { index: idx }
                .encode_msg(out)
                .unwrap_or_else(|_| nexo_sys::exit(1462));
            if nexo_sys::channel_send(wm_ch, &out[..m], &[]) != Status::Ok {
                nexo_sys::exit(1463);
            }
            let (n, _) =
                nexo_sys::channel_recv(wm_ch, buf, hs).unwrap_or_else(|_| nexo_sys::exit(1464));
            let info = nexo_proto::wm::decode_surface_info_response(&buf[..n])
                .unwrap_or_else(|_| nexo_sys::exit(1465));
            if info.used == 1 && info.title() == want {
                return true;
            }
        }
        false
    };

    // rodada 1: PERMITIR — o app e lancado e a janela "calc" aparece
    if nexo_sys::channel_send(pipe, b"abre app-sim", &[0]) != Status::Ok {
        nexo_sys::exit(1466);
    }
    expect_pipe(b"pedido", 1467, &mut buf, &mut hs);
    wm_click(inj, 8 + 2 + 10, 8 + 12 + 4); // centro do Permitir (janela do lanc em (8,8))
    expect_pipe(b"permitido", 1468, &mut buf, &mut hs);
    let start = nexo_sys::time_now();
    while !find_title(b"calc", &mut out, &mut buf, &mut hs) {
        if nexo_sys::time_now() - start > 10_000_000_000 {
            nexo_sys::exit(1469);
        }
        nexo_sys::sleep_ns(10_000_000);
    }
    if nexo_sys::channel_send(pipe, b"fecha", &[]) != Status::Ok {
        nexo_sys::exit(1470);
    }
    expect_pipe(b"fim", 1471, &mut buf, &mut hs);

    // rodada 2: NEGAR — nada e executado ("negado" chega DEPOIS da decisao de nao lancar)
    if nexo_sys::channel_send(pipe, b"abre app-nao", &[]) != Status::Ok {
        nexo_sys::exit(1472);
    }
    expect_pipe(b"pedido", 1473, &mut buf, &mut hs);
    wm_click(inj, 8 + 26 + 10, 8 + 12 + 4); // centro do Negar
    expect_pipe(b"negado", 1474, &mut buf, &mut hs);
    if find_title(b"calc", &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(1475); // janela de um app negado: o consentimento falhou
    }

    nexo_sys::log("utest: consentimento ok — permitir lanca, negar nao executa");
    nexo_sys::exit(0)
}

fn launch_gui_client(elf_len: usize) -> ! {
    let mem: nexo_sys::Handle = 1;
    let wm_ch: nexo_sys::Handle = 2;
    let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| nexo_sys::exit(1280));
    // SAFETY: base .. base+elf_len esta dentro do MemoryObject mapeado.
    let elf = unsafe { core::slice::from_raw_parts(base as *const u8, elf_len) };
    let mut c = FsClient {
        ch: 0,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut fs = InstFs { c: &mut c };
    static mut PKG2: [u8; 49152] = [0; 49152];
    static mut ELF2: [u8; 40960] = [0; 40960];
    // SAFETY: utest tem uma unica thread; buffers estaticos evitam estourar a pilha.
    let (pkg, elf_buf) = unsafe {
        (
            &mut *core::ptr::addr_of_mut!(PKG2),
            &mut *core::ptr::addr_of_mut!(ELF2),
        )
    };
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    // COM a permissao "janelas": instala e lanca com uma sessao do compositor
    let n = build_app_pkg(
        b"name=calc-app\nversion=1.0\nentry=app.elf\nperms=janelas\n",
        elf,
        pkg,
    );
    nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1281));
    let v = nexo_inst::current_version(&mut fs, "calc-app").unwrap_or_else(|| nexo_sys::exit(1282));
    let mut pb = [0u8; nexo_inst::MAX_PATH];
    let mpath = nexo_inst::versioned_path("calc-app", v, "manifest.txt", &mut pb)
        .unwrap_or_else(|_| nexo_sys::exit(1283));
    let mut mbuf = [0u8; 256];
    let mn = nexo_inst::AppFs::read_file(&mut fs, mpath, &mut mbuf)
        .unwrap_or_else(|_| nexo_sys::exit(1284));
    let manifest = nexo_pkg::Manifest::parse(&mbuf[..mn]).unwrap_or_else(|_| nexo_sys::exit(1285));
    if !manifest.declares("janelas") {
        nexo_sys::exit(1286);
    }
    let mut pb = [0u8; nexo_inst::MAX_PATH];
    let epath = nexo_inst::versioned_path("calc-app", v, manifest.entry, &mut pb)
        .unwrap_or_else(|_| nexo_sys::exit(1287));
    let en = nexo_inst::AppFs::read_file(&mut fs, epath, elf_buf)
        .unwrap_or_else(|_| nexo_sys::exit(1288));
    // capacidade "janelas": abre uma sessao nova do compositor para o app
    let (sess_app, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1289));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1290));
    if nexo_sys::channel_send(wm_ch, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1291);
    }
    match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1292),
    }
    let (pipe, pipe_child) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1293));
    let child = nexo_sys::process_spawn_mem(&elf_buf[..en], 0, &[pipe_child])
        .unwrap_or_else(|_| nexo_sys::exit(1294));
    if nexo_sys::channel_send(pipe, b"sess", &[sess_app]) != Status::Ok {
        nexo_sys::exit(1295);
    }
    // a janela "calc" aparece (o driver e a sessao shell: surface_info)
    let start = nexo_sys::time_now();
    'outer: loop {
        for idx in 0..8u32 {
            let m = nexo_proto::wm::SurfaceInfoRequest { index: idx }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| nexo_sys::exit(1296));
            if nexo_sys::channel_send(wm_ch, &out[..m], &[]) != Status::Ok {
                nexo_sys::exit(1297);
            }
            let (n, _) = nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs)
                .unwrap_or_else(|_| nexo_sys::exit(1298));
            let info = nexo_proto::wm::decode_surface_info_response(&buf[..n])
                .unwrap_or_else(|_| nexo_sys::exit(1299));
            if info.used == 1 && info.title() == b"calc" {
                break 'outer;
            }
        }
        if nexo_sys::time_now() - start > 10_000_000_000 {
            nexo_sys::exit(1300);
        }
        nexo_sys::sleep_ns(10_000_000);
    }
    // encerra o app pelo cordao de vida
    if nexo_sys::handle_close(pipe) != Status::Ok {
        nexo_sys::exit(1301);
    }
    if nexo_sys::process_wait(child) != Ok(0) {
        nexo_sys::exit(1302);
    }

    // SEM a permissao: nenhuma sessao e concedida; o app sai com o proprio erro (21)
    let n = build_app_pkg(b"name=calc-sem\nversion=1.0\nentry=app.elf\n", elf, pkg);
    nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1303));
    let v = nexo_inst::current_version(&mut fs, "calc-sem").unwrap_or_else(|| nexo_sys::exit(1304));
    let mut pb = [0u8; nexo_inst::MAX_PATH];
    let mpath = nexo_inst::versioned_path("calc-sem", v, "manifest.txt", &mut pb)
        .unwrap_or_else(|_| nexo_sys::exit(1305));
    let mn = nexo_inst::AppFs::read_file(&mut fs, mpath, &mut mbuf)
        .unwrap_or_else(|_| nexo_sys::exit(1306));
    let manifest = nexo_pkg::Manifest::parse(&mbuf[..mn]).unwrap_or_else(|_| nexo_sys::exit(1307));
    if manifest.declares("janelas") {
        nexo_sys::exit(1308);
    }
    let (pipe, pipe_child) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1309));
    let child = nexo_sys::process_spawn_mem(&elf_buf[..en], 0, &[pipe_child])
        .unwrap_or_else(|_| nexo_sys::exit(1310));
    // sem a permissao, o lancador NAO envia sessao: fecha o canal e o app encerra sem janela
    if nexo_sys::handle_close(pipe) != Status::Ok {
        nexo_sys::exit(1311);
    }
    if nexo_sys::process_wait(child) != Ok(21) {
        nexo_sys::exit(1312);
    }
    nexo_sys::log("utest: launch_gui ok — app grafico instalado ganha janela SO com a permissao");
    nexo_sys::exit(0)
}

/// Empacota `elf` num NEXOPKG1 com o manifesto dado; devolve o tamanho em `out`.
fn build_app_pkg(manifest: &[u8], elf: &[u8], out: &mut [u8]) -> usize {
    let name = b"app.elf";
    let mut o = 0;
    out[o..o + 8].copy_from_slice(nexo_pkg::MAGIC);
    o += 8;
    out[o..o + 4].copy_from_slice(&nexo_pkg::VERSION.to_le_bytes());
    o += 4;
    out[o..o + 4].copy_from_slice(&(manifest.len() as u32).to_le_bytes());
    o += 4;
    out[o..o + 4].copy_from_slice(&1u32.to_le_bytes());
    o += 4;
    let crc_at = o;
    o += 4;
    let body_at = o;
    out[o..o + manifest.len()].copy_from_slice(manifest);
    o += manifest.len();
    out[o..o + 2].copy_from_slice(&(name.len() as u16).to_le_bytes());
    o += 2;
    out[o..o + name.len()].copy_from_slice(name);
    o += name.len();
    out[o..o + 4].copy_from_slice(&(elf.len() as u32).to_le_bytes());
    o += 4;
    out[o..o + elf.len()].copy_from_slice(elf);
    o += elf.len();
    let crc = nexo_pkg::crc32(&out[body_at..o]);
    out[crc_at..crc_at + 4].copy_from_slice(&crc.to_le_bytes());
    o
}

/// Lanca a versao corrente instalada de `app`: le o manifesto, concede o canal de controle SO se
/// a permissao "ipc" foi declarada, le o ELF da instalacao e executa da memoria. Devolve
/// (handle do filho, Some(canal de controle) se concedido).
fn launch_installed(
    fs: &mut InstFs,
    app: &str,
    elf_buf: &mut [u8],
) -> (nexo_sys::Handle, Option<nexo_sys::Handle>) {
    let v = nexo_inst::current_version(fs, app).unwrap_or_else(|| nexo_sys::exit(1250));
    let mut pb = [0u8; nexo_inst::MAX_PATH];
    let mpath = nexo_inst::versioned_path(app, v, "manifest.txt", &mut pb)
        .unwrap_or_else(|_| nexo_sys::exit(1251));
    let mut mbuf = [0u8; 256];
    let mn =
        nexo_inst::AppFs::read_file(fs, mpath, &mut mbuf).unwrap_or_else(|_| nexo_sys::exit(1252));
    let manifest = nexo_pkg::Manifest::parse(&mbuf[..mn]).unwrap_or_else(|_| nexo_sys::exit(1253));
    // portal de capacidades: so o que o manifesto DECLARA e concedido
    let grant_ipc = manifest.declares("ipc");
    let mut pb = [0u8; nexo_inst::MAX_PATH];
    let epath = nexo_inst::versioned_path(app, v, manifest.entry, &mut pb)
        .unwrap_or_else(|_| nexo_sys::exit(1254));
    let en =
        nexo_inst::AppFs::read_file(fs, epath, elf_buf).unwrap_or_else(|_| nexo_sys::exit(1255));
    if grant_ipc {
        let (ctl, ctl_child) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1256));
        let child = nexo_sys::process_spawn_mem(&elf_buf[..en], 3, &[ctl_child])
            .unwrap_or_else(|_| nexo_sys::exit(1257));
        (child, Some(ctl))
    } else {
        let child = nexo_sys::process_spawn_mem(&elf_buf[..en], 3, &[])
            .unwrap_or_else(|_| nexo_sys::exit(1258));
        (child, None)
    }
}

/// Modo 48: o laco COMPLETO da plataforma — empacota um app real (o `echo`, entregue pelo kernel
/// num MemoryObject), instala transacionalmente no NexoFS, e um LANCADOR le o manifesto instalado
/// e concede capacidades so pelas permissoes declaradas: com "ipc" o app recebe o canal e ecoa;
/// sem, nasce sem canal (a capacidade nao e concedida) e sai com o erro proprio.
fn launcher_client(elf_len: usize) -> ! {
    let mem: nexo_sys::Handle = 1;
    let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| nexo_sys::exit(1260));
    // SAFETY: base .. base+elf_len esta dentro do MemoryObject mapeado.
    let elf = unsafe { core::slice::from_raw_parts(base as *const u8, elf_len) };

    let mut c = FsClient {
        ch: 0,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut fs = InstFs { c: &mut c };
    static mut PKG: [u8; 40960] = [0; 40960];
    static mut ELF_BUF: [u8; 40960] = [0; 40960];
    // SAFETY: utest tem uma unica thread; os buffers estaticos evitam estourar a pilha.
    let (pkg, elf_buf) = unsafe {
        (
            &mut *core::ptr::addr_of_mut!(PKG),
            &mut *core::ptr::addr_of_mut!(ELF_BUF),
        )
    };

    // app COM a permissao "ipc": recebe o canal e ecoa
    let n = build_app_pkg(
        b"name=eco-app\nversion=1.0\nentry=app.elf\nperms=ipc\n",
        elf,
        pkg,
    );
    nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1261));
    let (child, ctl) = launch_installed(&mut fs, "eco-app", elf_buf);
    let ctl = ctl.unwrap_or_else(|| nexo_sys::exit(1262));
    if !svc_echo_round(ctl, b"instalado") {
        nexo_sys::exit(1264);
    }
    let _ = nexo_sys::handle_close(ctl);
    if nexo_sys::process_wait(child) != Ok(0) {
        nexo_sys::exit(1268);
    }

    // app SEM a permissao: o lancador NAO concede o canal; o filho sai com o erro proprio (20)
    let n = build_app_pkg(b"name=eco-sem\nversion=1.0\nentry=app.elf\n", elf, pkg);
    nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1269));
    let (child, ctl) = launch_installed(&mut fs, "eco-sem", elf_buf);
    if ctl.is_some() {
        nexo_sys::exit(1270);
    }
    if nexo_sys::process_wait(child) != Ok(20) {
        nexo_sys::exit(1271);
    }
    nexo_sys::log(
        "utest: launcher ok — instalado executa; capacidade so com a permissao declarada",
    );
    nexo_sys::exit(0)
}

/// Modo 47: executa um programa a partir da MEMORIA/// Modo 47: executa um programa a partir da MEMORIA (process_spawn_mem) — o elo "instalar ->
/// executar". Handle 0 = canal com o kernel; handle 1 = MemoryObject com o ELF do `echo`
/// (`param` = tamanho real). Mapeia, spawna com um canal de controle, conversa com o filho
/// (pedido/eco) e o encerra limpo fechando o controle.
fn spawn_mem_client(elf_len: usize) -> ! {
    let mem: nexo_sys::Handle = 1;
    let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| nexo_sys::exit(1230));
    // SAFETY: base .. base+elf_len esta dentro do MemoryObject mapeado (USER|RW).
    let elf = unsafe { core::slice::from_raw_parts(base as *const u8, elf_len) };

    // canal de controle do echo (protocolo do svcmgr: "serve" + canal do cliente)
    let (ctl, ctl_child) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1231));
    let child =
        nexo_sys::process_spawn_mem(elf, 3, &[ctl_child]).unwrap_or_else(|_| nexo_sys::exit(1232));

    if !svc_echo_round(ctl, b"ola do spawn_mem") {
        nexo_sys::exit(1234);
    }
    // fecha o controle: o echo sai limpo com 0
    if nexo_sys::handle_close(ctl) != Status::Ok {
        nexo_sys::exit(1238);
    }
    let code = nexo_sys::process_wait(child).unwrap_or_else(|_| nexo_sys::exit(1239));
    if code != 0 {
        nexo_sys::exit(1240);
    }
    // ELF invalido e recusado sem derrubar nada
    if nexo_sys::process_spawn_mem(b"lixo-que-nao-e-elf", 0, &[]).is_ok() {
        nexo_sys::exit(1241);
    }
    nexo_sys::log("utest: spawn_mem ok — ELF da memoria executou, ecoou e saiu limpo");
    nexo_sys::exit(0)
}

/// Adaptador do instalador transacional sobre o protocolo `nexo.fs` (via [`FsClient`]).
struct InstFs<'a> {
    c: &'a mut FsClient,
}

impl nexo_inst::AppFs for InstFs<'_> {
    fn mkdir(&mut self, path: &str) -> Result<(), nexo_inst::FsErr> {
        let (st, _, _) = self.c.call(0, 0, 0, 0, path.as_bytes());
        if st == 0 {
            return Ok(()); // ja existe: idempotente
        }
        let (st, _, _) = self.c.call(2, 0, 0, 0, path.as_bytes());
        if st == 0 {
            Ok(())
        } else {
            Err(nexo_inst::FsErr)
        }
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), nexo_inst::FsErr> {
        let (st, v, _) = self.c.call(0, 0, 0, 0, path.as_bytes());
        let ino = if st == 0 {
            v as u32
        } else {
            let (st, v, _) = self.c.call(1, 0, 0, 0, path.as_bytes());
            if st != 0 {
                return Err(nexo_inst::FsErr);
            }
            v as u32
        };
        let (st, _, _) = self.c.call(9, ino, 0, 0, &[]);
        if st != 0 {
            return Err(nexo_inst::FsErr);
        }
        let mut off = 0usize;
        while off < data.len() {
            let n = (data.len() - off).min(3900);
            let (st, w, _) = self.c.call(5, ino, off as u64, 0, &data[off..off + n]);
            if st != 0 || w as usize != n {
                return Err(nexo_inst::FsErr);
            }
            off += n;
        }
        Ok(())
    }

    fn unlink(&mut self, path: &str) -> Result<(), nexo_inst::FsErr> {
        let (st, _, _) = self.c.call(3, 0, 0, 0, path.as_bytes());
        if st == 0 {
            Ok(())
        } else {
            Err(nexo_inst::FsErr)
        }
    }

    fn read_file(&mut self, path: &str, buf: &mut [u8]) -> Result<usize, nexo_inst::FsErr> {
        let (st, v, _) = self.c.call(0, 0, 0, 0, path.as_bytes());
        if st != 0 {
            return Err(nexo_inst::FsErr);
        }
        let ino = v as u32;
        let size = u64::from_le_bytes(self.c.reply[13..21].try_into().unwrap()) as usize;
        if size > buf.len() {
            return Err(nexo_inst::FsErr);
        }
        let mut off = 0usize;
        while off < size {
            let want = (size - off).min(3900) as u32;
            let (st, dl, _) = self.c.call(4, ino, off as u64, want, &[]);
            if st != 0 || dl == 0 {
                return Err(nexo_inst::FsErr);
            }
            let dl = dl as usize;
            buf[off..off + dl].copy_from_slice(self.c.data(dl));
            off += dl;
        }
        Ok(size)
    }
}

/// Monta um pacote NEXOPKG1 minimo em `out` (manifesto + 1 arquivo "app.elf" com `payload`).
fn build_pkg(version: &[u8], payload: &[u8], out: &mut [u8; 512]) -> usize {
    let mut manifest = [0u8; 96];
    let mut ml = 0;
    for part in [
        b"name=inst-demo\nversion=".as_slice(),
        version,
        b"\nentry=app.elf\nperms=janelas\n",
    ] {
        manifest[ml..ml + part.len()].copy_from_slice(part);
        ml += part.len();
    }
    let name = b"app.elf";
    let plen = ml + 2 + name.len() + 4 + payload.len();
    // payload do pacote
    let mut body = [0u8; 400];
    let mut o = 0;
    body[o..o + ml].copy_from_slice(&manifest[..ml]);
    o += ml;
    body[o..o + 2].copy_from_slice(&(name.len() as u16).to_le_bytes());
    o += 2;
    body[o..o + name.len()].copy_from_slice(name);
    o += name.len();
    body[o..o + 4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    o += 4;
    body[o..o + payload.len()].copy_from_slice(payload);
    o += payload.len();
    assert!(o == plen);
    // cabecalho
    out[..8].copy_from_slice(nexo_pkg::MAGIC);
    out[8..12].copy_from_slice(&nexo_pkg::VERSION.to_le_bytes());
    out[12..16].copy_from_slice(&(ml as u32).to_le_bytes());
    out[16..20].copy_from_slice(&1u32.to_le_bytes());
    out[20..24].copy_from_slice(&nexo_pkg::crc32(&body[..o]).to_le_bytes());
    out[24..24 + o].copy_from_slice(&body[..o]);
    24 + o
}

/// Modo 46: instalacao transacional sobre o NexoFS real (handle 0 = canal nexo.fs). Instala a v1
/// de um pacote, le o arquivo de volta pelo caminho versionado, atualiza para a v2 (o ponteiro
/// .cur vira por ultimo) e confere que a v1 segue intacta.
fn install_client() -> ! {
    let mut c = FsClient {
        ch: 0,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut fs = InstFs { c: &mut c };
    let mut pkg = [0u8; 512];

    // O disco de dados PERSISTE entre boots: as versões são relativas à corrente.
    let v0 = nexo_inst::current_version(&mut fs, "inst-demo").unwrap_or(0);
    let n = build_pkg(b"0.1", b"PAYLOAD-V1", &mut pkg);
    let v = nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1200));
    if v != v0 + 1 || nexo_inst::current_version(&mut fs, "inst-demo") != Some(v0 + 1) {
        nexo_sys::exit(1201);
    }
    let mut pb = [0u8; nexo_inst::MAX_PATH];
    let path = nexo_inst::versioned_path("inst-demo", v0 + 1, "app.elf", &mut pb)
        .unwrap_or_else(|_| nexo_sys::exit(1202));
    let mut back = [0u8; 64];
    let bn = nexo_inst::AppFs::read_file(&mut fs, path, &mut back)
        .unwrap_or_else(|_| nexo_sys::exit(1203));
    if &back[..bn] != b"PAYLOAD-V1" {
        nexo_sys::exit(1204);
    }

    // atualizacao: o ponteiro vira, a versao anterior continua intacta
    let n = build_pkg(b"0.2", b"PAYLOAD-V2!", &mut pkg);
    let v = nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1205));
    if v != v0 + 2 || nexo_inst::current_version(&mut fs, "inst-demo") != Some(v0 + 2) {
        nexo_sys::exit(1206);
    }
    let mut pb = [0u8; nexo_inst::MAX_PATH];
    let path = nexo_inst::versioned_path("inst-demo", v0 + 2, "app.elf", &mut pb)
        .unwrap_or_else(|_| nexo_sys::exit(1207));
    let bn = nexo_inst::AppFs::read_file(&mut fs, path, &mut back)
        .unwrap_or_else(|_| nexo_sys::exit(1208));
    if &back[..bn] != b"PAYLOAD-V2!" {
        nexo_sys::exit(1209);
    }
    let mut pb = [0u8; nexo_inst::MAX_PATH];
    let path = nexo_inst::versioned_path("inst-demo", v0 + 1, "app.elf", &mut pb)
        .unwrap_or_else(|_| nexo_sys::exit(1210));
    let bn = nexo_inst::AppFs::read_file(&mut fs, path, &mut back)
        .unwrap_or_else(|_| nexo_sys::exit(1211));
    if &back[..bn] != b"PAYLOAD-V1" {
        nexo_sys::exit(1212);
    }
    // pacote corrompido nao muda nada
    let n = build_pkg(b"0.3", b"MAL", &mut pkg);
    pkg[20] ^= 0xff; // quebra o crc
    if nexo_inst::install(&mut fs, &pkg[..n]).is_ok() {
        nexo_sys::exit(1213);
    }
    if nexo_inst::current_version(&mut fs, "inst-demo") != Some(v0 + 2) {
        nexo_sys::exit(1214);
    }
    // coleta de versoes antigas: depois das duas instalacoes acima a corrente e v0+2 e a
    // janela mantida e {v0+1, v0+2}. Toda versao coletavel (<= v0, com files.txt) deve ter
    // sido removida; versoes gravadas antes do files.txt existir sao toleradas (documentado).
    for v in 1..=v0 {
        let mut pb = [0u8; nexo_inst::MAX_PATH];
        let path = nexo_inst::versioned_path("inst-demo", v, "files.txt", &mut pb)
            .unwrap_or_else(|_| nexo_sys::exit(1219));
        let mut tmp = [0u8; 1024];
        if nexo_inst::AppFs::read_file(&mut fs, path, &mut tmp).is_ok() {
            nexo_sys::exit(1220); // versao coletavel sobrou: gc nao rodou
        }
    }

    // revogacao (idempotente entre boots: na 2a execucao o app ja esta revogado)
    if !nexo_inst::is_revoked(&mut fs, "rev-demo") {
        let n = build_app_pkg(
            b"name=rev-demo\nversion=1.0\nentry=app.elf\n",
            b"X",
            &mut pkg,
        );
        nexo_inst::install(&mut fs, &pkg[..n]).unwrap_or_else(|_| nexo_sys::exit(1215));
        nexo_inst::revoke(&mut fs, "rev-demo").unwrap_or_else(|_| nexo_sys::exit(1216));
    }
    if !nexo_inst::is_revoked(&mut fs, "rev-demo") {
        nexo_sys::exit(1217);
    }
    let n = build_app_pkg(
        b"name=rev-demo\nversion=2.0\nentry=app.elf\n",
        b"Y",
        &mut pkg,
    );
    match nexo_inst::install(&mut fs, &pkg[..n]) {
        Err(nexo_inst::InstError::Revoked) => {}
        _ => nexo_sys::exit(1218),
    }
    // repositorio local: o pacote em /repo/<nome>.npk instala pelo caminho oficial
    {
        use nexo_inst::AppFs;
        let n = build_app_pkg(
            b"name=repo-demo\nversion=1.0\nentry=app.elf\n",
            b"R",
            &mut pkg,
        );
        fs.mkdir("/repo").unwrap_or_else(|_| nexo_sys::exit(1221));
        fs.write_file("/repo/repo-demo.npk", &pkg[..n])
            .unwrap_or_else(|_| nexo_sys::exit(1222));
        let v0r = nexo_inst::current_version(&mut fs, "repo-demo").unwrap_or(0);
        let mut rbuf = [0u8; 512];
        let v = nexo_inst::install_from_repo(&mut fs, "repo-demo", &mut rbuf)
            .unwrap_or_else(|_| nexo_sys::exit(1223));
        if v != v0r + 1 {
            nexo_sys::exit(1224);
        }
        let mut rbuf2 = [0u8; 512];
        if nexo_inst::install_from_repo(&mut fs, "sumido", &mut rbuf2).is_ok() {
            nexo_sys::exit(1225);
        }
        // indice do repositorio: escrito no NexoFS e lido de volta pelo parser oficial
        fs.write_file(
            "/repo/indice.txt",
            b"# repositorio de teste\nrepo-demo 1.0\n",
        )
        .unwrap_or_else(|_| nexo_sys::exit(1226));
        let mut ibuf = [0u8; 256];
        let n = fs
            .read_file("/repo/indice.txt", &mut ibuf)
            .unwrap_or_else(|_| nexo_sys::exit(1227));
        let idx = nexo_pkg::RepoIndex::parse(&ibuf[..n]).unwrap_or_else(|_| nexo_sys::exit(1228));
        if idx.find("repo-demo") != Some("1.0") || idx.find("sumido").is_some() {
            nexo_sys::exit(1229);
        }
    }
    nexo_sys::log(
        "utest: install ok — transacional; corrompido/revogado nao instalam; repositorio local instala",
    );
    nexo_sys::exit(0)
}

/// Modo 9: cria, le, altera, lista e remove arquivos; contador de boots persistente.
fn fs_client() -> ! {
    fs_exercise(0, true);
    nexo_sys::log("utest: fs ok");
    nexo_sys::exit(0)
}

/// Exercita o `fs` no canal `ch` (modos 9 e 11); so o modo 9 avanca o contador de boots.
fn fs_exercise(ch: nexo_sys::Handle, count_boot: bool) {
    let mut c = FsClient {
        ch,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let (_, n) = c.ok(8, 0, 0, 0, &[], 140);
    let d = c.data(n);
    let total = u64::from_le_bytes(d[0..8].try_into().unwrap());
    let free = u64::from_le_bytes(d[8..16].try_into().unwrap());
    let repairs = u64::from_le_bytes(d[16..24].try_into().unwrap());
    nexo_rt::log!(
        "utest: fs: {} blocos, {} livres, {} reparo(s)",
        total,
        free,
        repairs
    );
    // limpeza de um boot anterior interrompido
    let _ = c.call(3, 0, 0, 0, b"docs/a.txt");
    let _ = c.call(3, 0, 0, 0, b"docs");
    c.ok(2, 0, 0, 0, b"docs", 141);
    let (ino, _) = c.ok(1, 0, 0, 0, b"docs/a.txt", 142);
    let ino = ino as u32;
    let (st, _, _) = c.call(1, 0, 0, 0, b"docs/a.txt");
    if st != 4 {
        nexo_rt::log!("utest: fs: criar duplicado devolveu {}", st);
        nexo_sys::exit(143);
    }
    c.ok(5, ino, 0, 0, b"hello world", 144);
    c.ok(5, ino, 6, 0, b"nexo!", 145);
    let (r, n) = c.ok(4, ino, 0, 64, &[], 146);
    if r != 11 || c.data(n) != b"hello nexo!" {
        nexo_sys::exit(147);
    }
    let mut big = [0u8; 3000];
    for (i, b) in big.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    c.ok(5, ino, 11, 0, &big, 148);
    let (r, n) = c.ok(4, ino, 0, 4000, &[], 149);
    if r != 3011 || n != 3011 || &c.data(n)[..11] != b"hello nexo!" || c.data(n)[11..] != big[..] {
        nexo_rt::log!("utest: fs: releitura de 3011 bytes divergiu ({} lidos)", r);
        nexo_sys::exit(150);
    }
    let (st, size_v, n) = c.call(0, 0, 0, 0, b"docs/a.txt");
    if st != 0
        || size_v != ino as u64
        || u64::from_le_bytes(c.data(n)[1..9].try_into().unwrap()) != 3011
    {
        nexo_sys::exit(151);
    }
    c.ok(9, ino, 5, 0, &[], 152);
    let (r, _) = c.ok(4, ino, 0, 64, &[], 153);
    if r != 5 {
        nexo_sys::exit(154);
    }
    let (count, n) = c.ok(6, 0, 0, 0, b"docs", 155);
    let d = c.data(n);
    if count != 1 || d[4] != 1 || &d[6..6 + d[5] as usize] != b"a.txt" {
        nexo_sys::exit(156);
    }
    let (st, _, _) = c.call(3, 0, 0, 0, b"docs");
    if st != 7 {
        nexo_rt::log!("utest: fs: remover diretorio cheio devolveu {}", st);
        nexo_sys::exit(157);
    }
    c.ok(3, 0, 0, 0, b"docs/a.txt", 158);
    c.ok(3, 0, 0, 0, b"docs", 159);
    let (st, _, _) = c.call(0, 0, 0, 0, b"docs/a.txt");
    if st != 3 {
        nexo_sys::exit(160);
    }
    if !count_boot {
        c.ok(7, 0, 0, 0, &[], 168);
        return;
    }
    // contador de boots
    let (st, bino, n) = c.call(0, 0, 0, 0, b"boot.count");
    let boots = if st == 3 {
        let (i, _) = c.ok(1, 0, 0, 0, b"boot.count", 161);
        c.ok(5, i as u32, 0, 0, b"1", 162);
        1
    } else if st == 0 {
        let _ = n;
        let (r, n) = c.ok(4, bino as u32, 0, 32, &[], 163);
        let mut v = 0u64;
        for &b in &c.data(n)[..r as usize] {
            v = v * 10 + (b - b'0') as u64;
        }
        let v = v + 1;
        let mut txt = nexo_rt::Buf::<32>::new();
        let _ = core::fmt::Write::write_fmt(&mut txt, format_args!("{}", v));
        c.ok(5, bino as u32, 0, 0, txt.as_bytes(), 164);
        v
    } else {
        nexo_sys::exit(165)
    };
    c.ok(7, 0, 0, 0, &[], 166);
    let (_, n) = c.ok(8, 0, 0, 0, &[], 167);
    let free2 = u64::from_le_bytes(c.data(n)[8..16].try_into().unwrap());
    nexo_rt::log!("utest: fs: boot numero {} ({} blocos livres)", boots, free2);
}

/// Modo 10: escreve sem parar (cria, sobrescreve, estende e remove arquivos em `churn/`),
/// registrando os ciclos; termina so quando o host corta a energia.
fn fs_churn() -> ! {
    let mut c = FsClient {
        ch: 0,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let _ = c.call(2, 0, 0, 0, b"churn");
    let mut pattern = [0u8; 3000];
    let mut cycle = 0u64;
    loop {
        cycle += 1;
        for (i, b) in pattern.iter_mut().enumerate() {
            *b = ((i as u64 + cycle) % 253) as u8;
        }
        let mut name = nexo_rt::Buf::<32>::new();
        let _ = core::fmt::Write::write_fmt(&mut name, format_args!("churn/f{}", cycle % 8));
        let _ = c.call(3, 0, 0, 0, name.as_bytes());
        let (st, ino, _) = c.call(1, 0, 0, 0, name.as_bytes());
        if st != 0 {
            nexo_rt::log!("utest: churn: create falhou ({})", st);
            nexo_sys::exit(170);
        }
        let ino = ino as u32;
        c.ok(5, ino, 0, 0, &pattern, 171);
        c.ok(5, ino, 1500, 0, &pattern[..1000], 172);
        c.ok(5, ino, 3000, 0, &pattern, 173);
        c.ok(9, ino, 100, 0, &[], 174);
        c.ok(5, ino, 100, 0, &pattern[..2000], 175);
        // contador de ciclos persistente: cada ciclo reescreve `churn/ciclos`
        let (st, cino, _) = c.call(0, 0, 0, 0, b"churn/ciclos");
        let cino = if st == 0 {
            cino as u32
        } else {
            c.ok(1, 0, 0, 0, b"churn/ciclos", 176).0 as u32
        };
        let mut txt = nexo_rt::Buf::<32>::new();
        let _ = core::fmt::Write::write_fmt(&mut txt, format_args!("{}", cycle));
        c.ok(5, cino, 0, 0, txt.as_bytes(), 177);
        if cycle.is_multiple_of(5) {
            nexo_rt::log!("utest: churn: {} ciclos", cycle);
        }
    }
}

/// Modo 11: cliente do `devmgr` (handle 0): recebe os canais de servico (`fs`, `rng`) e usa-os.
fn devmgr_client() -> ! {
    let mut buf = [0u8; 64];
    let mut hs = [0u32; 1];
    let mut fs: Option<nexo_sys::Handle> = None;
    let mut rng: Option<nexo_sys::Handle> = None;
    let mut esp: Option<nexo_sys::Handle> = None;
    loop {
        let (n, nh) = match nexo_sys::channel_recv(0, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(_) => nexo_sys::exit(180),
        };
        match &buf[..n] {
            b"fs" if nh == 1 => fs = Some(hs[0]),
            b"rng" if nh == 1 => rng = Some(hs[0]),
            b"esp" if nh == 1 => esp = Some(hs[0]),
            b"done" => break,
            _ => nexo_sys::exit(181),
        }
    }
    let Some(fs) = fs else { nexo_sys::exit(182) };
    let Some(rng) = rng else { nexo_sys::exit(183) };
    fs_exercise(fs, false);
    use nexo_proto::ProtoError;
    use nexo_proto::rng::{FillRequest, decode_fill_response};
    let mut r1 = [0u8; 64];
    let mut r2 = [0u8; 64];
    let mut rbuf = [0u8; 4096];
    for (i, out) in [&mut r1, &mut r2].into_iter().enumerate() {
        let m = FillRequest { len: 32 }.encode_msg(&mut rbuf).unwrap_or(0);
        if nexo_sys::channel_send(rng, &rbuf[..m], &[]) != Status::Ok {
            nexo_sys::exit(184);
        }
        let n = match nexo_sys::channel_recv(rng, &mut rbuf, &mut hs) {
            Ok((n, _)) => n,
            Err(_) => nexo_sys::exit(186),
        };
        match decode_fill_response(&rbuf[..n]) {
            Ok(resp) if resp.data().len() == 32 => out[..32].copy_from_slice(resp.data()),
            other => {
                nexo_rt::log!("utest: rng: resposta {} inesperada: {:?}", i, other.err());
                nexo_sys::exit(185)
            }
        }
    }
    if r1[..32].iter().all(|&b| b == 0) || r1[..32] == r2[..32] {
        nexo_sys::log("utest: rng: bytes nulos ou repetidos");
        nexo_sys::exit(187);
    }
    // pedido invalido e recusado com erro tipado, sem derrubar o driver
    let m = FillRequest { len: 4096 }.encode_msg(&mut rbuf).unwrap_or(0);
    let _ = nexo_sys::channel_send(rng, &rbuf[..m], &[]);
    match nexo_sys::channel_recv(rng, &mut rbuf, &mut hs) {
        Ok((n, _)) if decode_fill_response(&rbuf[..n]) == Err(ProtoError::Remote(1)) => {}
        _ => nexo_sys::exit(188),
    }
    // mensagem malformada -> erro tipado 3
    let _ = nexo_sys::channel_send(rng, b"lixo", &[]);
    match nexo_sys::channel_recv(rng, &mut rbuf, &mut hs) {
        Ok((n, _)) if decode_fill_response(&rbuf[..n]) == Err(ProtoError::Remote(3)) => {}
        _ => nexo_sys::exit(199),
    }
    esp_checks(esp, &mut hs);
    nexo_rt::log!(
        "utest: devmgr ok (rng {:02x}{:02x}{:02x}{:02x}...)",
        r1[0],
        r1[1],
        r1[2],
        r1[3]
    );
    nexo_sys::exit(0)
}

/// Verifica a ESP pelo `espfs`: raiz com `EFI` e `nexo`, tamanhos e cabecalhos dos binarios.
fn esp_checks(esp: Option<nexo_sys::Handle>, hs: &mut [u32; 1]) {
    let Some(esp) = esp else { nexo_sys::exit(189) };
    let mut ereq = [0u8; 64];
    let mut ereply = [0u8; 4096];
    let call = |op: u8,
                offset: u64,
                len: u32,
                path: &[u8],
                _ereq: &mut [u8; 64],
                ereply: &mut [u8; 4096],
                hs: &mut [u32; 1]|
     -> (u8, u64, usize) {
        use nexo_proto::ProtoError;
        use nexo_proto::esp as pesp;
        let mut pbuf = [0u8; 256];
        let pn = path.len().min(256);
        pbuf[..pn].copy_from_slice(&path[..pn]);
        let mut msg = [0u8; 4096];
        let m = match op {
            0 => pesp::ListRequest {
                path: pbuf,
                path_len: pn as u32,
            }
            .encode_msg(&mut msg),
            1 => pesp::StatRequest {
                path: pbuf,
                path_len: pn as u32,
            }
            .encode_msg(&mut msg),
            _ => pesp::ReadRequest {
                path: pbuf,
                path_len: pn as u32,
                offset,
                len,
            }
            .encode_msg(&mut msg),
        }
        .unwrap_or(0);
        if nexo_sys::channel_send(esp, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(190);
        }
        let n = match nexo_sys::channel_recv(esp, ereply, hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(191),
        };
        fn remote(e: ProtoError) -> (u8, u64, usize) {
            match e {
                ProtoError::Remote(c) => (c as u8, 0, 0),
                _ => (0xfd, 0, 0),
            }
        }
        let copy: [u8; 4096] = *ereply;
        match op {
            0 => match pesp::decode_list_response(&copy[..n]) {
                Ok(r) => {
                    let dl = r.entries().len();
                    ereply[12..12 + dl].copy_from_slice(r.entries());
                    (0, r.count as u64, dl)
                }
                Err(e) => remote(e),
            },
            1 => match pesp::decode_stat_response(&copy[..n]) {
                Ok(r) => {
                    ereply[12] = r.attr;
                    ereply[13..17].copy_from_slice(&r.size.to_le_bytes());
                    (0, r.size as u64, 5)
                }
                Err(e) => remote(e),
            },
            _ => match pesp::decode_read_response(&copy[..n]) {
                Ok(r) => {
                    let dl = r.data().len();
                    ereply[12..12 + dl].copy_from_slice(r.data());
                    (0, dl as u64, dl)
                }
                Err(e) => remote(e),
            },
        }
    };
    let (st, count, n) = call(0, 0, 0, b"/", &mut ereq, &mut ereply, hs);
    if st != 0 || count < 2 {
        nexo_rt::log!(
            "utest: esp: listar raiz: status {} ({} entradas)",
            st,
            count
        );
        nexo_sys::exit(192);
    }
    let (mut has_efi, mut has_nexo, mut pos) = (false, false, 0usize);
    while pos + 6 <= n {
        let nl = ereply[12 + pos + 5] as usize;
        let name = &ereply[12 + pos + 6..12 + pos + 6 + nl];
        has_efi |= name.eq_ignore_ascii_case(b"EFI");
        has_nexo |= name.eq_ignore_ascii_case(b"nexo");
        pos += 6 + nl;
    }
    if !has_efi || !has_nexo {
        nexo_sys::exit(193);
    }
    let (st, boot_size, _) = call(
        1,
        0,
        0,
        b"/EFI/BOOT/BOOTX64.EFI",
        &mut ereq,
        &mut ereply,
        hs,
    );
    if st != 0 || boot_size == 0 {
        nexo_sys::exit(194);
    }
    // Layout A/B: basta UM slot com kernel ELF integro (o outro pode estar corrompido —
    // e exatamente o cenario do teste de fallback do loader, tools/test-ab).
    let mut kernel_size = 0u64;
    for path in [
        &b"/nexo/a/kernel.elf"[..],
        &b"/nexo/b/kernel.elf"[..],
        &b"/nexo/recovery/kernel.elf"[..],
    ] {
        let (st, size, _) = call(1, 0, 0, path, &mut ereq, &mut ereply, hs);
        if st != 0 || size == 0 {
            continue;
        }
        let (st, r, n) = call(2, 0, 4, path, &mut ereq, &mut ereply, hs);
        if st == 0 && r == 4 && n == 4 && &ereply[12..16] == b"\x7fELF" {
            kernel_size = size;
            break;
        }
    }
    if kernel_size == 0 {
        nexo_rt::log!("utest: esp: nenhum slot com kernel ELF integro");
        nexo_sys::exit(195);
    }
    let (st, r, _) = call(
        2,
        0,
        2,
        b"/EFI/BOOT/BOOTX64.EFI",
        &mut ereq,
        &mut ereply,
        hs,
    );
    if st != 0 || r != 2 || &ereply[12..14] != b"MZ" {
        nexo_sys::exit(197);
    }
    let (st, _, _) = call(1, 0, 0, b"/nexo/inexistente", &mut ereq, &mut ereply, hs);
    if st != 3 {
        nexo_sys::exit(198);
    }
    nexo_rt::log!(
        "utest: esp ok (BOOTX64.EFI {} bytes, kernel.elf {} bytes)",
        boot_size,
        kernel_size
    );
}

/// Modo 12: cliente do `vfs` — handle 0 = namespace completo (/boot /disk /tmp),
/// handle 1 = namespace so com /tmp. Verifica roteamento, ramfs e isolamento.
fn vfs_client() -> ! {
    let mut a = FsClient {
        ch: 0,
        req: [0; 4096],
        reply: [0; 4096],
    };
    // raiz do namespace completo: boot, disk, tmp
    let (count, n) = a.ok(6, 0, 0, 0, b"/", 210);
    if count != 3 {
        nexo_rt::log!("utest: vfs: raiz com {} entradas ({} bytes)", count, n);
        nexo_sys::exit(211);
    }
    // /boot: stat + leitura do cabecalho ELF por inode; escrita recusada (13)
    // Layout A/B: aceita o primeiro slot com kernel ELF integro (o outro pode estar
    // corrompido — o cenario do teste de fallback do loader, tools/test-ab).
    let mut kernel = None;
    for path in [
        &b"/boot/nexo/a/kernel.elf"[..],
        &b"/boot/nexo/b/kernel.elf"[..],
        &b"/boot/nexo/recovery/kernel.elf"[..],
    ] {
        let (st, kino, n) = a.call(0, 0, 0, 0, path);
        if st != 0 || n < 9 {
            continue;
        }
        let ksize = u64::from_le_bytes(a.data(n)[1..9].try_into().unwrap());
        if a.data(n)[0] != 1 || ksize == 0 {
            continue;
        }
        let kino = kino as u32;
        let (st, r, n) = a.call(4, kino, 0, 4, &[]);
        if st == 0 && r == 4 && a.data(n) == b"\x7fELF" {
            kernel = Some((kino, ksize));
            break;
        }
    }
    let Some((kino, ksize)) = kernel else {
        nexo_rt::log!("utest: vfs: nenhum slot com kernel ELF integro");
        nexo_sys::exit(213)
    };
    let (st, _, _) = a.call(5, kino, 0, 0, b"x");
    if st != 13 {
        nexo_rt::log!("utest: vfs: escrita no /boot devolveu {}", st);
        nexo_sys::exit(216);
    }
    // /tmp (ramfs): cria, escreve, le, lista, remove
    let (tino, _) = a.ok(1, 0, 0, 0, b"/tmp/nota.txt", 217);
    let tino = tino as u32;
    a.ok(5, tino, 0, 0, b"ola tmp", 218);
    let (r, n) = a.ok(4, tino, 0, 32, &[], 219);
    if r != 7 || a.data(n) != b"ola tmp" {
        nexo_sys::exit(220);
    }
    let (count, _) = a.ok(6, 0, 0, 0, b"/tmp", 221);
    if count != 1 {
        nexo_sys::exit(222);
    }
    a.ok(3, 0, 0, 0, b"/tmp/nota.txt", 223);
    let (st, _, _) = a.call(0, 0, 0, 0, b"/tmp/nota.txt");
    if st != 3 {
        nexo_sys::exit(224);
    }
    // /disk (NexoFS via vfs): cria, escreve, le por inode, remove
    let _ = a.call(3, 0, 0, 0, b"/disk/vfs.txt");
    let (dino, _) = a.ok(1, 0, 0, 0, b"/disk/vfs.txt", 225);
    let dino = dino as u32;
    a.ok(5, dino, 0, 0, b"via vfs", 226);
    let (r, n) = a.ok(4, dino, 0, 32, &[], 227);
    if r != 7 || a.data(n) != b"via vfs" {
        nexo_sys::exit(228);
    }
    let (st, sino, n) = a.call(0, 0, 0, 0, b"/disk/vfs.txt");
    if st != 0
        || sino != dino as u64
        || u64::from_le_bytes(a.data(n)[1..9].try_into().unwrap()) != 7
    {
        nexo_sys::exit(229);
    }
    a.ok(3, 0, 0, 0, b"/disk/vfs.txt", 230);
    // criar fora de montagem
    let (st, _, _) = a.call(1, 0, 0, 0, b"/qualquer");
    if st != 3 && st != 11 {
        nexo_sys::exit(231);
    }
    // namespace restrito (handle 1): so /tmp; /disk e /boot nao existem
    let mut b = FsClient {
        ch: 1,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let (count, _) = b.ok(6, 0, 0, 0, b"/", 232);
    if count != 1 {
        nexo_rt::log!("utest: vfs: namespace restrito com {} entradas", count);
        nexo_sys::exit(233);
    }
    let (st, _, _) = b.call(0, 0, 0, 0, b"/disk");
    if st != 3 {
        nexo_sys::exit(234);
    }
    let (st, _, _) = b.call(0, 0, 0, 0, b"/boot/nexo/a/kernel.elf");
    if st != 3 {
        nexo_sys::exit(235);
    }
    let (xino, _) = b.ok(1, 0, 0, 0, b"/tmp/isolada", 236);
    b.ok(5, xino as u32, 0, 0, b"so aqui", 237);
    // o arquivo do namespace restrito nao aparece no completo? (ramfs e por instancia)
    let (st, _, _) = a.call(0, 0, 0, 0, b"/tmp/isolada");
    if st != 3 {
        nexo_rt::log!("utest: vfs: ramfs vazou entre namespaces ({})", st);
        nexo_sys::exit(238);
    }
    nexo_rt::log!(
        "utest: vfs ok (namespaces isolados, kernel.elf {} bytes)",
        ksize
    );
    nexo_sys::exit(0)
}

/// Modo 13: cliente do `inputdev` (handle 0): espera 3 teclas pressionadas (EV_KEY, value 1)
/// injetadas pelo host via QMP e registra os codigos.
fn input_client() -> ! {
    use nexo_proto::input::{PollRequest, decode_poll_response};
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut keys = 0u32;
    let mut last_code = 0u16;
    let start = nexo_sys::time_now();
    loop {
        let m = PollRequest {}.encode_msg(&mut msg).unwrap_or(0);
        if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(240);
        }
        let mut events = [0u8; 3500];
        let n = match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
            Ok((n, _)) => match decode_poll_response(&msg[..n]) {
                Ok(r) => {
                    let len = r.events().len();
                    events[..len].copy_from_slice(r.events());
                    len
                }
                Err(_) => nexo_sys::exit(241),
            },
            _ => nexo_sys::exit(241),
        };
        let mut off = 0usize;
        while off + 8 <= n {
            let ty = u16::from_le_bytes(events[off..off + 2].try_into().unwrap());
            let code = u16::from_le_bytes(events[off + 2..off + 4].try_into().unwrap());
            let value = u32::from_le_bytes(events[off + 4..off + 8].try_into().unwrap());
            if ty == 1 && value == 1 {
                keys += 1;
                last_code = code;
                nexo_rt::log!("utest: input: tecla code={}", code);
            }
            off += 8;
        }
        if keys >= 3 {
            nexo_rt::log!(
                "utest: input ok ({} teclas, ultima code={})",
                keys,
                last_code
            );
            nexo_sys::exit(0)
        }
        if nexo_sys::time_now() - start > 30_000_000_000 {
            nexo_rt::log!("utest: input: apenas {} tecla(s) em 30 s", keys);
            nexo_sys::exit(242)
        }
        nexo_sys::sleep_ns(10_000_000);
    }
}

/// Modo 14: cliente do `netdev` (handle 0): pergunta o MAC, manda um ARP request pelo
/// gateway do slirp (10.0.2.2) e espera o ARP reply — um pacote de verdade indo e voltando.
fn net_client(tcp_port: u16) -> ! {
    use nexo_proto::net::{
        MacRequest, RecvRequest, SendRequest, decode_mac_response, decode_recv_response,
        decode_send_response,
    };
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    let m = MacRequest {}.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(250);
    }
    let mac = match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
        Ok((n, _)) => match decode_mac_response(&msg[..n]) {
            Ok(r) if r.addr().len() == 6 => {
                let mut m6 = [0u8; 6];
                m6.copy_from_slice(r.addr());
                m6
            }
            _ => nexo_sys::exit(251),
        },
        _ => nexo_sys::exit(251),
    };
    nexo_rt::log!(
        "utest: net: mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    );
    // IPv6/NDP: emite um Neighbor Solicitation valido pelo link-local do gateway; o harness
    // confirma no pcap que saiu um ICMPv6 tipo 135 bem-formado (nexo-netstack).
    {
        use nexo_netstack::ipv6;
        let me = ipv6::link_local(mac);
        // alvo: link-local derivado do MAC do gateway do slirp (52:55:0a:00:02:02)
        let gw_mac = [0x52u8, 0x55, 0x0a, 0x00, 0x02, 0x02];
        let target = ipv6::link_local(gw_mac);
        let mut ns = SendRequest {
            frame: [0; 1514],
            frame_len: 0,
        };
        let n = ipv6::neighbor_solicitation(&mut ns.frame, mac, &me, &target);
        ns.frame_len = n as u32;
        let m = ns.encode_msg(&mut msg).unwrap_or(0);
        if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(265);
        }
        match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
            Ok((n, _)) if decode_send_response(&msg[..n]).is_ok() => {}
            _ => nexo_sys::exit(266),
        }
        nexo_rt::log!(
            "utest: ipv6 ok — Neighbor Solicitation emitido (link-local {:02x}{:02x}..)",
            me[0],
            me[1]
        );
    }
    // DHCP: DISCOVER -> OFFER -> REQUEST -> ACK contra o servidor do slirp.
    let lease = dhcp_handshake(mac);
    nexo_rt::log!(
        "utest: dhcp ok — ip {}.{}.{}.{} mascara {}.{}.{}.{} gw {}.{}.{}.{} dns {}.{}.{}.{}",
        lease.ip[0],
        lease.ip[1],
        lease.ip[2],
        lease.ip[3],
        lease.mask[0],
        lease.mask[1],
        lease.mask[2],
        lease.mask[3],
        lease.router[0],
        lease.router[1],
        lease.router[2],
        lease.router[3],
        lease.dns[0],
        lease.dns[1],
        lease.dns[2],
        lease.dns[3]
    );
    // ARP request: quem tem 10.0.2.2? (nosso IP: 10.0.2.15, padrao do slirp)
    let mut arp = SendRequest {
        frame: [0; 1514],
        frame_len: 42,
    };
    let f = &mut arp.frame;
    f[0..6].fill(0xff); // broadcast
    f[6..12].copy_from_slice(&mac);
    f[12..14].copy_from_slice(&0x0806u16.to_be_bytes()); // ARP
    f[14..16].copy_from_slice(&1u16.to_be_bytes()); // Ethernet
    f[16..18].copy_from_slice(&0x0800u16.to_be_bytes()); // IPv4
    f[18] = 6;
    f[19] = 4;
    f[20..22].copy_from_slice(&1u16.to_be_bytes()); // request
    f[22..28].copy_from_slice(&mac);
    f[28..32].copy_from_slice(&lease.ip);
    f[38..42].copy_from_slice(&[10, 0, 2, 2]);
    let m = arp.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(252);
    }
    match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
        Ok((n, _)) if decode_send_response(&msg[..n]).is_ok() => {}
        _ => nexo_sys::exit(253),
    }
    // Espera o ARP reply do gateway.
    let start = nexo_sys::time_now();
    loop {
        let m = RecvRequest {}.encode_msg(&mut msg).unwrap_or(0);
        if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(254);
        }
        let n = match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(255),
        };
        let mut frame = [0u8; 1514];
        let flen = match decode_recv_response(&msg[..n]) {
            Ok(r) => {
                let l = r.frame().len();
                frame[..l].copy_from_slice(r.frame());
                l
            }
            Err(_) => nexo_sys::exit(256),
        };
        if flen >= 42
            && frame[12..14] == 0x0806u16.to_be_bytes()
            && frame[20..22] == 2u16.to_be_bytes()
            && frame[28..32] == lease.router
        {
            let mut gw = [0u8; 6];
            gw.copy_from_slice(&frame[22..28]);
            nexo_rt::log!(
                "utest: net: ARP reply do gateway ({:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})",
                gw[0],
                gw[1],
                gw[2],
                gw[3],
                gw[4],
                gw[5]
            );
            net_ping(mac, gw, lease.ip, lease.router, lease.dns, tcp_port);
        }
        if nexo_sys::time_now() - start > 20_000_000_000 {
            nexo_rt::log!("utest: net: sem ARP reply em 20 s");
            nexo_sys::exit(257)
        }
        nexo_sys::sleep_ns(20_000_000);
    }
}

/// Ping ICMP ao gateway com a biblioteca `nexo-netstack`; sai com 0 no echo reply.
fn net_ping(
    mac: [u8; 6],
    gw: [u8; 6],
    my_ip: [u8; 4],
    gw_ip: [u8; 4],
    dns_ip: [u8; 4],
    tcp_port: u16,
) -> ! {
    use nexo_netstack as nsk;
    use nexo_proto::net::{RecvRequest, SendRequest, decode_recv_response, decode_send_response};
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut send = SendRequest {
        frame: [0; 1514],
        frame_len: 0,
    };
    let n = nsk::icmp_echo_request(
        &mut send.frame,
        mac,
        gw,
        my_ip,
        gw_ip,
        0x4e58,
        1,
        b"nexo-ping",
    );
    send.frame_len = n as u32;
    let m = send.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(258);
    }
    match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
        Ok((n, _)) if decode_send_response(&msg[..n]).is_ok() => {}
        _ => nexo_sys::exit(259),
    }
    let start = nexo_sys::time_now();
    loop {
        let m = RecvRequest {}.encode_msg(&mut msg).unwrap_or(0);
        if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(260);
        }
        let n = match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(261),
        };
        let mut frame = [0u8; 1514];
        let flen = match decode_recv_response(&msg[..n]) {
            Ok(r) => {
                let l = r.frame().len();
                frame[..l].copy_from_slice(r.frame());
                l
            }
            Err(_) => nexo_sys::exit(262),
        };
        if let Some((ttl, data)) = nsk::icmp_echo_reply(&frame[..flen], gw_ip, 0x4e58, 1) {
            if data == b"nexo-ping" {
                nexo_rt::log!(
                    "utest: net ok — echo reply de 10.0.2.2 (ttl={}, 9 bytes)",
                    ttl
                );
                dns_lookup(mac, gw, my_ip, dns_ip, tcp_port);
            }
            nexo_sys::exit(263)
        }
        if nexo_sys::time_now() - start > 20_000_000_000 {
            nexo_rt::log!("utest: net: sem echo reply em 20 s");
            nexo_sys::exit(264)
        }
        nexo_sys::sleep_ns(20_000_000);
    }
}

/// DISCOVER → OFFER → REQUEST → ACK; devolve o lease confirmado.
fn dhcp_handshake(mac: [u8; 6]) -> nexo_netstack::DhcpLease {
    use nexo_netstack as nsk;
    use nexo_proto::net::{RecvRequest, SendRequest, decode_recv_response, decode_send_response};
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    let xid = 0x4e58_0001u32;
    let send_frame = |req: Option<([u8; 4], [u8; 4])>, msg: &mut [u8; 4096], hs: &mut [u32; 1]| {
        let mut sr = SendRequest {
            frame: [0; 1514],
            frame_len: 0,
        };
        let n = nsk::dhcp_build(&mut sr.frame, mac, xid, req);
        sr.frame_len = n as u32;
        let m = sr.encode_msg(msg).unwrap_or(0);
        if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(270);
        }
        match nexo_sys::channel_recv(0, msg, hs) {
            Ok((n, _)) if decode_send_response(&msg[..n]).is_ok() => {}
            _ => nexo_sys::exit(271),
        }
    };
    let wait_kind = |want: u8, msg: &mut [u8; 4096], hs: &mut [u32; 1]| -> nsk::DhcpLease {
        let start = nexo_sys::time_now();
        loop {
            let m = RecvRequest {}.encode_msg(msg).unwrap_or(0);
            if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
                nexo_sys::exit(272);
            }
            let n = match nexo_sys::channel_recv(0, msg, hs) {
                Ok((n, _)) => n,
                _ => nexo_sys::exit(273),
            };
            let mut frame = [0u8; 1514];
            let flen = match decode_recv_response(&msg[..n]) {
                Ok(r) => {
                    let l = r.frame().len();
                    frame[..l].copy_from_slice(r.frame());
                    l
                }
                Err(_) => nexo_sys::exit(274),
            };
            if let Some((kind, lease)) = nsk::dhcp_parse(&frame[..flen], xid)
                && kind == want
            {
                return lease;
            }
            if nexo_sys::time_now() - start > 20_000_000_000 {
                nexo_rt::log!("utest: dhcp: sem resposta tipo {} em 20 s", want);
                nexo_sys::exit(275)
            }
            nexo_sys::sleep_ns(20_000_000);
        }
    };
    send_frame(None, &mut msg, &mut hs);
    let offer = wait_kind(2, &mut msg, &mut hs);
    send_frame(Some((offer.ip, offer.server)), &mut msg, &mut hs);
    let ack = wait_kind(5, &mut msg, &mut hs);
    if ack.ip == [0, 0, 0, 0] || ack.router == [0, 0, 0, 0] {
        nexo_sys::exit(276);
    }
    ack
}

/// Consulta DNS A por `example.com` ao servidor do lease (o slirp encaminha ao resolvedor do
/// host); qualquer resposta valida com o mesmo id conta como sucesso (o rcode e registrado).
fn dns_lookup(mac: [u8; 6], gw: [u8; 6], my_ip: [u8; 4], dns_ip: [u8; 4], tcp_port: u16) -> ! {
    use nexo_netstack as nsk;
    use nexo_proto::net::{RecvRequest, SendRequest, decode_recv_response, decode_send_response};
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut send = SendRequest {
        frame: [0; 1514],
        frame_len: 0,
    };
    let Some(n) = nsk::dns_query(
        &mut send.frame,
        mac,
        gw,
        my_ip,
        dns_ip,
        40000,
        0x4e59,
        b"example.com",
    ) else {
        nexo_sys::exit(280)
    };
    send.frame_len = n as u32;
    let m = send.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(281);
    }
    match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
        Ok((n, _)) if decode_send_response(&msg[..n]).is_ok() => {}
        _ => nexo_sys::exit(282),
    }
    let start = nexo_sys::time_now();
    loop {
        let m = RecvRequest {}.encode_msg(&mut msg).unwrap_or(0);
        if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
            nexo_sys::exit(283);
        }
        let n = match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(284),
        };
        let mut frame = [0u8; 1514];
        let flen = match decode_recv_response(&msg[..n]) {
            Ok(r) => {
                let l = r.frame().len();
                frame[..l].copy_from_slice(r.frame());
                l
            }
            Err(_) => nexo_sys::exit(285),
        };
        if let Some(ans) = nsk::dns_parse(&frame[..flen], dns_ip, 40000, 0x4e59) {
            match ans.a {
                Some(a) => nexo_rt::log!(
                    "utest: dns ok — example.com rcode={} A={}.{}.{}.{}",
                    ans.rcode,
                    a[0],
                    a[1],
                    a[2],
                    a[3]
                ),
                None => {
                    nexo_rt::log!(
                        "utest: dns ok — resposta rcode={} ({} registros)",
                        ans.rcode,
                        ans.answers
                    );
                }
            }
            if tcp_port == 0 {
                nexo_sys::exit(0)
            }
            tcp_check(mac, gw, my_ip, tcp_port);
        }
        if nexo_sys::time_now() - start > 20_000_000_000 {
            nexo_rt::log!("utest: dns: sem resposta em 20 s");
            nexo_sys::exit(286)
        }
        nexo_sys::sleep_ns(20_000_000);
    }
}

/// Handshake TCP + eco com um servidor no host (10.0.2.2:`port`, atras do slirp):
/// SYN -> SYN-ACK -> ACK, envia "ola tcp\n", espera "nexo-tcp-ok" e encerra com RST.
fn tcp_check(mac: [u8; 6], gw: [u8; 6], my_ip: [u8; 4], port: u16) -> ! {
    use nexo_netstack as nsk;
    use nexo_proto::net::{RecvRequest, SendRequest, decode_recv_response, decode_send_response};
    let host: [u8; 4] = [10, 0, 2, 2];
    let sport = 40100u16;
    let iss = 0x4e58_2000u32;
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    let tx =
        |seq: u32, ack: u32, flags: u8, payload: &[u8], msg: &mut [u8; 4096], hs: &mut [u32; 1]| {
            let mut sr = SendRequest {
                frame: [0; 1514],
                frame_len: 0,
            };
            let n = nsk::tcp_write(
                &mut sr.frame,
                mac,
                gw,
                my_ip,
                host,
                sport,
                port,
                seq,
                ack,
                flags,
                8192,
                payload,
            );
            sr.frame_len = n as u32;
            let m = sr.encode_msg(msg).unwrap_or(0);
            if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
                nexo_sys::exit(290);
            }
            match nexo_sys::channel_recv(0, msg, hs) {
                Ok((n, _)) if decode_send_response(&msg[..n]).is_ok() => {}
                _ => nexo_sys::exit(291),
            }
        };
    let rx = |want: u8,
              msg: &mut [u8; 4096],
              hs: &mut [u32; 1],
              out: &mut [u8; 1024]|
     -> (u32, u32, u8, usize) {
        let start = nexo_sys::time_now();
        loop {
            let m = RecvRequest {}.encode_msg(msg).unwrap_or(0);
            if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
                nexo_sys::exit(292);
            }
            let n = match nexo_sys::channel_recv(0, msg, hs) {
                Ok((n, _)) => n,
                _ => nexo_sys::exit(293),
            };
            let mut frame = [0u8; 1514];
            let flen = match decode_recv_response(&msg[..n]) {
                Ok(r) => {
                    let l = r.frame().len();
                    frame[..l].copy_from_slice(r.frame());
                    l
                }
                Err(_) => nexo_sys::exit(294),
            };
            if let Some(seg) = nsk::tcp_parse(&frame[..flen], host, sport)
                && seg.src_port == port
                && seg.flags & want == want
            {
                let dl = seg.payload.len().min(1024);
                out[..dl].copy_from_slice(&seg.payload[..dl]);
                return (seg.seq, seg.ack, seg.flags, dl);
            }
            if nexo_sys::time_now() - start > 20_000_000_000 {
                nexo_rt::log!("utest: tcp: sem segmento com flags {:#x} em 20 s", want);
                nexo_sys::exit(295)
            }
            nexo_sys::sleep_ns(10_000_000);
        }
    };
    let mut data = [0u8; 1024];
    // 1. SYN -> SYN-ACK -> ACK
    tx(iss, 0, nsk::TCP_SYN, b"", &mut msg, &mut hs);
    let (srv_seq, srv_ack, _, _) = rx(nsk::TCP_SYN | nsk::TCP_ACK, &mut msg, &mut hs, &mut data);
    if srv_ack != iss.wrapping_add(1) {
        nexo_sys::exit(296);
    }
    let mut my_seq = iss.wrapping_add(1);
    let mut their_seq = srv_seq.wrapping_add(1);
    tx(my_seq, their_seq, nsk::TCP_ACK, b"", &mut msg, &mut hs);
    nexo_rt::log!("utest: tcp: handshake completo com 10.0.2.2:{}", port);
    // 2. envia a linha e espera a resposta do servidor
    let payload = b"ola tcp\n";
    tx(
        my_seq,
        their_seq,
        nsk::TCP_ACK | nsk::TCP_PSH,
        payload,
        &mut msg,
        &mut hs,
    );
    my_seq = my_seq.wrapping_add(payload.len() as u32);
    let (dseq, _, _, dl) = rx(nsk::TCP_ACK | nsk::TCP_PSH, &mut msg, &mut hs, &mut data);
    if &data[..dl] != b"nexo-tcp-ok" {
        nexo_rt::log!("utest: tcp: resposta inesperada ({} bytes)", dl);
        nexo_sys::exit(297);
    }
    their_seq = dseq.wrapping_add(dl as u32);
    tx(my_seq, their_seq, nsk::TCP_ACK, b"", &mut msg, &mut hs);
    nexo_rt::log!(
        "utest: tcp ok — dados recebidos de 10.0.2.2:{} ({} bytes)",
        port,
        dl
    );
    // 3. encerra sem estados demorados (RST)
    tx(
        my_seq,
        their_seq,
        nsk::TCP_RST | nsk::TCP_ACK,
        b"",
        &mut msg,
        &mut hs,
    );
    nexo_sys::exit(0)
}

/// Modo 15: cliente do `netd` (handle 0, protocolo `nexo.sock`): info, DNS com cache,
/// eco UDP e conexao TCP com os servidores do harness no host (10.0.2.2).
fn sock_client(tcp_port: u16, udp_port: u16, http_port: u16) -> ! {
    use nexo_proto::sock::{
        InfoRequest, ResolveRequest, TcpCloseRequest, TcpConnectRequest, TcpRecvRequest,
        TcpSendRequest, UdpRecvRequest, UdpSendRequest, decode_info_response,
        decode_resolve_response, decode_tcp_close_response, decode_tcp_connect_response,
        decode_tcp_recv_response, decode_tcp_send_response, decode_udp_recv_response,
        decode_udp_send_response,
    };
    let mut msg = [0u8; 4096];
    let mut hs = [0u32; 1];
    macro_rules! call {
        ($req:expr, $dec:ident, $code:expr) => {{
            let m = $req.encode_msg(&mut msg).unwrap_or(0);
            if nexo_sys::channel_send(0, &msg[..m], &[]) != Status::Ok {
                nexo_sys::exit($code);
            }
            let n = match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
                Ok((n, _)) => n,
                _ => nexo_sys::exit($code + 1),
            };
            match $dec(&msg[..n]) {
                Ok(r) => r,
                Err(e) => {
                    nexo_rt::log!("utest: sock: erro {:?} (passo {})", e, $code);
                    nexo_sys::exit($code + 2)
                }
            }
        }};
    }
    // 1. info
    let info = call!(InfoRequest {}, decode_info_response, 300);
    if info.ip() != [10, 0, 2, 15] || info.gateway() != [10, 0, 2, 2] {
        nexo_sys::exit(303);
    }
    nexo_rt::log!(
        "utest: sock info — ip {}.{}.{}.{} gw {}.{}.{}.{}",
        info.ip()[0],
        info.ip()[1],
        info.ip()[2],
        info.ip()[3],
        info.gateway()[0],
        info.gateway()[1],
        info.gateway()[2],
        info.gateway()[3]
    );
    // 2. DNS com cache: segunda consulta deve vir do cache
    let mut name = ResolveRequest {
        name: [0; 253],
        name_len: 11,
    };
    name.name[..11].copy_from_slice(b"example.com");
    let r1 = call!(name.clone(), decode_resolve_response, 310);
    let r2 = call!(name, decode_resolve_response, 314);
    if r1.cached != 0 || r2.cached != 1 || r1.addr() != r2.addr() {
        nexo_rt::log!(
            "utest: sock dns: cached {}/{} divergente",
            r1.cached,
            r2.cached
        );
        nexo_sys::exit(318);
    }
    nexo_rt::log!(
        "utest: sock dns ok — example.com = {}.{}.{}.{} (2a consulta do cache)",
        r1.addr()[0],
        r1.addr()[1],
        r1.addr()[2],
        r1.addr()[3]
    );
    // 3. eco UDP com o servidor do harness
    let mut u = UdpSendRequest {
        dst_ip: [10, 0, 2, 2],
        dst_ip_len: 4,
        dst_port: udp_port,
        src_port: 40200,
        data: [0; 1400],
        data_len: 8,
    };
    u.data[..8].copy_from_slice(b"nexo-udp");
    call!(u, decode_udp_send_response, 320);
    let start = nexo_sys::time_now();
    loop {
        let r = call!(
            UdpRecvRequest { port: 40200 },
            decode_udp_recv_response,
            324
        );
        if r.data().is_empty() {
            if nexo_sys::time_now() - start > 20_000_000_000 {
                nexo_rt::log!("utest: sock udp: sem eco em 20 s");
                nexo_sys::exit(328)
            }
            nexo_sys::sleep_ns(20_000_000);
            continue;
        }
        if r.data() != b"nexo-udp-ok" || r.from_ip() != [10, 0, 2, 2] {
            nexo_sys::exit(329);
        }
        break;
    }
    nexo_rt::log!("utest: sock udp ok — eco de 10.0.2.2:{}", udp_port);
    // 4. TCP pela API de sockets
    let c = call!(
        TcpConnectRequest {
            dst_ip: [10, 0, 2, 2],
            dst_ip_len: 4,
            dst_port: tcp_port,
        },
        decode_tcp_connect_response,
        330
    );
    let mut t = TcpSendRequest {
        conn: c.conn,
        data: [0; 1400],
        data_len: 9,
    };
    t.data[..9].copy_from_slice(b"ola netd\n");
    call!(t, decode_tcp_send_response, 334);
    let start = nexo_sys::time_now();
    loop {
        let r = call!(
            TcpRecvRequest { conn: c.conn },
            decode_tcp_recv_response,
            338
        );
        if !r.data().is_empty() {
            if r.data() != b"nexo-tcp-ok" {
                nexo_sys::exit(342);
            }
            break;
        }
        if r.closed != 0 {
            nexo_sys::exit(343);
        }
        if nexo_sys::time_now() - start > 20_000_000_000 {
            nexo_rt::log!("utest: sock tcp: sem resposta em 20 s");
            nexo_sys::exit(344)
        }
        nexo_sys::sleep_ns(20_000_000);
    }
    call!(
        TcpCloseRequest { conn: c.conn },
        decode_tcp_close_response,
        346
    );
    nexo_rt::log!(
        "utest: sock tcp ok — conectou, enviou e recebeu por 10.0.2.2:{}",
        tcp_port
    );
    // 5. escuta: o harness conecta em nos via hostfwd (porta 8080 do convidado)
    use nexo_proto::sock::{TcpListenRequest, decode_tcp_listen_response};
    let l = call!(
        TcpListenRequest { port: 8080 },
        decode_tcp_listen_response,
        350
    );
    nexo_rt::log!(
        "utest: sock listen — conexao de entrada de {}.{}.{}.{}:{}",
        l.peer_ip()[0],
        l.peer_ip()[1],
        l.peer_ip()[2],
        l.peer_ip()[3],
        l.peer_port
    );
    let start = nexo_sys::time_now();
    loop {
        let r = call!(
            TcpRecvRequest { conn: l.conn },
            decode_tcp_recv_response,
            354
        );
        if !r.data().is_empty() {
            if r.data() != b"ola do host\n" {
                nexo_sys::exit(358);
            }
            break;
        }
        if r.closed != 0 {
            nexo_sys::exit(359);
        }
        if nexo_sys::time_now() - start > 20_000_000_000 {
            nexo_rt::log!("utest: sock listen: sem dados em 20 s");
            nexo_sys::exit(360)
        }
        nexo_sys::sleep_ns(20_000_000);
    }
    let mut t2 = TcpSendRequest {
        conn: l.conn,
        data: [0; 1400],
        data_len: 14,
    };
    t2.data[..14].copy_from_slice(b"nexo-listen-ok");
    call!(t2, decode_tcp_send_response, 362);
    call!(
        TcpCloseRequest { conn: l.conn },
        decode_tcp_close_response,
        366
    );
    nexo_rt::log!("utest: sock listen ok — servimos uma conexao de entrada na porta 8080");
    // 6. cliente HTTP/1.0 minimo sobre a API de sockets
    if http_port != 0 {
        let h = call!(
            TcpConnectRequest {
                dst_ip: [10, 0, 2, 2],
                dst_ip_len: 4,
                dst_port: http_port,
            },
            decode_tcp_connect_response,
            370
        );
        let req = b"GET /nexo.txt HTTP/1.0\r\nHost: 10.0.2.2\r\n\r\n";
        let mut t3 = TcpSendRequest {
            conn: h.conn,
            data: [0; 1400],
            data_len: req.len() as u32,
        };
        t3.data[..req.len()].copy_from_slice(req);
        call!(t3, decode_tcp_send_response, 374);
        let mut resp = [0u8; 2048];
        let mut rlen = 0usize;
        let start = nexo_sys::time_now();
        loop {
            let r = call!(
                TcpRecvRequest { conn: h.conn },
                decode_tcp_recv_response,
                378
            );
            if !r.data().is_empty() && rlen + r.data().len() <= resp.len() {
                resp[rlen..rlen + r.data().len()].copy_from_slice(r.data());
                rlen += r.data().len();
            }
            let done = rlen >= 16 && resp[..rlen].windows(12).any(|w| w == b"nexo-http-ok");
            if done {
                break;
            }
            if r.closed != 0 {
                nexo_rt::log!(
                    "utest: http: conexao fechou com {} bytes sem o corpo esperado",
                    rlen
                );
                nexo_sys::exit(382)
            }
            if nexo_sys::time_now() - start > 20_000_000_000 {
                nexo_rt::log!("utest: http: sem resposta em 20 s ({} bytes)", rlen);
                nexo_sys::exit(383)
            }
            nexo_sys::sleep_ns(20_000_000);
        }
        if !resp[..rlen].starts_with(b"HTTP/1.0 200") {
            nexo_sys::exit(384);
        }
        call!(
            TcpCloseRequest { conn: h.conn },
            decode_tcp_close_response,
            386
        );
        nexo_rt::log!(
            "utest: http ok — GET /nexo.txt devolveu 200 e o corpo esperado ({} bytes)",
            rlen
        );
    }
    // 6b. personalidade POSIX de sockets: socket()/connect()/send()/recv()/close() sobre nexo.sock,
    //     contra o mesmo servidor TCP do host — prova a camada de compatibilidade BSD.
    {
        use nexo_net::{AF_INET, SOCK_STREAM, SockAddrIn, Sockets};
        let mut s = Sockets::new(0);
        let fd = s.socket(AF_INET, SOCK_STREAM);
        if fd < 0 {
            nexo_sys::exit(400 - fd as i64);
        }
        let addr = SockAddrIn {
            addr: [10, 0, 2, 2],
            port: tcp_port,
        };
        if s.connect(fd, &addr) != 0 {
            nexo_sys::exit(401);
        }
        if s.send(fd, b"ola posix\n") < 0 {
            nexo_sys::exit(402);
        }
        let mut buf = [0u8; 64];
        let r = s.recv(fd, &mut buf);
        if r <= 0 || &buf[..r as usize] != b"nexo-tcp-ok" {
            nexo_rt::log!("utest: posix recv devolveu {}", r);
            nexo_sys::exit(403);
        }
        s.close(fd);
        nexo_rt::log!(
            "utest: posix sockets ok — socket/connect/send/recv/close via BSD sobre nexo.sock"
        );
    }
    // 7. multi-cliente + firewall: abre uma segunda sessao RESTRITA (perfil que so permite TCP
    //    para 10.0.2.2:<tcp_port>, sem DNS nem escuta) e comprova que o netd nega o resto.
    {
        use nexo_proto::sock::{
            OpenRequest, ResolveRequest, TcpConnectRequest, UdpSendRequest, decode_open_response,
            decode_resolve_response, decode_tcp_connect_response, decode_udp_send_response,
        };
        let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(388));
        let open = OpenRequest {
            chan: theirs,
            allow_dns: 0,
            allow_listen: 0,
            rule_ip: [10, 0, 2, 2],
            rule_ip_len: 4,
            rule_prefix: 32,
            rule_port_lo: tcp_port,
            rule_port_hi: tcp_port,
            rule_protos: 1, // so TCP
        };
        let m = open.encode_msg(&mut msg).unwrap_or(0);
        // o handle `theirs` sai da nossa tabela e entra na do netd
        if nexo_sys::channel_send(0, &msg[..m], &[theirs]) != Status::Ok {
            nexo_sys::exit(389);
        }
        match nexo_sys::channel_recv(0, &mut msg, &mut hs) {
            Ok((n, _)) if decode_open_response(&msg[..n]).is_ok() => {}
            _ => nexo_sys::exit(390),
        }
        let mut ask = |req_bytes: &[u8], out: &mut [u8; 4096]| -> usize {
            if nexo_sys::channel_send(mine, req_bytes, &[]) != Status::Ok {
                nexo_sys::exit(391);
            }
            match nexo_sys::channel_recv(mine, out, &mut hs) {
                Ok((n, _)) => n,
                _ => nexo_sys::exit(392),
            }
        };
        // permitido: TCP para 10.0.2.2:<tcp_port>
        let m = TcpConnectRequest {
            dst_ip: [10, 0, 2, 2],
            dst_ip_len: 4,
            dst_port: tcp_port,
        }
        .encode_msg(&mut msg)
        .unwrap_or(0);
        let req = msg;
        let n = ask(&req[..m], &mut msg);
        if decode_tcp_connect_response(&msg[..n]).is_err() {
            nexo_rt::log!("utest: firewall: conexao permitida foi negada");
            nexo_sys::exit(394);
        }
        // negado: TCP para outra porta (mesmo host)
        let m = TcpConnectRequest {
            dst_ip: [10, 0, 2, 2],
            dst_ip_len: 4,
            dst_port: tcp_port.wrapping_add(1),
        }
        .encode_msg(&mut msg)
        .unwrap_or(0);
        let req = msg;
        let n = ask(&req[..m], &mut msg);
        if decode_tcp_connect_response(&msg[..n]) != Err(nexo_proto::ProtoError::Remote(7)) {
            nexo_sys::exit(395);
        }
        // negado: DNS (perfil sem allow_dns)
        let mut r = ResolveRequest {
            name: [0; 253],
            name_len: 11,
        };
        r.name[..11].copy_from_slice(b"example.com");
        let m = r.encode_msg(&mut msg).unwrap_or(0);
        let req = msg;
        let n = ask(&req[..m], &mut msg);
        if decode_resolve_response(&msg[..n]) != Err(nexo_proto::ProtoError::Remote(7)) {
            nexo_sys::exit(396);
        }
        // negado: UDP (perfil so-TCP)
        let mut u = UdpSendRequest {
            dst_ip: [10, 0, 2, 2],
            dst_ip_len: 4,
            dst_port: 9,
            src_port: 40300,
            data: [0; 1400],
            data_len: 1,
        };
        u.data[0] = b'x';
        let m = u.encode_msg(&mut msg).unwrap_or(0);
        let req = msg;
        let n = ask(&req[..m], &mut msg);
        if decode_udp_send_response(&msg[..n]) != Err(nexo_proto::ProtoError::Remote(7)) {
            nexo_sys::exit(397);
        }
        nexo_rt::log!("utest: firewall ok — sessao restrita: TCP permitido, DNS/UDP/porta negados");
    }
    nexo_sys::exit(0)
}

/// Modo 16: espera múltipla (`channel_wait_any`) — pronto imediato, mensagem chegando e
/// par fechado, tudo dentro de um só processo (os dois lados dos canais são nossos).
fn wait_any_test() -> ! {
    let (a1, b1) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(400));
    let (a2, b2) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(401));
    // 1. mensagem ja na fila do canal 2 -> indice 1
    if nexo_sys::channel_send(b2, b"oi", &[]) != Status::Ok {
        nexo_sys::exit(402);
    }
    match nexo_sys::channel_wait_any(&[a1, a2]) {
        Ok(1) => {}
        r => {
            nexo_rt::log!("utest: wait_any pronto-imediato devolveu {:?}", r);
            nexo_sys::exit(403)
        }
    }
    let mut buf = [0u8; 16];
    let mut hs = [0u32; 1];
    let _ = nexo_sys::channel_recv(a2, &mut buf, &mut hs);
    // 2. canal 1 na frente do array -> indice 0
    if nexo_sys::channel_send(b1, b"outra", &[]) != Status::Ok {
        nexo_sys::exit(404);
    }
    match nexo_sys::channel_wait_any(&[a1, a2]) {
        Ok(0) => {}
        r => {
            nexo_rt::log!("utest: wait_any indice 0 devolveu {:?}", r);
            nexo_sys::exit(405)
        }
    }
    let _ = nexo_sys::channel_recv(a1, &mut buf, &mut hs);
    // 3. par fechado conta como pronto
    let _ = nexo_sys::handle_close(b2);
    match nexo_sys::channel_wait_any(&[a1, a2]) {
        Ok(1) => {}
        r => {
            nexo_rt::log!("utest: wait_any par-fechado devolveu {:?}", r);
            nexo_sys::exit(406)
        }
    }
    // 4. erros: array vazio e handle que nao e canal
    if nexo_sys::channel_wait_any(&[]) != Err(Status::InvalidArgs) {
        nexo_sys::exit(407);
    }
    if nexo_sys::channel_wait_any(&[9999]) != Err(Status::BadHandle) {
        nexo_sys::exit(408);
    }
    nexo_sys::log("utest: wait_any ok");
    nexo_sys::exit(0)
}

/// Modo 17: produtor de memoria compartilhada (handle 0 = canal). Cria um objeto de memoria,
/// mapeia, escreve um marcador e envia o handle do objeto pelo canal; espera a resposta do
/// consumidor na propria memoria compartilhada.
fn shmem_producer() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mem = nexo_sys::memory_create(1).unwrap_or_else(|_| nexo_sys::exit(410));
    let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| nexo_sys::exit(411));
    let marker = b"SHMEM-OK";
    // SAFETY: base .. base+4096 foi mapeada por memory_map (USER|RW) neste processo.
    unsafe {
        core::ptr::copy_nonoverlapping(marker.as_ptr(), base as *mut u8, marker.len());
    }
    // transfere o handle do objeto ao consumidor
    if nexo_sys::channel_send(ch, b"mem", &[mem]) != Status::Ok {
        nexo_sys::exit(412);
    }
    // espera a resposta escrita pelo consumidor no offset 64
    let start = nexo_sys::time_now();
    loop {
        let mut reply = [0u8; 5];
        // SAFETY: leitura da mesma pagina mapeada.
        unsafe {
            core::ptr::copy_nonoverlapping((base + 64) as *const u8, reply.as_mut_ptr(), 5);
        }
        if &reply == b"REPLY" {
            nexo_sys::log(
                "utest: shmem produtor ok — consumidor escreveu na memoria compartilhada",
            );
            nexo_sys::exit(0)
        }
        if nexo_sys::time_now() - start > 10_000_000_000 {
            nexo_sys::exit(413)
        }
        nexo_sys::sleep_ns(5_000_000);
    }
}

/// Modo 19: cliente do compositor `wm` (handle 0 = canal `nexo.wm`). Cria duas superficies
/// sobrepostas com cores distintas em memoria compartilhada, faz commit e le a saida composta
/// para conferir a ordem-Z — prova de composicao fim a fim entre processos.
fn wm_client() -> ! {
    let ch: nexo_sys::Handle = 0;
    // A: vermelha em (0,0) 8x8, z=0
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0);
    wm_commit(ch, a);
    // B: verde em (4,4) 8x8, z=1 (sobrepoe A no canto)
    let (b, b_base) = wm_create(ch, 4, 4, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0);
    wm_commit(ch, b);

    // le a saida composta
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(440));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(441);
    }
    let (n, nh) = match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok(v) => v,
        _ => nexo_sys::exit(442),
    };
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(443));
    if nh != 1 {
        nexo_sys::exit(444);
    }
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(445));
    let stride = outp.w;
    // (2,2): so A -> vermelho; (6,6): sobreposicao, B em cima -> verde;
    // (10,10): so B -> verde; (30,30): fundo -> preto.
    if wm_px(ob, stride, 2, 2) != (255, 0, 0) {
        nexo_sys::exit(450);
    }
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(451);
    }
    if wm_px(ob, stride, 10, 10) != (0, 255, 0) {
        nexo_sys::exit(452);
    }
    if wm_px(ob, stride, 30, 30) != (0, 0, 0) {
        nexo_sys::exit(453);
    }
    nexo_sys::log("utest: wm cliente ok — composicao Z de duas superficies conferida na saida");
    nexo_sys::exit(0)
}

/// Modo 20: multi-cliente do compositor. A sessao 1 (handle 0) cria a superficie vermelha; abre
/// uma 2a sessao com `open` (transferindo a ponta de um canal novo) e cria a verde nela. Confere
/// a composicao Z das superficies de sessoes independentes e o isolamento (uma sessao nao mexe na
/// superficie da outra).
fn wm_multi_client() -> ! {
    let s1: nexo_sys::Handle = 0;
    // Sessao 1: A vermelha em (0,0) 8x8, z=0
    let (a, a_base) = wm_create(s1, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0);
    wm_commit(s1, a);

    // Abre a sessao 2: cria um canal e transfere uma ponta ao wm via `open`.
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(460));
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(461));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(462);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(463),
    }
    let s2 = mine;

    // Sessao 2: B verde em (4,4) 8x8, z=1 (sobrepoe A no canto)
    let (b, b_base) = wm_create(s2, 4, 4, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0);
    wm_commit(s2, b);

    // Isolamento: a sessao 1 nao pode dar commit na superficie da sessao 2 (id `b`).
    let m = nexo_proto::wm::CommitRequest { id: b }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(464));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(465);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        // esperado: erro remoto (superficie de outra sessao) -> decode falha
        Ok((n, _)) if nexo_proto::wm::decode_commit_response(&buf[..n]).is_err() => {}
        _ => nexo_sys::exit(466),
    }

    // Le a saida composta (na sessao 1) e confere a ordem-Z das duas sessoes.
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(467));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(468);
    }
    let (n, nh) = match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok(v) => v,
        _ => nexo_sys::exit(469),
    };
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(470));
    if nh != 1 {
        nexo_sys::exit(471);
    }
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(472));
    let stride = outp.w;
    if wm_px(ob, stride, 2, 2) != (255, 0, 0) {
        nexo_sys::exit(473);
    }
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(474);
    }
    if wm_px(ob, stride, 10, 10) != (0, 255, 0) {
        nexo_sys::exit(475);
    }
    if wm_px(ob, stride, 30, 30) != (0, 0, 0) {
        nexo_sys::exit(476);
    }
    nexo_sys::log("utest: wm multi-cliente ok — duas sessoes compoem e o isolamento vale");
    nexo_sys::exit(0)
}

/// Modo 21: restacking de janelas. Duas superficies sobrepostas; alterna quem fica na frente com
/// `raise`/`lower` e confere na saida composta que o pixel da sobreposicao muda de cor conforme o z.
fn wm_restack() -> ! {
    let ch: nexo_sys::Handle = 0;
    // A vermelha em (0,0), B verde em (4,4); overlap em (6,6). B comeca por cima (z=1).
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0);
    wm_commit(ch, a);
    let (b, b_base) = wm_create(ch, 4, 4, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0);
    wm_commit(ch, b);

    // Mapeia a saida uma vez (as paginas sao estaveis; cada recomposicao as reescreve).
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(480));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(481);
    }
    let (n, nh) = match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok(v) => v,
        _ => nexo_sys::exit(482),
    };
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(483));
    if nh != 1 {
        nexo_sys::exit(484);
    }
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(485));
    let stride = outp.w;

    // B por cima -> verde
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(490);
    }
    // envia B para tras -> A (vermelho) por cima
    wm_restack_op(ch, b, false);
    if wm_px(ob, stride, 6, 6) != (255, 0, 0) {
        nexo_sys::exit(491);
    }
    // traz B para frente -> verde de novo
    wm_restack_op(ch, b, true);
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(492);
    }
    // traz A para frente -> vermelho
    wm_restack_op(ch, a, true);
    if wm_px(ob, stride, 6, 6) != (255, 0, 0) {
        nexo_sys::exit(493);
    }
    nexo_sys::log("utest: wm restack ok — raise/lower reordenam o z e a saida acompanha");
    nexo_sys::exit(0)
}

/// Modo 23: foco por clique. Cria duas superficies sobrepostas, registra uma fonte de entrada
/// (canal de eventos evdev) e injeta cliques: a superficie sob o ponteiro vem para a frente, o que
/// e observavel na saida composta (o pixel da sobreposicao muda de cor). Usa polling porque a
/// entrada e assincrona (o wm processa o evento no seu proprio laco).
fn wm_input() -> ! {
    let ch: nexo_sys::Handle = 0;
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0); // A vermelha
    wm_commit(ch, a);
    let (b, b_base) = wm_create(ch, 4, 4, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0); // B verde (por cima)
    wm_commit(ch, b);

    // Mapeia a saida.
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(530));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(531);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(532));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(533));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(534));
    let stride = outp.w;
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(535); // B por cima no inicio
    }

    // Registra a fonte de entrada: transfere a ponta de leitura de um canal novo ao wm.
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(536));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(537));
    if nexo_sys::channel_send(ch, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(538);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(539),
    }

    // Clique sobre A (2,2): A vem para a frente -> (6,6) fica vermelho.
    wm_click(inj, 2, 2);
    wm_wait_px(ob, stride, 6, 6, (255, 0, 0), 540);
    // Clique sobre B (10,10): B vem para a frente -> (6,6) fica verde.
    wm_click(inj, 10, 10);
    wm_wait_px(ob, stride, 6, 6, (0, 255, 0), 541);
    // Clique sobre A de novo (2,2): -> vermelho.
    wm_click(inj, 2, 2);
    wm_wait_px(ob, stride, 6, 6, (255, 0, 0), 542);

    nexo_sys::log("utest: wm input ok — clique traz a janela sob o ponteiro para a frente");
    nexo_sys::exit(0)
}

/// Modo 45: driver da calculadora — o primeiro app real. Handle 0 = sessao nexo.wm (bootstrap),
/// handle 1 = canal com a calc. Entrega a sessao a calc, clica nos botoes "1 + 2 =" (eventos
/// pointer nas coordenadas dos botoes) e confere o resultado "3" pelo clipboard mediado.
fn calc_driver() -> ! {
    let s1: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 384];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    // janela do driver (fica com o foco inicial), longe da calc
    let (_w, w_base) = wm_create(s1, 48, 0, 8, 8, 0);
    wm_fill(w_base, 8, 8, 128, 128, 128);
    wm_commit(s1, _w);

    // sessao para a calc
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1170));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1171));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1172);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1173),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(1174);
    }

    // entrada sintetica
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1175));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1176));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1177);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1178),
    }

    // espera a calc criar a janela: da um tempo curto e clica; a FIFO do canal de entrada
    // garante a ordem entre os cliques, e o "eq" no pipe sincroniza o fim.
    nexo_sys::sleep_ns(300_000_000);
    // botoes (janela da calc em (8,8); botao k no centro local (4+k*8, 18)):
    for k in [0i32, 1, 2, 3] {
        wm_click(inj, 12 + k * 8, 26);
        nexo_sys::sleep_ns(30_000_000); // deixa a calc repintar entre cliques
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"eq" => {}
        _ => nexo_sys::exit(1179),
    }

    // foco de volta ao driver e le o resultado pelo clipboard mediado
    wm_click(inj, 52, 4);
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_pointer_event(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1180),
    }
    let m = nexo_proto::wm::ClipboardGetRequest {}
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1181));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1182);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1183));
    let r = nexo_proto::wm::decode_clipboard_get_response(&buf[..n])
        .unwrap_or_else(|_| nexo_sys::exit(1184));
    if r.data() != b"3" {
        nexo_sys::exit(1185);
    }
    nexo_sys::log("utest: calc ok — 1 + 2 = 3, clicado por eventos pointer e lido pelo clipboard");
    nexo_sys::exit(0)
}

/// Modo 44: driver da Central de Acoes. Publica dois avisos, clica na zona direita da barra (o
/// shell abre o painel com um bullet por notificacao — conferido por pixel) e clica de novo (o
/// painel some).
fn shellcenter_driver() -> ! {
    let pipe: nexo_sys::Handle = 0;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    if nexo_sys::channel_send(pipe, b"sess", &[]) != Status::Ok {
        nexo_sys::exit(1140);
    }
    let s = match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => nexo_sys::exit(1141),
    };

    // dois avisos no registro
    for txt in [b"alpha".as_slice(), b"beta".as_slice()] {
        let mut rq = nexo_proto::wm::NotifyRequest {
            title: [0; 64],
            title_len: txt.len() as u32,
        };
        rq.title[..txt.len()].copy_from_slice(txt);
        let m = rq
            .encode_msg(&mut out)
            .unwrap_or_else(|_| nexo_sys::exit(1142));
        if nexo_sys::channel_send(s, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(1143);
        }
        match nexo_sys::channel_recv(s, &mut buf, &mut hs) {
            Ok((n, _)) if nexo_proto::wm::decode_notify_response(&buf[..n]).is_ok() => {}
            _ => nexo_sys::exit(1144),
        }
    }

    // entrada sintetica e o clique na zona direita da barra
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1145));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1146));
    if nexo_sys::channel_send(s, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1147);
    }
    match nexo_sys::channel_recv(s, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1148),
    }
    wm_click(inj, 56, 42);
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"copen" => {}
        _ => nexo_sys::exit(1149),
    }

    // painel visivel: fundo, bullets das 2 notificacoes, 3a linha vazia
    // a saida composta e privilegio do shell: o driver pede ao shellui pelo pipe
    if nexo_sys::channel_send(pipe, b"saida", &[]) != Status::Ok {
        nexo_sys::exit(1151);
    }
    let (n, nh) =
        nexo_sys::channel_recv(pipe, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1152));
    if nh != 1 {
        nexo_sys::exit(1150);
    }
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1153));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1154));
    let stride = outp.w;
    if wm_px(ob, stride, 18, 10) != (0x1e, 0x1f, 0x24) {
        nexo_sys::exit(1155); // fundo do painel
    }
    if wm_px(ob, stride, 20, 12) != (0x6f, 0x9f, 0xff) {
        nexo_sys::exit(1156); // bullet da notificacao mais recente
    }
    if wm_px(ob, stride, 20, 20) != (0x6f, 0x9f, 0xff) {
        nexo_sys::exit(1157); // bullet da segunda
    }
    if wm_px(ob, stride, 20, 28) != (0x1e, 0x1f, 0x24) {
        nexo_sys::exit(1158); // 3a linha vazia (fundo)
    }

    // fecha o painel
    wm_click(inj, 56, 42);
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"cclosed" => {}
        _ => nexo_sys::exit(1159),
    }
    if wm_px(ob, stride, 18, 10) != (0, 0, 0) {
        nexo_sys::exit(1160); // sem painel, ali e fundo da cena
    }
    nexo_sys::log("utest: central ok — painel abre com um bullet por aviso e fecha no 2o clique");
    nexo_sys::exit(0)
}

/// Modo 43: driver da Faixa de Atividades. Handle 0 = canal com o shellui. Pede uma sessao ao
/// shell (broker), cria uma janela com titulo, pede o sync da barra e confere os pixels da
/// celula; clica na celula (via entrada sintetica) e confere que o shell ATIVOU a janela (a
/// tecla seguinte chega a ela).
fn shellui_driver() -> ! {
    let pipe: nexo_sys::Handle = 0;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    // sessao nexo.wm via broker do shell
    if nexo_sys::channel_send(pipe, b"sess", &[]) != Status::Ok {
        nexo_sys::exit(1110);
    }
    let s = match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => nexo_sys::exit(1111),
    };

    // janela do app, com titulo
    let (app, app_base) = wm_create(s, 0, 0, 8, 8, 0);
    wm_fill(app_base, 8, 8, 255, 255, 0); // amarela
    wm_commit(s, app);
    let mut rq = nexo_proto::wm::SetTitleRequest {
        id: app,
        title: [0; 32],
        title_len: 4,
    };
    rq.title[..4].copy_from_slice(b"app1");
    let m = rq
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1112));
    if nexo_sys::channel_send(s, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1113);
    }
    match nexo_sys::channel_recv(s, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_title_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1114),
    }

    // sync da barra
    if nexo_sys::channel_send(pipe, b"sync", &[]) != Status::Ok {
        nexo_sys::exit(1115);
    }
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"ok" => {}
        _ => nexo_sys::exit(1116),
    }

    // pixels da barra: fundo (tema escuro, surface) e a celula 0 (acento)
    // a saida composta e privilegio do shell: o driver pede ao shellui pelo pipe
    if nexo_sys::channel_send(pipe, b"saida", &[]) != Status::Ok {
        nexo_sys::exit(1118);
    }
    let (n, nh) =
        nexo_sys::channel_recv(pipe, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1119));
    if nh != 1 {
        nexo_sys::exit(1117);
    }
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1120));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1121));
    let stride = outp.w;
    if wm_px(ob, stride, 1, 39) != (0x1e, 0x1f, 0x24) {
        nexo_sys::exit(1122); // fundo da barra
    }
    if wm_px(ob, stride, 4, 42) != (0x6f, 0x9f, 0xff) {
        nexo_sys::exit(1123); // celula da app1
    }

    // clica na celula 0: o shell recebe o pointer e ativa a app1
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1124));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1125));
    if nexo_sys::channel_send(s, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(1126);
    }
    match nexo_sys::channel_recv(s, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1127),
    }
    wm_click(inj, 4, 42);
    match nexo_sys::channel_recv(pipe, &mut buf, &mut hs) {
        Ok((n, _)) if &buf[..n] == b"activated" => {}
        _ => nexo_sys::exit(1128),
    }
    // a app1 esta focada: a tecla chega a ela
    wm_key(inj, 30, 1);
    let ev = wm_recv_key(s, &mut buf, &mut hs);
    if ev.surface != app || ev.code != 30 {
        nexo_sys::exit(1129);
    }
    nexo_sys::log("utest: shellui ok — barra desenhada; clique na celula ativou a janela");
    nexo_sys::exit(0)
}

/// Modo 42: mecanismo da Central de Acoes. O registro guarda as notificacoes recentes — inclusive
/// as suprimidas pelo DND (que corta so a interrupcao); o shell lista (0 = mais recente) e limpa;
/// sessao comum e negada.
fn wm_center() -> ! {
    let s1: nexo_sys::Handle = 0;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    let (_a, a_base) = wm_create(s1, 0, 0, 8, 8, 0); // foco (para o set_dnd)
    wm_fill(a_base, 8, 8, 0, 0, 255);
    let notify = |ch: nexo_sys::Handle,
                  txt: &[u8],
                  out: &mut [u8; 256],
                  buf: &mut [u8; 256],
                  hs: &mut [u32; 1]| {
        let mut rq = nexo_proto::wm::NotifyRequest {
            title: [0; 64],
            title_len: txt.len() as u32,
        };
        rq.title[..txt.len()].copy_from_slice(txt);
        let m = rq.encode_msg(out).unwrap_or_else(|_| nexo_sys::exit(1080));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(1081);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) if nexo_proto::wm::decode_notify_response(&buf[..n]).is_ok() => {}
            _ => nexo_sys::exit(1082),
        }
    };
    let ninfo = |ch: nexo_sys::Handle,
                 idx: u32,
                 out: &mut [u8; 256],
                 buf: &mut [u8; 256],
                 hs: &mut [u32; 1]|
     -> Option<nexo_proto::wm::NotificationInfoResponse> {
        let m = nexo_proto::wm::NotificationInfoRequest { index: idx }
            .encode_msg(out)
            .unwrap_or_else(|_| nexo_sys::exit(1083));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(1084);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_notification_info_response(&buf[..n]).ok(),
            _ => nexo_sys::exit(1085),
        }
    };

    notify(s1, b"a", &mut out, &mut buf, &mut hs);
    // liga o DND e publica "b": o banner e suprimido, mas a Central registra
    let m = nexo_proto::wm::SetDndRequest { enabled: 1 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1086));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1087);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_dnd_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1088),
    }
    notify(s1, b"b", &mut out, &mut buf, &mut hs);

    let i0 = ninfo(s1, 0, &mut out, &mut buf, &mut hs).unwrap_or_else(|| nexo_sys::exit(1089));
    if i0.used != 1 || i0.title() != b"b" {
        nexo_sys::exit(1090);
    }
    let i1 = ninfo(s1, 1, &mut out, &mut buf, &mut hs).unwrap_or_else(|| nexo_sys::exit(1091));
    if i1.used != 1 || i1.title() != b"a" {
        nexo_sys::exit(1092);
    }
    let i2 = ninfo(s1, 2, &mut out, &mut buf, &mut hs).unwrap_or_else(|| nexo_sys::exit(1093));
    if i2.used != 0 {
        nexo_sys::exit(1094);
    }

    // limpa
    let m = nexo_proto::wm::NotificationsClearRequest {}
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1095));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1096);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_notifications_clear_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1097),
    }
    let i0 = ninfo(s1, 0, &mut out, &mut buf, &mut hs).unwrap_or_else(|| nexo_sys::exit(1098));
    if i0.used != 0 {
        nexo_sys::exit(1099);
    }

    // sessao comum: negada
    let (s2, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1100));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1101));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1102);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1103),
    }
    if ninfo(s2, 0, &mut out, &mut buf, &mut hs).is_some() {
        nexo_sys::exit(1104);
    }
    nexo_sys::log(
        "utest: wm central ok — registro guarda ate sob DND; shell lista/limpa; comum negada",
    );
    nexo_sys::exit(0)
}

/// Modo 41: escala fracionaria + reducao de movimento. set_scale muda so o retangulo de exibicao
/// (a composicao escala); a preferencia de reducao de movimento e mediada para escrita e livre
/// para leitura.
fn wm_scale() -> ! {
    let s1: nexo_sys::Handle = 0;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    let (a, a_base) = wm_create(s1, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 255); // magenta
    wm_commit(s1, a);
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1040));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1041);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1042));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1043));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1044));
    let stride = outp.w;
    if wm_px(ob, stride, 4, 4) != (255, 0, 255) || wm_px(ob, stride, 12, 12) != (0, 0, 0) {
        nexo_sys::exit(1045);
    }

    let scale = |ch: nexo_sys::Handle,
                 id: u32,
                 num: u32,
                 den: u32,
                 out: &mut [u8; 256],
                 buf: &mut [u8; 256],
                 hs: &mut [u32; 1]|
     -> bool {
        let m = nexo_proto::wm::SetScaleRequest { id, num, den }
            .encode_msg(out)
            .unwrap_or_else(|_| nexo_sys::exit(1046));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(1047);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_set_scale_response(&buf[..n]).is_ok(),
            _ => nexo_sys::exit(1048),
        }
    };
    // 200%%: 8x8 exibida em 16x16
    if !scale(s1, a, 2, 1, &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(1049);
    }
    if wm_px(ob, stride, 12, 12) != (255, 0, 255) {
        nexo_sys::exit(1050);
    }
    // 150%%: 12x12
    if !scale(s1, a, 3, 2, &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(1051);
    }
    if wm_px(ob, stride, 10, 10) != (255, 0, 255) || wm_px(ob, stride, 14, 14) != (0, 0, 0) {
        nexo_sys::exit(1052);
    }
    // invalido
    if scale(s1, a, 0, 1, &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(1053);
    }

    // reducao de movimento: escrita mediada, leitura livre
    let prefs =
        |ch: nexo_sys::Handle, out: &mut [u8; 256], buf: &mut [u8; 256], hs: &mut [u32; 1]| -> u8 {
            let m = nexo_proto::wm::PrefsRequest {}
                .encode_msg(out)
                .unwrap_or_else(|_| nexo_sys::exit(1054));
            if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
                nexo_sys::exit(1055);
            }
            match nexo_sys::channel_recv(ch, buf, hs) {
                Ok((n, _)) => {
                    nexo_proto::wm::decode_prefs_response(&buf[..n])
                        .unwrap_or_else(|_| nexo_sys::exit(1056))
                        .reduce_motion
                }
                _ => nexo_sys::exit(1057),
            }
        };
    let set_rm = |ch: nexo_sys::Handle,
                  on: u8,
                  out: &mut [u8; 256],
                  buf: &mut [u8; 256],
                  hs: &mut [u32; 1]|
     -> bool {
        let m = nexo_proto::wm::SetReduceMotionRequest { enabled: on }
            .encode_msg(out)
            .unwrap_or_else(|_| nexo_sys::exit(1058));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(1059);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_set_reduce_motion_response(&buf[..n]).is_ok(),
            _ => nexo_sys::exit(1060),
        }
    };
    if prefs(s1, &mut out, &mut buf, &mut hs) != 0 {
        nexo_sys::exit(1061);
    }
    if !set_rm(s1, 1, &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(1062);
    }
    if prefs(s1, &mut out, &mut buf, &mut hs) != 1 {
        nexo_sys::exit(1063);
    }
    // sessao em segundo plano: nao muda, mas le
    let (s2, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1064));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1065));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1066);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1067),
    }
    if set_rm(s2, 0, &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(1068);
    }
    if prefs(s2, &mut out, &mut buf, &mut hs) != 1 {
        nexo_sys::exit(1069);
    }
    nexo_sys::log(
        "utest: wm scale ok — escala fracionaria por composicao; reducao de movimento mediada",
    );
    nexo_sys::exit(0)
}

/// Modo 40: mecanismo da Faixa de Atividades. A sessao bootstrap (shell) enumera as janelas
/// (surface_info: id/contexto/titulo) e ativa qualquer uma (activate: troca de contexto, traz a
/// frente, foca); sessoes comuns sao negadas (erro 7).
fn wm_shell() -> ! {
    let s1: nexo_sys::Handle = 0; // sessao bootstrap = shell
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    let (a, a_base) = wm_create(s1, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0);
    wm_commit(s1, a);
    let (b, b_base) = wm_create(s1, 0, 0, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0);
    wm_commit(s1, b);
    let set_title = |ch: nexo_sys::Handle,
                     id: u32,
                     t: &[u8],
                     out: &mut [u8; 256],
                     buf: &mut [u8; 256],
                     hs: &mut [u32; 1]| {
        let mut rq = nexo_proto::wm::SetTitleRequest {
            id,
            title: [0; 32],
            title_len: t.len() as u32,
        };
        rq.title[..t.len()].copy_from_slice(t);
        let m = rq.encode_msg(out).unwrap_or_else(|_| nexo_sys::exit(1000));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(1001);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) if nexo_proto::wm::decode_set_title_response(&buf[..n]).is_ok() => {}
            _ => nexo_sys::exit(1002),
        }
    };
    set_title(s1, a, b"editor", &mut out, &mut buf, &mut hs);
    set_title(s1, b, b"chat", &mut out, &mut buf, &mut hs);
    // B vai para o contexto 1
    let m = nexo_proto::wm::SetContextRequest { id: b, context: 1 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1003));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1004);
    }
    if nexo_sys::channel_recv(s1, &mut buf, &mut hs).is_err() {
        nexo_sys::exit(1005);
    }

    // enumeracao pelo shell: acha "editor" (ctx 0) e "chat" (ctx 1)
    let info = |ch: nexo_sys::Handle,
                idx: u32,
                out: &mut [u8; 256],
                buf: &mut [u8; 256],
                hs: &mut [u32; 1]|
     -> Option<nexo_proto::wm::SurfaceInfoResponse> {
        let m = nexo_proto::wm::SurfaceInfoRequest { index: idx }
            .encode_msg(out)
            .unwrap_or_else(|_| nexo_sys::exit(1006));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(1007);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_surface_info_response(&buf[..n]).ok(),
            _ => nexo_sys::exit(1008),
        }
    };
    let ia = info(s1, a, &mut out, &mut buf, &mut hs).unwrap_or_else(|| nexo_sys::exit(1009));
    if ia.used != 1 || ia.context != 0 || ia.title() != b"editor" {
        nexo_sys::exit(1010);
    }
    let ib = info(s1, b, &mut out, &mut buf, &mut hs).unwrap_or_else(|| nexo_sys::exit(1011));
    if ib.used != 1 || ib.context != 1 || ib.title() != b"chat" {
        nexo_sys::exit(1012);
    }

    // saida: ctx 0 ativo mostra A
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1013));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1014);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(1015));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(1016));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(1017));
    let stride = outp.w;
    if wm_px(ob, stride, 4, 4) != (255, 0, 0) {
        nexo_sys::exit(1018);
    }

    // activate(B): troca para o ctx 1, traz B a frente e foca — o clique da Faixa.
    let m = nexo_proto::wm::ActivateRequest { id: b }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1019));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1020);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_activate_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1021),
    }
    if wm_px(ob, stride, 4, 4) != (0, 255, 0) {
        nexo_sys::exit(1022);
    }

    // sessao comum: negada (erro 7)
    let (s2, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(1023));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1024));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(1025);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(1026),
    }
    if info(s2, 0, &mut out, &mut buf, &mut hs).is_some() {
        nexo_sys::exit(1027);
    }
    let m = nexo_proto::wm::ActivateRequest { id: a }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1028));
    if nexo_sys::channel_send(s2, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1029);
    }
    match nexo_sys::channel_recv(s2, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_activate_response(&buf[..n]).is_err() => {}
        _ => nexo_sys::exit(1030),
    }
    // e a saida composta (a tela inteira) tambem e privilegio do shell
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(1031));
    if nexo_sys::channel_send(s2, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(1032);
    }
    match nexo_sys::channel_recv(s2, &mut buf, &mut hs) {
        Ok((n, 0)) if nexo_proto::wm::decode_output_response(&buf[..n]).is_err() => {}
        _ => nexo_sys::exit(1033),
    }
    nexo_sys::log(
        "utest: wm shell ok — shell enumera e ativa janelas; sessao comum e negada (info e saida)",
    );
    nexo_sys::exit(0)
}

/// Modo 39: arquitetura de leitor de tela. Assina o fluxo de eventos semanticos do compositor e
/// confere que mudancas de foco (com o titulo da janela), avisos e trocas de contexto chegam como
/// eventos `a11y` na ordem.
fn wm_a11y() -> ! {
    let s1: nexo_sys::Handle = 0;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    let (a, a_base) = wm_create(s1, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0);
    wm_commit(s1, a);
    let (b, b_base) = wm_create(s1, 16, 0, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0);
    wm_commit(s1, b);

    let set_title = |ch: nexo_sys::Handle,
                     id: u32,
                     t: &[u8],
                     out: &mut [u8; 256],
                     buf: &mut [u8; 256],
                     hs: &mut [u32; 1]| {
        let mut rq = nexo_proto::wm::SetTitleRequest {
            id,
            title: [0; 32],
            title_len: t.len() as u32,
        };
        rq.title[..t.len()].copy_from_slice(t);
        let m = rq.encode_msg(out).unwrap_or_else(|_| nexo_sys::exit(970));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(971);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) if nexo_proto::wm::decode_set_title_response(&buf[..n]).is_ok() => {}
            _ => nexo_sys::exit(972),
        }
    };
    set_title(s1, a, b"editor", &mut out, &mut buf, &mut hs);
    set_title(s1, b, b"chat", &mut out, &mut buf, &mut hs);

    // assina o fluxo de acessibilidade
    let (reader, sub) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(973));
    let m = nexo_proto::wm::A11ySubscribeRequest { chan: sub }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(974));
    if nexo_sys::channel_send(s1, &out[..m], &[sub]) != Status::Ok {
        nexo_sys::exit(975);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_a11y_subscribe_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(976),
    }

    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(977));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(978));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(979);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(980),
    }

    let recv_a11y = |reader: nexo_sys::Handle,
                     buf: &mut [u8; 256],
                     hs: &mut [u32; 1]|
     -> nexo_proto::wm::A11yEvent {
        let n = match nexo_sys::channel_recv(reader, buf, hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(981),
        };
        nexo_proto::wm::decode_a11y_event(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(982))
    };

    // drena o evento `pointer` que o clique entrega a sessao dona da janela
    let drain_pointer = |ch: nexo_sys::Handle, buf: &mut [u8; 256], hs: &mut [u32; 1]| {
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) if nexo_proto::wm::decode_pointer_event(&buf[..n]).is_ok() => {}
            _ => nexo_sys::exit(996),
        }
    };
    // foco por clique -> evento com o titulo
    wm_click(inj, 2, 2);
    let ev = recv_a11y(reader, &mut buf, &mut hs);
    if ev.kind != 1 || ev.surface != a || ev.text() != b"editor" {
        nexo_sys::exit(983);
    }
    drain_pointer(s1, &mut buf, &mut hs);
    wm_click(inj, 18, 2);
    let ev = recv_a11y(reader, &mut buf, &mut hs);
    if ev.kind != 1 || ev.surface != b || ev.text() != b"chat" {
        nexo_sys::exit(984);
    }
    drain_pointer(s1, &mut buf, &mut hs);

    // aviso -> evento de notificacao
    let mut rq = nexo_proto::wm::NotifyRequest {
        title: [0; 64],
        title_len: 2,
    };
    rq.title[..2].copy_from_slice(b"oi");
    let m = rq
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(985));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(986);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_notify_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(987),
    }
    let ev = recv_a11y(reader, &mut buf, &mut hs);
    if ev.kind != 2 || ev.text() != b"oi" {
        nexo_sys::exit(988);
    }

    // troca de contexto -> evento
    let m = nexo_proto::wm::SwitchContextRequest { context: 1 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(989));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(990);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_switch_context_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(991),
    }
    let ev = recv_a11y(reader, &mut buf, &mut hs);
    if ev.kind != 3 || ev.surface != 1 {
        nexo_sys::exit(992);
    }
    nexo_sys::log(
        "utest: wm a11y ok — foco/aviso/contexto chegam como eventos semanticos ao leitor",
    );
    nexo_sys::exit(0)
}

/// Modo 38: drag-and-drop por grant. A sessao dona da entrada inicia o arrasto; soltar sobre a
/// janela da outra sessao entrega os dados SO a ela (evento drop); soltar no vazio descarta; quem
/// nao detem a entrada nao inicia arrasto.
fn wm_dnd() -> ! {
    let s1: nexo_sys::Handle = 0;
    let mut out = [0u8; 384];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    let (_a, a_base) = wm_create(s1, 0, 0, 8, 8, 0); // foco: A (s1)
    wm_fill(a_base, 8, 8, 255, 0, 0);
    let (s2, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(940));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(941));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(942);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(943),
    }
    let req = nexo_proto::wm::CreateSurfaceRequest {
        x: 16,
        y: 0,
        w: 8,
        h: 8,
        z: 1,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(944));
    if nexo_sys::channel_send(s2, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(945);
    }
    let (n, nh) =
        nexo_sys::channel_recv(s2, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(946));
    let bs = nexo_proto::wm::decode_create_surface_response(&buf[..n])
        .unwrap_or_else(|_| nexo_sys::exit(947));
    if nh != 1 {
        nexo_sys::exit(948);
    }
    let b = bs.id;

    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(949));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(950));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(951);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(952),
    }

    let drag = |ch: nexo_sys::Handle,
                txt: &[u8],
                out: &mut [u8; 384],
                buf: &mut [u8; 384],
                hs: &mut [u32; 1]|
     -> bool {
        let mut rq = nexo_proto::wm::DragStartRequest {
            data: [0; 256],
            data_len: txt.len() as u32,
        };
        rq.data[..txt.len()].copy_from_slice(txt);
        let m = rq.encode_msg(out).unwrap_or_else(|_| nexo_sys::exit(953));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(954);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_drag_start_response(&buf[..n]).is_ok(),
            _ => nexo_sys::exit(955),
        }
    };

    // s2 (sem a entrada) nao inicia arrasto
    if drag(s2, b"spy", &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(956);
    }
    // s1 arrasta "doc" e solta sobre B (release em (18,2))
    if !drag(s1, b"doc", &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(957);
    }
    wm_click_at(inj, 18, 2, 0); // ABS + release
    let n = match nexo_sys::channel_recv(s2, &mut buf, &mut hs) {
        Ok((n, _)) => n,
        _ => nexo_sys::exit(958),
    };
    match nexo_proto::wm::decode_drop_event(&buf[..n]) {
        Ok(ev) if ev.surface == b && ev.data() == b"doc" => {}
        _ => nexo_sys::exit(959),
    }

    // soltar no vazio descarta: nada chega a s2 (conferido apos sincronizar pela tecla em s1)
    if !drag(s1, b"nada", &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(960);
    }
    wm_click_at(inj, 40, 40, 0);
    wm_key(inj, 30, 1);
    let ev = wm_recv_key(s1, &mut buf, &mut hs);
    if ev.code != 30 {
        nexo_sys::exit(961);
    }
    match nexo_sys::channel_try_recv(s2, &mut buf, &mut hs) {
        Err(Status::WouldBlock) => {}
        _ => nexo_sys::exit(962),
    }
    nexo_sys::log("utest: wm dnd ok — o drop entrega os dados so a janela alvo; no vazio descarta");
    nexo_sys::exit(0)
}

/// Injeta ABS X/Y + BTN_LEFT com `value` (1 = press, 0 = release) num canal de entrada.
fn wm_click_at(inj: nexo_sys::Handle, x: i32, y: i32, value: u32) {
    let mut ev = [0u8; 24];
    let put = |ev: &mut [u8], off: usize, ty: u16, code: u16, v: u32| {
        ev[off..off + 2].copy_from_slice(&ty.to_le_bytes());
        ev[off + 2..off + 4].copy_from_slice(&code.to_le_bytes());
        ev[off + 4..off + 8].copy_from_slice(&v.to_le_bytes());
    };
    put(&mut ev, 0, 3, 0, x as u32);
    put(&mut ev, 8, 3, 1, y as u32);
    put(&mut ev, 16, 1, 0x110, value);
    if nexo_sys::channel_send(inj, &ev, &[]) != Status::Ok {
        nexo_sys::exit(963);
    }
}

/// Modo 37: notificacoes + nao-perturbe. Um aviso (inclusive de sessao em segundo plano) desenha
/// o banner de sobreposicao; dismiss o remove; com DND ativo o aviso e descartado; o set_dnd e
/// mediado pela posse da entrada.
fn wm_notify() -> ! {
    let s1: nexo_sys::Handle = 0;
    let mut out = [0u8; 256];
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];

    // Janela azul cobrindo a tela toda (fica atras do banner).
    let (a, a_base) = wm_create(s1, 0, 0, 64, 48, 0);
    wm_fill(a_base, 64, 48, 0, 0, 255);
    wm_commit(s1, a);
    let _ = a;

    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(900));
    if nexo_sys::channel_send(s1, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(901);
    }
    let (n, _) =
        nexo_sys::channel_recv(s1, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(902));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(903));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(904));
    let stride = outp.w;
    // regiao do banner (topo direito): comeca azul
    if wm_px(ob, stride, 60, 4) != (0, 0, 255) {
        nexo_sys::exit(905);
    }

    let notify = |ch: nexo_sys::Handle,
                  txt: &[u8],
                  out: &mut [u8; 256],
                  buf: &mut [u8; 256],
                  hs: &mut [u32; 1]| {
        let mut rq = nexo_proto::wm::NotifyRequest {
            title: [0; 64],
            title_len: txt.len() as u32,
        };
        rq.title[..txt.len()].copy_from_slice(txt);
        let m = rq.encode_msg(out).unwrap_or_else(|_| nexo_sys::exit(906));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(907);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) if nexo_proto::wm::decode_notify_response(&buf[..n]).is_ok() => {}
            _ => nexo_sys::exit(908),
        }
    };
    let dismiss =
        |ch: nexo_sys::Handle, out: &mut [u8; 256], buf: &mut [u8; 256], hs: &mut [u32; 1]| {
            let m = nexo_proto::wm::DismissNotificationRequest {}
                .encode_msg(out)
                .unwrap_or_else(|_| nexo_sys::exit(909));
            if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
                nexo_sys::exit(910);
            }
            match nexo_sys::channel_recv(ch, buf, hs) {
                Ok((n, _))
                    if nexo_proto::wm::decode_dismiss_notification_response(&buf[..n]).is_ok() => {}
                _ => nexo_sys::exit(911),
            }
        };
    let set_dnd = |ch: nexo_sys::Handle,
                   on: u8,
                   out: &mut [u8; 256],
                   buf: &mut [u8; 256],
                   hs: &mut [u32; 1]|
     -> bool {
        let m = nexo_proto::wm::SetDndRequest { enabled: on }
            .encode_msg(out)
            .unwrap_or_else(|_| nexo_sys::exit(912));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(913);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_set_dnd_response(&buf[..n]).is_ok(),
            _ => nexo_sys::exit(914),
        }
    };

    // aviso -> banner (azul do fundo some na regiao); dismiss -> volta o azul
    notify(s1, b"oi", &mut out, &mut buf, &mut hs);
    if wm_px(ob, stride, 60, 4) == (0, 0, 255) {
        nexo_sys::exit(915);
    }
    dismiss(s1, &mut out, &mut buf, &mut hs);
    if wm_px(ob, stride, 60, 4) != (0, 0, 255) {
        nexo_sys::exit(916);
    }

    // nao-perturbe: aviso descartado
    if !set_dnd(s1, 1, &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(917);
    }
    notify(s1, b"spam", &mut out, &mut buf, &mut hs);
    if wm_px(ob, stride, 60, 4) != (0, 0, 255) {
        nexo_sys::exit(918);
    }
    if !set_dnd(s1, 0, &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(919);
    }

    // sessao em segundo plano pode notificar (mas nao mudar o DND)
    let (s2, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(920));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(921));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(922);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(923),
    }
    if set_dnd(s2, 1, &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(924); // sem posse da entrada: negado
    }
    notify(s2, b"bg", &mut out, &mut buf, &mut hs);
    if wm_px(ob, stride, 60, 4) == (0, 0, 255) {
        nexo_sys::exit(925); // o banner do aviso em segundo plano aparece
    }
    nexo_sys::log(
        "utest: wm notify ok — banner de aviso, DND descarta e so o dono da entrada o controla",
    );
    nexo_sys::exit(0)
}

/// Modo 36: clipboard mediado. So a sessao dona da entrada (janela focada) le/escreve; sessoes em
/// segundo plano recebem erro 6 (nem farejar nem injetar). Historico e opt-in (anel de 4).
fn wm_clipboard() -> ! {
    let s1: nexo_sys::Handle = 0;
    let mut out = [0u8; 384];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];

    let (a, a_base) = wm_create(s1, 0, 0, 8, 8, 0); // foco: A (sessao 1)
    wm_fill(a_base, 8, 8, 255, 0, 0);
    wm_commit(s1, a);
    let _ = a;

    // sessao 2 com a janela B (nao rouba o foco na criacao)
    let (s2, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(840));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(841));
    if nexo_sys::channel_send(s1, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(842);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(843),
    }
    let req = nexo_proto::wm::CreateSurfaceRequest {
        x: 16,
        y: 0,
        w: 8,
        h: 8,
        z: 1,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(844));
    if nexo_sys::channel_send(s2, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(845);
    }
    let (n, nh) =
        nexo_sys::channel_recv(s2, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(846));
    let bs = nexo_proto::wm::decode_create_surface_response(&buf[..n])
        .unwrap_or_else(|_| nexo_sys::exit(847));
    if nh != 1 {
        nexo_sys::exit(848);
    }
    let b = bs.id;
    let b_base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(849));
    wm_fill(b_base, 8, 8, 0, 255, 0);
    let m = nexo_proto::wm::CommitRequest { id: b }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(850));
    if nexo_sys::channel_send(s2, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(851);
    }
    if nexo_sys::channel_recv(s2, &mut buf, &mut hs).is_err() {
        nexo_sys::exit(852);
    }

    // sessao 1 (focada) escreve e le
    let set = |ch: nexo_sys::Handle,
               txt: &[u8],
               out: &mut [u8; 384],
               buf: &mut [u8; 384],
               hs: &mut [u32; 1]|
     -> bool {
        let mut rq = nexo_proto::wm::ClipboardSetRequest {
            data: [0; 256],
            data_len: txt.len() as u32,
        };
        rq.data[..txt.len()].copy_from_slice(txt);
        let m = rq.encode_msg(out).unwrap_or_else(|_| nexo_sys::exit(853));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(854);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_clipboard_set_response(&buf[..n]).is_ok(),
            _ => nexo_sys::exit(855),
        }
    };
    let get = |ch: nexo_sys::Handle,
               out: &mut [u8; 384],
               buf: &mut [u8; 384],
               hs: &mut [u32; 1]|
     -> Option<([u8; 256], usize)> {
        let m = nexo_proto::wm::ClipboardGetRequest {}
            .encode_msg(out)
            .unwrap_or_else(|_| nexo_sys::exit(856));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(857);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_clipboard_get_response(&buf[..n])
                .ok()
                .map(|r| {
                    let mut d = [0u8; 256];
                    d[..r.data().len()].copy_from_slice(r.data());
                    (d, r.data().len())
                }),
            _ => nexo_sys::exit(858),
        }
    };

    if !set(s1, b"hello", &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(860);
    }
    match get(s1, &mut out, &mut buf, &mut hs) {
        Some((d, l)) if &d[..l] == b"hello" => {}
        _ => nexo_sys::exit(861),
    }
    // sessao 2 (sem foco) nao le nem escreve
    if get(s2, &mut out, &mut buf, &mut hs).is_some() {
        nexo_sys::exit(862);
    }
    if set(s2, b"spy", &mut out, &mut buf, &mut hs) {
        nexo_sys::exit(863);
    }

    // foca B por clique; a tecla seguinte (na sessao 2) confirma o novo dono da entrada
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(864));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(865));
    if nexo_sys::channel_send(s1, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(866);
    }
    match nexo_sys::channel_recv(s1, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(867),
    }
    wm_click(inj, 18, 2);
    wm_key(inj, 30, 1);
    let ev = wm_recv_key(s2, &mut buf, &mut hs);
    if ev.surface != b {
        nexo_sys::exit(869);
    }

    // agora a sessao 2 le ("hello" atravessou as sessoes via mediacao) e a 1 e negada
    match get(s2, &mut out, &mut buf, &mut hs) {
        Some((d, l)) if &d[..l] == b"hello" => {}
        _ => nexo_sys::exit(870),
    }
    if get(s1, &mut out, &mut buf, &mut hs).is_some() {
        nexo_sys::exit(871);
    }

    // historico opt-in (sessao 2, focada): liga, grava 2, le na ordem; indice invalido falha
    let m = nexo_proto::wm::ClipboardEnableHistoryRequest {}
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(872));
    if nexo_sys::channel_send(s2, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(873);
    }
    match nexo_sys::channel_recv(s2, &mut buf, &mut hs) {
        Ok((n, _))
            if nexo_proto::wm::decode_clipboard_enable_history_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(874),
    }
    if !set(s2, b"aa", &mut out, &mut buf, &mut hs) || !set(s2, b"bb", &mut out, &mut buf, &mut hs)
    {
        nexo_sys::exit(875);
    }
    let hist = |ch: nexo_sys::Handle,
                idx: u32,
                out: &mut [u8; 384],
                buf: &mut [u8; 384],
                hs: &mut [u32; 1]|
     -> Option<([u8; 256], usize)> {
        let m = nexo_proto::wm::ClipboardHistoryRequest { index: idx }
            .encode_msg(out)
            .unwrap_or_else(|_| nexo_sys::exit(876));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(877);
        }
        match nexo_sys::channel_recv(ch, buf, hs) {
            Ok((n, _)) => nexo_proto::wm::decode_clipboard_history_response(&buf[..n])
                .ok()
                .map(|r| {
                    let mut d = [0u8; 256];
                    d[..r.data().len()].copy_from_slice(r.data());
                    (d, r.data().len())
                }),
            _ => nexo_sys::exit(878),
        }
    };
    match hist(s2, 0, &mut out, &mut buf, &mut hs) {
        Some((d, l)) if &d[..l] == b"bb" => {}
        _ => nexo_sys::exit(880),
    }
    match hist(s2, 1, &mut out, &mut buf, &mut hs) {
        Some((d, l)) if &d[..l] == b"aa" => {}
        _ => nexo_sys::exit(881),
    }
    if hist(s2, 2, &mut out, &mut buf, &mut hs).is_some() {
        nexo_sys::exit(882);
    }
    if hist(s1, 0, &mut out, &mut buf, &mut hs).is_some() {
        nexo_sys::exit(883);
    }
    nexo_sys::log("utest: wm clipboard ok — mediado pela posse da entrada; historico opt-in");
    nexo_sys::exit(0)
}

/// Modo 35: Contextos. Duas janelas no mesmo lugar; move B para o contexto 1 (some da tela, com
/// o estado preservado), troca o contexto ativo (a saida e o foco acompanham) e confere que
/// cliques/teclas ignoram janelas de contextos ocultos.
fn wm_context() -> ! {
    let ch: nexo_sys::Handle = 0;
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0); // A vermelha (ctx 0, foco na criacao)
    wm_commit(ch, a);
    let (b, b_base) = wm_create(ch, 0, 0, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0); // B verde por cima (ctx 0)
    wm_commit(ch, b);

    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(800));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(801);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(802));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(803));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(804));
    let stride = outp.w;
    if wm_px(ob, stride, 4, 4) != (0, 255, 0) {
        nexo_sys::exit(805); // B por cima no ctx 0
    }

    // helper de RPCs vazios
    macro_rules! rpc_ok {
        ($req:expr, $dec:path, $code:expr) => {{
            let m = $req
                .encode_msg(&mut out)
                .unwrap_or_else(|_| nexo_sys::exit($code));
            if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
                nexo_sys::exit($code + 1);
            }
            match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
                Ok((n, _)) if $dec(&buf[..n]).is_ok() => {}
                _ => nexo_sys::exit($code + 2),
            }
        }};
    }

    // B vai para o contexto 1: some da tela (A aparece), estado preservado.
    rpc_ok!(
        nexo_proto::wm::SetContextRequest { id: b, context: 1 },
        nexo_proto::wm::decode_set_context_response,
        810
    );
    if wm_px(ob, stride, 4, 4) != (255, 0, 0) {
        nexo_sys::exit(813);
    }

    // Ativa o contexto 1: so B aparece; o foco vai para B.
    rpc_ok!(
        nexo_proto::wm::SwitchContextRequest { context: 1 },
        nexo_proto::wm::decode_switch_context_response,
        814
    );
    if wm_px(ob, stride, 4, 4) != (0, 255, 0) {
        nexo_sys::exit(817);
    }
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(818));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(819));
    if nexo_sys::channel_send(ch, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(820);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(821),
    }
    wm_key(inj, 30, 1);
    let ev = wm_recv_key(ch, &mut buf, &mut hs);
    if ev.surface != b || ev.code != 30 {
        nexo_sys::exit(822); // foco acompanhou a troca de contexto
    }
    // clique em (4,4): so o contexto ativo conta -> B (A oculta nao e clicavel)
    wm_click(inj, 4, 4);
    wm_key(inj, 48, 1);
    let ev = wm_recv_key(ch, &mut buf, &mut hs);
    if ev.surface != b || ev.code != 48 {
        nexo_sys::exit(823);
    }

    // Volta ao contexto 0: A reaparece intacta e recebe o foco.
    rpc_ok!(
        nexo_proto::wm::SwitchContextRequest { context: 0 },
        nexo_proto::wm::decode_switch_context_response,
        824
    );
    if wm_px(ob, stride, 4, 4) != (255, 0, 0) {
        nexo_sys::exit(827);
    }
    wm_key(inj, 49, 1);
    let ev = wm_recv_key(ch, &mut buf, &mut hs);
    if ev.surface != a || ev.code != 49 {
        nexo_sys::exit(828);
    }
    nexo_sys::log("utest: wm contextos ok — troca mostra so o contexto ativo e move o foco");
    nexo_sys::exit(0)
}

/// Modo 34: driver do teste de login/bloqueio. Handle 0 = sessao nexo.wm, handle 1 = pipe com o
/// greeter. Abre uma 2a sessao do wm e a entrega ao greeter; injeta uma senha errada (continua
/// bloqueado) e depois a certa ("nexo"+Enter); apos o desbloqueio confere que a entrada voltou
/// (clique + tecla chegam a janela do driver).
fn greeter_driver() -> ! {
    let wm_ch: nexo_sys::Handle = 0;
    let pipe: nexo_sys::Handle = 1;
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];

    // Janela do driver (ganha o foco na criacao).
    let (w, w_base) = wm_create(wm_ch, 0, 0, 8, 8, 0);
    wm_fill(w_base, 8, 8, 255, 0, 0);
    wm_commit(wm_ch, w);

    // Abre a 2a sessao e entrega a ponta ao greeter.
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(780));
    let m = nexo_proto::wm::OpenRequest { chan: theirs }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(781));
    if nexo_sys::channel_send(wm_ch, &out[..m], &[theirs]) != Status::Ok {
        nexo_sys::exit(782);
    }
    match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_open_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(783),
    }
    if nexo_sys::channel_send(pipe, b"sess", &[mine]) != Status::Ok {
        nexo_sys::exit(784);
    }

    // Fonte de entrada (o driver injeta as teclas "fisicas" do teste).
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(785));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(786));
    if nexo_sys::channel_send(wm_ch, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(787);
    }
    match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(788),
    }

    let expect_pipe =
        |pipe: nexo_sys::Handle, want: &[u8], code: i64, buf: &mut [u8; 128], hs: &mut [u32; 1]| {
            match nexo_sys::channel_recv(pipe, buf, hs) {
                Ok((n, _)) if &buf[..n] == want => {}
                _ => nexo_sys::exit(code),
            }
        };
    expect_pipe(pipe, b"locked", 789, &mut buf, &mut hs);

    // Senha errada: a(30), a(30), Enter -> continua bloqueado.
    wm_key(inj, 30, 1);
    wm_key(inj, 30, 1);
    wm_key(inj, 28, 1);
    expect_pipe(pipe, b"wrong", 790, &mut buf, &mut hs);

    // Senha certa: n(49) e(18) x(45) o(24) Enter.
    for code in [49u16, 18, 45, 24, 28] {
        wm_key(inj, code, 1);
    }
    expect_pipe(pipe, b"unlocked", 791, &mut buf, &mut hs);

    // Entrada devolvida: clique + tecla chegam a janela do driver.
    wm_click(inj, 2, 2);
    wm_key(inj, 30, 1);
    let ev = wm_recv_key(wm_ch, &mut buf, &mut hs);
    if ev.surface != w || ev.code != 30 {
        nexo_sys::exit(792);
    }
    nexo_sys::log(
        "utest: greeter ok — senha protegida por captura; errada mantem bloqueado; certa devolve a entrada",
    );
    nexo_sys::exit(0)
}

/// Modo 33: multiplos displays (emulados). Cria A no display 0 e B no display 1 (mesmas
/// coordenadas); confere que cada saida so mostra a sua janela; move A para o display 1 e confere
/// que o display 0 fica vazio e o 1 compoe as duas por z.
fn wm_displays() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];

    let mk = |ch: nexo_sys::Handle,
              display: u8,
              z: i32,
              out: &mut [u8; 128],
              buf: &mut [u8; 128],
              hs: &mut [u32; 1]|
     -> (u32, u64) {
        let req = nexo_proto::wm::CreateSurfaceRequest {
            x: 0,
            y: 0,
            w: 8,
            h: 8,
            z,
            display,
        };
        let m = req.encode_msg(out).unwrap_or_else(|_| nexo_sys::exit(750));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(751);
        }
        let (n, nh) = nexo_sys::channel_recv(ch, buf, hs).unwrap_or_else(|_| nexo_sys::exit(752));
        let cs = nexo_proto::wm::decode_create_surface_response(&buf[..n])
            .unwrap_or_else(|_| nexo_sys::exit(753));
        if nh != 1 {
            nexo_sys::exit(754);
        }
        let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(755));
        (cs.id, base)
    };
    let (a, a_base) = mk(ch, 0, 0, &mut out, &mut buf, &mut hs);
    wm_fill(a_base, 8, 8, 255, 0, 0); // A vermelha no display 0
    wm_commit(ch, a);
    let (b, b_base) = mk(ch, 1, 1, &mut out, &mut buf, &mut hs);
    wm_fill(b_base, 8, 8, 0, 255, 0); // B verde no display 1
    wm_commit(ch, b);

    // mapeia as duas saidas
    let outp = |ch: nexo_sys::Handle,
                d: u8,
                out: &mut [u8; 128],
                buf: &mut [u8; 128],
                hs: &mut [u32; 1]|
     -> (u64, i32) {
        let m = nexo_proto::wm::OutputRequest { display: d }
            .encode_msg(out)
            .unwrap_or_else(|_| nexo_sys::exit(756));
        if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(757);
        }
        let (n, nh) = nexo_sys::channel_recv(ch, buf, hs).unwrap_or_else(|_| nexo_sys::exit(758));
        let r = nexo_proto::wm::decode_output_response(&buf[..n])
            .unwrap_or_else(|_| nexo_sys::exit(759));
        if nh != 1 {
            nexo_sys::exit(760);
        }
        let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(761));
        (base, r.w)
    };
    let (d0, stride) = outp(ch, 0, &mut out, &mut buf, &mut hs);
    let (d1, _) = outp(ch, 1, &mut out, &mut buf, &mut hs);

    // cada display mostra so a sua janela
    if wm_px(d0, stride, 4, 4) != (255, 0, 0) {
        nexo_sys::exit(762);
    }
    if wm_px(d1, stride, 4, 4) != (0, 255, 0) {
        nexo_sys::exit(763);
    }

    // move A para o display 1: display 0 fica vazio; no 1, B (z=1) cobre A na sobreposicao
    let m = nexo_proto::wm::MoveToDisplayRequest { id: a, display: 1 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(764));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(765);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_move_to_display_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(766),
    }
    if wm_px(d0, stride, 4, 4) != (0, 0, 0) {
        nexo_sys::exit(767); // display 0 vazio (fundo)
    }
    if wm_px(d1, stride, 4, 4) != (0, 255, 0) {
        nexo_sys::exit(768); // B por cima de A no display 1
    }
    let _ = b;
    nexo_sys::log(
        "utest: wm displays ok — cada display compoe as suas janelas; mover troca de tela",
    );
    nexo_sys::exit(0)
}

/// Modo 32: captura segura de entrada. Com duas janelas, foca B por clique, captura A (`grab`) e
/// confere que (a) as teclas passam a ir para A mesmo com B em foco e (b) cliques sao engolidos
/// (B continua na frente). Depois do `ungrab`, o clique volta a focar/trazer a frente.
fn wm_grab() -> ! {
    let ch: nexo_sys::Handle = 0;
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0); // foco na criacao: A
    wm_fill(a_base, 8, 8, 255, 0, 0);
    wm_commit(ch, a);
    let (b, b_base) = wm_create(ch, 4, 4, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0);
    wm_commit(ch, b);

    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(720));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(721);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(722));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(723));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(724));
    let stride = outp.w;
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(725);
    }

    // fonte de entrada
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(726));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(727));
    if nexo_sys::channel_send(ch, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(728);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(729),
    }

    // 1) clique em B (10,10) muda o foco para B; a tecla seguinte (FIFO) prova o novo foco.
    wm_click(inj, 10, 10);
    wm_key(inj, 30, 1);
    let ev = wm_recv_key(ch, &mut buf, &mut hs);
    if ev.surface != b || ev.code != 30 {
        nexo_sys::exit(730);
    }

    // 2) captura A: teclas vao para A mesmo com B em foco; cliques sao engolidos.
    let m = nexo_proto::wm::GrabRequest { id: a }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(731));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(732);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_grab_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(733),
    }
    wm_click(inj, 2, 2); // engolido: normalmente traria A a frente
    wm_key(inj, 48, 1);
    let ev = wm_recv_key(ch, &mut buf, &mut hs);
    if ev.surface != a || ev.code != 48 {
        nexo_sys::exit(734); // a captura nao desviou a tecla para A
    }
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(735); // o clique nao foi engolido (A veio a frente)
    }

    // 3) solta a captura: o clique volta a focar/trazer a frente.
    let m = nexo_proto::wm::UngrabRequest { id: a }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(736));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(737);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_ungrab_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(738),
    }
    wm_click(inj, 2, 2);
    wm_wait_px(ob, stride, 6, 6, (255, 0, 0), 739); // A a frente de novo
    wm_key(inj, 28, 1);
    let ev = wm_recv_key(ch, &mut buf, &mut hs);
    if ev.surface != a || ev.code != 28 {
        nexo_sys::exit(740);
    }
    nexo_sys::log("utest: wm grab ok — captura desvia o teclado e engole cliques; ungrab restaura");
    nexo_sys::exit(0)
}

/// Modo 31: entrada REAL de ponta a ponta — handle 0 = sessao `nexo.wm`, handle 1 = canal do
/// `inputdev`. Cria uma janela (que ganha o foco na criacao), assina o inputdev (`subscribe`
/// transferindo a ponta de um canal novo) e entrega a outra ponta ao wm (`set_input`). Teclas
/// fisicas (injetadas pelo host via QMP) percorrem inputdev -> wm -> evento `key` da janela em
/// foco. Sai com 0 apos 3 teclas (esperado: 30, 48, 28).
/// Modo 54: ponteiro real. Cadeia inputdev(tablet) --subscribe{64x48}--> canal --set_input-->
/// wm --evento pointer--> janela. O tablet reporta ABS em 0..32767 (absinfo); o inputdev
/// normaliza para os pixels da saida (v1.2) e o clique QMP do host chega como PointerEvent com
/// coordenadas locais da janela sob o cursor.
fn wm_real_pointer() -> ! {
    let wm_ch: nexo_sys::Handle = 0;
    let drv: nexo_sys::Handle = 1;
    // janela unica cobrindo (10,10)..(30,30); clique no centro (20,20) => local (10,10)
    let (a, a_base) = wm_create(wm_ch, 10, 10, 20, 20, 0);
    wm_fill(a_base, 20, 20, 0, 255, 0);
    wm_commit(wm_ch, a);

    let mut out = [0u8; 128];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];
    let (push_wm, push_drv) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(720));
    let m = nexo_proto::input::SubscribeRequest {
        chan: push_drv,
        abs_w: 64,
        abs_h: 48,
    }
    .encode_msg(&mut out)
    .unwrap_or_else(|_| nexo_sys::exit(721));
    if nexo_sys::channel_send(drv, &out[..m], &[push_drv]) != Status::Ok {
        nexo_sys::exit(722);
    }
    match nexo_sys::channel_recv(drv, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::input::decode_subscribe_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(723),
    }
    let m = nexo_proto::wm::SetInputRequest { chan: push_wm }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(724));
    if nexo_sys::channel_send(wm_ch, &out[..m], &[push_wm]) != Status::Ok {
        nexo_sys::exit(725);
    }
    match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(726),
    }
    nexo_sys::log("utest: ponteiro real: cadeia inputdev -> wm -> janela pronta");

    let start = nexo_sys::time_now();
    loop {
        let n = match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(727),
        };
        if let Ok(ev) = nexo_proto::wm::decode_pointer_event(&buf[..n]) {
            // tolerancia de +-1 pixel no arredondamento da normalizacao
            if ev.surface == a && (ev.x - 10).abs() <= 1 && (ev.y - 10).abs() <= 1 {
                nexo_rt::log!(
                    "utest: wm ponteiro real ok — clique local ({}, {}) na janela {}",
                    ev.x,
                    ev.y,
                    ev.surface
                );
                nexo_sys::exit(0)
            }
            nexo_rt::log!(
                "utest: ponteiro real: clique local inesperado ({}, {})",
                ev.x,
                ev.y
            );
        }
        if nexo_sys::time_now() - start > 30_000_000_000 {
            nexo_rt::log!("utest: ponteiro real: sem clique valido em 30 s");
            nexo_sys::exit(728)
        }
    }
}

/// Modo 55: entrada MESCLADA. Teclado e tablet reais alimentam o MESMO canal de entrada do
/// compositor: a ponta de escrita e duplicada (RIGHT_DUPLICATE) e cada driver recebe uma copia
/// no subscribe — lotes evdev sao atomicos por send, entao a mescla e limpa. O teste espera uma
/// tecla (da fase QMP) E um clique local valido na mesma execucao.
fn wm_merged_input() -> ! {
    let wm_ch: nexo_sys::Handle = 0;
    let drv_a: nexo_sys::Handle = 1;
    let drv_b: nexo_sys::Handle = 2;
    let (a, a_base) = wm_create(wm_ch, 10, 10, 20, 20, 0);
    wm_fill(a_base, 20, 20, 255, 255, 0);
    wm_commit(wm_ch, a);

    let mut out = [0u8; 128];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];
    let (push_wm, push) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(740));
    let push2 = nexo_sys::handle_duplicate(push, nexo_sys::abi::RIGHTS_CHANNEL_DEFAULT)
        .unwrap_or_else(|_| nexo_sys::exit(741));
    for (drv, chan) in [(drv_a, push), (drv_b, push2)] {
        let m = nexo_proto::input::SubscribeRequest {
            chan,
            abs_w: 64,
            abs_h: 48,
        }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(742));
        if nexo_sys::channel_send(drv, &out[..m], &[chan]) != Status::Ok {
            nexo_sys::exit(743);
        }
        match nexo_sys::channel_recv(drv, &mut buf, &mut hs) {
            Ok((n, _)) if nexo_proto::input::decode_subscribe_response(&buf[..n]).is_ok() => {}
            _ => nexo_sys::exit(744),
        }
    }
    let m = nexo_proto::wm::SetInputRequest { chan: push_wm }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(745));
    if nexo_sys::channel_send(wm_ch, &out[..m], &[push_wm]) != Status::Ok {
        nexo_sys::exit(746);
    }
    match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(747),
    }
    nexo_sys::log("utest: entrada mesclada: dois drivers no mesmo canal, janela pronta");

    let mut got_key = false;
    let mut got_click = false;
    let start = nexo_sys::time_now();
    loop {
        let n = match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(748),
        };
        if let Ok(ev) = nexo_proto::wm::decode_key_event(&buf[..n])
            && ev.value == 1
        {
            nexo_rt::log!("utest: mesclada: tecla code={}", ev.code);
            got_key = true;
        } else if let Ok(ev) = nexo_proto::wm::decode_pointer_event(&buf[..n])
            && ev.surface == a
            && (ev.x - 10).abs() <= 1
            && (ev.y - 10).abs() <= 1
        {
            nexo_rt::log!("utest: mesclada: clique local ({}, {})", ev.x, ev.y);
            got_click = true;
        }
        if got_key && got_click {
            nexo_sys::log("utest: entrada mesclada ok — tecla e clique pelo mesmo canal");
            nexo_sys::exit(0)
        }
        if nexo_sys::time_now() - start > 30_000_000_000 {
            nexo_rt::log!(
                "utest: mesclada: em 30 s tecla={} clique={}",
                got_key,
                got_click
            );
            nexo_sys::exit(749)
        }
    }
}

fn wm_real_input() -> ! {
    let wm_ch: nexo_sys::Handle = 0;
    let drv: nexo_sys::Handle = 1;
    let (a, a_base) = wm_create(wm_ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 0, 0, 255);
    wm_commit(wm_ch, a);

    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    // Assina o inputdev: a ponta `push_drv` vai para o driver, `push_wm` vira a fonte do wm.
    let (push_wm, push_drv) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(700));
    let m = nexo_proto::input::SubscribeRequest {
        chan: push_drv,
        abs_w: 0,
        abs_h: 0,
    }
    .encode_msg(&mut out)
    .unwrap_or_else(|_| nexo_sys::exit(701));
    if nexo_sys::channel_send(drv, &out[..m], &[push_drv]) != Status::Ok {
        nexo_sys::exit(702);
    }
    match nexo_sys::channel_recv(drv, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::input::decode_subscribe_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(703),
    }
    let m = nexo_proto::wm::SetInputRequest { chan: push_wm }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(704));
    if nexo_sys::channel_send(wm_ch, &out[..m], &[push_wm]) != Status::Ok {
        nexo_sys::exit(705);
    }
    match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(706),
    }
    nexo_sys::log("utest: input real: cadeia inputdev -> wm -> janela pronta");

    let mut presses = 0u32;
    let mut last = 0u32;
    let start = nexo_sys::time_now();
    loop {
        let n = match nexo_sys::channel_recv(wm_ch, &mut buf, &mut hs) {
            Ok((n, _)) => n,
            _ => nexo_sys::exit(707),
        };
        if let Ok(ev) = nexo_proto::wm::decode_key_event(&buf[..n])
            && ev.value == 1
        {
            presses += 1;
            last = ev.code;
            nexo_rt::log!(
                "utest: input real: tecla code={} na janela {}",
                ev.code,
                ev.surface
            );
        }
        if presses >= 3 {
            nexo_rt::log!(
                "utest: wm input real ok ({} teclas, ultima code={})",
                presses,
                last
            );
            nexo_sys::exit(0)
        }
        if nexo_sys::time_now() - start > 30_000_000_000 {
            nexo_rt::log!("utest: input real: apenas {} tecla(s) em 30 s", presses);
            nexo_sys::exit(708)
        }
    }
}

/// Modo 30: mosaico. Duas janelas totalmente sobrepostas; `tile` as organiza numa grade que cobre
/// a saida (sem realocar buffers — o conteudo e escalado na composicao) e a saida composta passa a
/// mostrar as duas lado a lado.
fn wm_tile() -> ! {
    let ch: nexo_sys::Handle = 0;
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0); // A vermelha
    wm_commit(ch, a);
    let (b, b_base) = wm_create(ch, 0, 0, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0); // B verde, exatamente por cima de A
    wm_commit(ch, b);

    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(660));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(661);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(662));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(663));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(664));
    let stride = outp.w;
    if wm_px(ob, stride, 4, 4) != (0, 255, 0) {
        nexo_sys::exit(665); // antes do mosaico: B cobre A
    }

    // Mosaico: A e B lado a lado, cada uma numa celula de 32x48, conteudo escalado.
    let m = nexo_proto::wm::TileRequest {}
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(666));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(667);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_tile_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(668),
    }
    if wm_px(ob, stride, 16, 24) != (255, 0, 0) {
        nexo_sys::exit(669); // centro da celula esquerda: A (escalada)
    }
    if wm_px(ob, stride, 48, 24) != (0, 255, 0) {
        nexo_sys::exit(670); // centro da celula direita: B (escalada)
    }
    if wm_px(ob, stride, 4, 4) != (255, 0, 0) {
        nexo_sys::exit(671); // sem sobreposicao: canto esquerdo agora e A
    }
    nexo_sys::log("utest: wm tile ok — mosaico poe as janelas lado a lado com conteudo escalado");
    nexo_sys::exit(0)
}

/// Modo 29: cliente da apresentacao no framebuffer real. Cria uma superficie 16x16 magenta em
/// (0,0) e commita — o wm (que recebeu a concessao do dispositivo de video) compoe e copia a cena
/// para a tela. Segura a cena por ~2 s para o kernel conferir os pixels no framebuffer fisico.
fn wm_present_client() -> ! {
    let ch: nexo_sys::Handle = 0;
    let (id, base) = wm_create(ch, 0, 0, 16, 16, 0);
    wm_fill(base, 16, 16, 255, 0, 255); // magenta (mesmos bytes em RGBX e BGRX)
    wm_commit(ch, id);
    nexo_sys::sleep_ns(2_000_000_000);
    nexo_sys::log("utest: present cliente ok — cena segurada para a leitura do framebuffer");
    nexo_sys::exit(0)
}

/// Modo 28: atalho global Meta+Tab cicla o foco. Duas superficies sobrepostas; injeta Meta+Tab e
/// confere na saida composta que a janela de tras vem para a frente (e ciclando, alterna).
fn wm_shortcut() -> ! {
    let ch: nexo_sys::Handle = 0;
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0); // A vermelha (fundo)
    wm_commit(ch, a);
    let (b, b_base) = wm_create(ch, 4, 4, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0); // B verde (por cima)
    wm_commit(ch, b);

    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(640));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(641);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(642));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(643));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(644));
    let stride = outp.w;
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(645); // B por cima no inicio
    }

    // Registra a fonte de entrada.
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(646));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(647));
    if nexo_sys::channel_send(ch, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(648);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(649),
    }

    // Meta pressionado + Tab: cicla o foco -> A (de tras) vem para a frente -> (6,6) vermelho.
    wm_key(inj, 125, 1); // KEY_LEFTMETA press
    wm_key(inj, 15, 1); // KEY_TAB press
    wm_wait_px(ob, stride, 6, 6, (255, 0, 0), 650);
    // Tab de novo (Meta ainda pressionado): cicla -> B vem para a frente -> (6,6) verde.
    wm_key(inj, 15, 1);
    wm_wait_px(ob, stride, 6, 6, (0, 255, 0), 651);

    nexo_sys::log("utest: wm atalho ok — Meta+Tab cicla o foco entre as janelas");
    nexo_sys::exit(0)
}

/// Modo 27: maximizar/restaurar janela. Cria uma superficie pequena, maximiza (preenche a saida),
/// e restaura (volta ao retangulo anterior) — conferindo na saida composta a cada passo. Cada
/// realocacao devolve um novo buffer, que o cliente remapeia.
fn wm_maximize() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];

    // Cria A 8x8 em (4,4), guardando o handle e o tamanho do buffer.
    let req = nexo_proto::wm::CreateSurfaceRequest {
        x: 4,
        y: 4,
        w: 8,
        h: 8,
        z: 0,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(600));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(601);
    }
    let (n, nh) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(602));
    let cs = nexo_proto::wm::decode_create_surface_response(&buf[..n])
        .unwrap_or_else(|_| nexo_sys::exit(603));
    if nh != 1 {
        nexo_sys::exit(604);
    }
    let a = cs.id;
    let mut handle = hs[0];
    let mut base = nexo_sys::memory_map(handle).unwrap_or_else(|_| nexo_sys::exit(605));
    let mut bytes = 8u64 * 8 * 4;
    wm_fill(base, 8, 8, 255, 0, 0);
    wm_commit(ch, a);

    // Mapeia a saida.
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(606));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(607);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(608));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(609));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(610));
    let stride = outp.w;
    // Inicial: A em (4,4) 8x8 -> (5,5) vermelho, (30,30) fundo.
    if wm_px(ob, stride, 5, 5) != (255, 0, 0) {
        nexo_sys::exit(611);
    }
    if wm_px(ob, stride, 30, 30) != (0, 0, 0) {
        nexo_sys::exit(612);
    }

    // Maximiza: A passa a preencher a saida inteira (outp.w x outp.h).
    let m = nexo_proto::wm::MaximizeRequest { id: a }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(613));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(614);
    }
    let (n, nh) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(615));
    if nexo_proto::wm::decode_maximize_response(&buf[..n]).is_err() || nh != 1 {
        nexo_sys::exit(616);
    }
    base = wm_realloc_apply(base, bytes, handle, hs[0]);
    handle = hs[0];
    bytes = (outp.w * outp.h * 4) as u64;
    wm_fill(base, outp.w, outp.h, 0, 0, 255); // azul, tela cheia
    wm_commit(ch, a);
    if wm_px(ob, stride, 30, 30) != (0, 0, 255) {
        nexo_sys::exit(617);
    }
    if wm_px(ob, stride, 0, 0) != (0, 0, 255) {
        nexo_sys::exit(618);
    }

    // Restaura: A volta a (4,4) 8x8.
    let m = nexo_proto::wm::RestoreRequest { id: a }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(619));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(620);
    }
    let (n, nh) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(621));
    if nexo_proto::wm::decode_restore_response(&buf[..n]).is_err() || nh != 1 {
        nexo_sys::exit(622);
    }
    base = wm_realloc_apply(base, bytes, handle, hs[0]);
    wm_fill(base, 8, 8, 0, 255, 0); // verde
    wm_commit(ch, a);
    if wm_px(ob, stride, 5, 5) != (0, 255, 0) {
        nexo_sys::exit(623);
    }
    if wm_px(ob, stride, 30, 30) != (0, 0, 0) {
        nexo_sys::exit(624);
    }
    nexo_sys::log("utest: wm maximize ok — maximizar preenche a tela e restaurar volta ao tamanho");
    nexo_sys::exit(0)
}

/// Aplica no cliente uma realocacao do compositor: desmapeia e fecha o buffer antigo, mapeia o
/// novo handle e devolve a nova base.
fn wm_realloc_apply(old_base: u64, old_bytes: u64, old_handle: u32, new_handle: u32) -> u64 {
    let old_pages = old_bytes.div_ceil(4096);
    if nexo_sys::memory_unmap(old_base, old_pages * 4096) != Status::Ok {
        nexo_sys::exit(630);
    }
    if nexo_sys::handle_close(old_handle) != Status::Ok {
        nexo_sys::exit(631);
    }
    nexo_sys::memory_map(new_handle).unwrap_or_else(|_| nexo_sys::exit(632))
}

/// Modo 26: toolkit de UI (`nexo-ui`) desenhado atraves do compositor. O cliente pinta o fundo do
/// tema e um botao na sua superficie (via nexo-gfx/nexo-ui) e confere na saida composta do wm que
/// o botao (fundo, borda) apareceu nas cores do tema — prova a pilha app -> ui -> gfx -> wm.
fn wm_ui() -> ! {
    use nexo_gfx::{PixelFormat, Rect, Surface};
    use nexo_ui::{Button, Theme};
    let ch: nexo_sys::Handle = 0;
    let w = 32i32;
    let h = 24i32;
    let (id, base) = wm_create(ch, 0, 0, w, h, 0);
    let theme = Theme::dark();
    {
        // SAFETY: base .. base + w*h*4 foi mapeada por memory_map (USER|RW) neste processo.
        let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (w * h * 4) as usize) };
        let mut surf = Surface::new(px, w as u32, h as u32, w as u32, PixelFormat::Rgbx8888)
            .unwrap_or_else(|| nexo_sys::exit(590));
        surf.clear(theme.bg);
        let btn = Button::new(Rect::new(4, 4, 24, 12), "OK");
        btn.draw(&mut surf, &theme);
    }
    wm_commit(ch, id);

    // Le a saida composta e confere os pixels do botao.
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(591));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(592);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(593));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(594));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(595));
    let stride = outp.w;
    let rgb = |c: nexo_gfx::Color| (c.r, c.g, c.b);
    // fundo do tema fora do botao
    if wm_px(ob, stride, 1, 1) != rgb(theme.bg) {
        nexo_sys::exit(596);
    }
    // interior do botao, a esquerda do rotulo -> fundo do botao
    if wm_px(ob, stride, 5, 11) != rgb(theme.button_bg) {
        nexo_sys::exit(597);
    }
    // borda do botao (canto superior esquerdo do rect)
    if wm_px(ob, stride, 4, 4) != rgb(theme.border) {
        nexo_sys::exit(598);
    }
    nexo_sys::log("utest: wm ui ok — botao do nexo-ui composto pelo wm nas cores do tema");
    nexo_sys::exit(0)
}

/// Modo 25: opacidade por superficie. Duas superficies opacas sobrepostas (a de cima verde);
/// define a opacidade da de cima para ~50% e confere na saida composta que a sobreposicao vira uma
/// mistura verde+vermelho, enquanto as areas exclusivas mantem suas cores.
fn wm_alpha() -> ! {
    let ch: nexo_sys::Handle = 0;
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0); // A vermelha (fundo, opaca)
    wm_commit(ch, a);
    let (b, b_base) = wm_create(ch, 4, 4, 8, 8, 1);
    wm_fill(b_base, 8, 8, 0, 255, 0); // B verde (por cima, opaca)
    wm_commit(ch, b);

    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(570));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(571);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(572));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(573));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(574));
    let stride = outp.w;
    // B opaca por cima: sobreposicao verde.
    if wm_px(ob, stride, 6, 6) != (0, 255, 0) {
        nexo_sys::exit(575);
    }

    // Define a opacidade de B em ~50%.
    let m = nexo_proto::wm::SetAlphaRequest { id: b, alpha: 128 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(576));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(577);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_alpha_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(578),
    }

    // Sobreposicao: ~50% verde sobre vermelho -> (127,128,0) aprox.
    let (r, g, bl) = wm_px(ob, stride, 6, 6);
    if (r as i32 - 127).abs() > 2 || (g as i32 - 128).abs() > 2 || bl != 0 {
        nexo_sys::exit(579);
    }
    // Area so de B (10,10): verde 50% sobre fundo preto -> (0,128,0) aprox.
    let (r, g, bl) = wm_px(ob, stride, 10, 10);
    if r != 0 || (g as i32 - 128).abs() > 2 || bl != 0 {
        nexo_sys::exit(580);
    }
    // Area so de A (2,2): vermelho opaco.
    if wm_px(ob, stride, 2, 2) != (255, 0, 0) {
        nexo_sys::exit(581);
    }
    nexo_sys::log("utest: wm alpha ok — opacidade da janela mistura com o que esta abaixo");
    nexo_sys::exit(0)
}

/// Modo 24: entrega de teclado a janela em foco. Cria uma superficie, foca-a por clique e injeta
/// teclas (EV_KEY); confere que chegam como eventos `key` na sessao dona da superficie focada.
fn wm_keyboard() -> ! {
    let ch: nexo_sys::Handle = 0;
    let (a, a_base) = wm_create(ch, 0, 0, 8, 8, 0);
    wm_fill(a_base, 8, 8, 255, 0, 0);
    wm_commit(ch, a);

    // Registra a fonte de entrada.
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let (inj, src) = nexo_sys::channel_create().unwrap_or_else(|_| nexo_sys::exit(550));
    let m = nexo_proto::wm::SetInputRequest { chan: src }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(551));
    if nexo_sys::channel_send(ch, &out[..m], &[src]) != Status::Ok {
        nexo_sys::exit(552);
    }
    match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, _)) if nexo_proto::wm::decode_set_input_response(&buf[..n]).is_ok() => {}
        _ => nexo_sys::exit(553),
    }

    // Foca A com um clique e injeta uma tecla (press) e depois o release.
    wm_click(inj, 2, 2);
    wm_key(inj, 30, 1); // KEY_A press
    let ev = wm_recv_key(ch, &mut buf, &mut hs);
    if ev.surface != a || ev.code != 30 || ev.value != 1 {
        nexo_sys::exit(560);
    }
    wm_key(inj, 30, 0); // release
    let ev = wm_recv_key(ch, &mut buf, &mut hs);
    if ev.surface != a || ev.code != 30 || ev.value != 0 {
        nexo_sys::exit(561);
    }
    nexo_sys::log("utest: wm teclado ok — teclas chegam a janela em foco");
    nexo_sys::exit(0)
}

/// Recebe um evento `key` do compositor na sessao `ch`.
fn wm_recv_key(
    ch: nexo_sys::Handle,
    buf: &mut [u8],
    hs: &mut [u32; 1],
) -> nexo_proto::wm::KeyEvent {
    loop {
        let (n, _) = nexo_sys::channel_recv(ch, buf, hs).unwrap_or_else(|_| nexo_sys::exit(562));
        if let Ok(ev) = nexo_proto::wm::decode_key_event(&buf[..n]) {
            return ev;
        }
        // eventos de ponteiro (clique entregue a janela) sao pulados aqui
        if nexo_proto::wm::decode_pointer_event(&buf[..n]).is_err() {
            nexo_sys::exit(563)
        }
    }
}

/// Injeta uma tecla evdev (EV_KEY code value) num canal de entrada.
fn wm_key(inj: nexo_sys::Handle, code: u16, value: u32) {
    let mut ev = [0u8; 8];
    ev[0..2].copy_from_slice(&1u16.to_le_bytes()); // EV_KEY
    ev[2..4].copy_from_slice(&code.to_le_bytes());
    ev[4..8].copy_from_slice(&value.to_le_bytes());
    if nexo_sys::channel_send(inj, &ev, &[]) != Status::Ok {
        nexo_sys::exit(546);
    }
}

/// Injeta um clique em (x,y): eventos evdev ABS_X, ABS_Y e BTN_LEFT (press) num canal de entrada.
fn wm_click(inj: nexo_sys::Handle, x: i32, y: i32) {
    let mut ev = [0u8; 24];
    let put = |ev: &mut [u8], off: usize, ty: u16, code: u16, value: u32| {
        ev[off..off + 2].copy_from_slice(&ty.to_le_bytes());
        ev[off + 2..off + 4].copy_from_slice(&code.to_le_bytes());
        ev[off + 4..off + 8].copy_from_slice(&value.to_le_bytes());
    };
    put(&mut ev, 0, 3, 0, x as u32); // EV_ABS, ABS_X
    put(&mut ev, 8, 3, 1, y as u32); // EV_ABS, ABS_Y
    put(&mut ev, 16, 1, 0x110, 1); // EV_KEY, BTN_LEFT, press
    if nexo_sys::channel_send(inj, &ev, &[]) != Status::Ok {
        nexo_sys::exit(545);
    }
}

/// Espera (com timeout) o pixel (x,y) da saida virar `want`; sai com `code` se estourar.
fn wm_wait_px(base: u64, stride: i32, x: i32, y: i32, want: (u8, u8, u8), code: i64) {
    let start = nexo_sys::time_now();
    loop {
        if wm_px(base, stride, x, y) == want {
            return;
        }
        if nexo_sys::time_now() - start > 5_000_000_000 {
            nexo_sys::exit(code);
        }
        nexo_sys::sleep_ns(2_000_000);
    }
}

/// Modo 22: redimensionamento de superficie. Cria uma superficie pequena (8x8), confere que uma
/// area alem dela e fundo, redimensiona para 16x16 (o wm realoca o buffer; o cliente desmapeia e
/// fecha o antigo, remapeia o novo), pinta e confere que a area nova agora aparece na saida.
fn wm_resize() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];

    // Cria A 8x8 em (0,0) z=0, guardando o handle do buffer.
    let req = nexo_proto::wm::CreateSurfaceRequest {
        x: 0,
        y: 0,
        w: 8,
        h: 8,
        z: 0,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(500));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(501);
    }
    let (n, nh) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(502));
    let cs = nexo_proto::wm::decode_create_surface_response(&buf[..n])
        .unwrap_or_else(|_| nexo_sys::exit(503));
    if nh != 1 {
        nexo_sys::exit(504);
    }
    let a = cs.id;
    let old_handle = hs[0];
    let old_base = nexo_sys::memory_map(old_handle).unwrap_or_else(|_| nexo_sys::exit(505));
    wm_fill(old_base, 8, 8, 255, 0, 0);
    wm_commit(ch, a);

    // Mapeia a saida (paginas estaveis).
    let m = nexo_proto::wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(506));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(507);
    }
    let (n, _) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(508));
    let outp =
        nexo_proto::wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(509));
    let ob = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(510));
    let stride = outp.w;
    // (2,2) dentro de A -> vermelho; (12,12) fora de A (8x8) -> fundo preto.
    if wm_px(ob, stride, 2, 2) != (255, 0, 0) {
        nexo_sys::exit(511);
    }
    if wm_px(ob, stride, 12, 12) != (0, 0, 0) {
        nexo_sys::exit(512);
    }

    // Redimensiona A para 16x16.
    let m = nexo_proto::wm::ResizeRequest {
        id: a,
        w: 16,
        h: 16,
    }
    .encode_msg(&mut out)
    .unwrap_or_else(|_| nexo_sys::exit(513));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(514);
    }
    let (n, nh) =
        nexo_sys::channel_recv(ch, &mut buf, &mut hs).unwrap_or_else(|_| nexo_sys::exit(515));
    let rz =
        nexo_proto::wm::decode_resize_response(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(516));
    if nh != 1 {
        nexo_sys::exit(517);
    }
    let _ = rz;
    let new_handle = hs[0];
    // Desmapeia e fecha o buffer antigo (8x8 = 1 pagina), depois mapeia o novo.
    if nexo_sys::memory_unmap(old_base, 4096) != Status::Ok {
        nexo_sys::exit(518);
    }
    if nexo_sys::handle_close(old_handle) != Status::Ok {
        nexo_sys::exit(519);
    }
    let new_base = nexo_sys::memory_map(new_handle).unwrap_or_else(|_| nexo_sys::exit(520));
    wm_fill(new_base, 16, 16, 0, 0, 255); // azul
    wm_commit(ch, a);

    // Agora (12,12) e (2,2) estao dentro de A (16x16) -> azul.
    if wm_px(ob, stride, 12, 12) != (0, 0, 255) {
        nexo_sys::exit(521);
    }
    if wm_px(ob, stride, 2, 2) != (0, 0, 255) {
        nexo_sys::exit(522);
    }
    nexo_sys::log("utest: wm resize ok — buffer realocado (munmap) e a area nova aparece na saida");
    nexo_sys::exit(0)
}

/// Envia `raise` (frente) ou `lower` (tras) para a superficie `id` e espera a resposta.
fn wm_restack_op(ch: nexo_sys::Handle, id: u32, raise: bool) {
    let mut out = [0u8; 64];
    let mut buf = [0u8; 64];
    let mut hs = [0u32; 1];
    let m = if raise {
        nexo_proto::wm::RaiseRequest { id }.encode_msg(&mut out)
    } else {
        nexo_proto::wm::LowerRequest { id }.encode_msg(&mut out)
    }
    .unwrap_or_else(|_| nexo_sys::exit(486));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(487);
    }
    if nexo_sys::channel_recv(ch, &mut buf, &mut hs).is_err() {
        nexo_sys::exit(488);
    }
}

/// Cria uma superficie no compositor e devolve (id, base mapeada da memoria compartilhada).
fn wm_create(ch: nexo_sys::Handle, x: i32, y: i32, w: i32, h: i32, z: i32) -> (u32, u64) {
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let req = nexo_proto::wm::CreateSurfaceRequest {
        x,
        y,
        w,
        h,
        z,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(430));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(431);
    }
    let (n, nh) = match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok(v) => v,
        _ => nexo_sys::exit(432),
    };
    let resp = nexo_proto::wm::decode_create_surface_response(&buf[..n])
        .unwrap_or_else(|_| nexo_sys::exit(433));
    if nh != 1 {
        nexo_sys::exit(434);
    }
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| nexo_sys::exit(435));
    (resp.id, base)
}

/// Faz commit de uma superficie e espera a resposta do compositor.
fn wm_commit(ch: nexo_sys::Handle, id: u32) {
    let mut out = [0u8; 128];
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let m = nexo_proto::wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| nexo_sys::exit(436));
    if nexo_sys::channel_send(ch, &out[..m], &[]) != Status::Ok {
        nexo_sys::exit(437);
    }
    if nexo_sys::channel_recv(ch, &mut buf, &mut hs).is_err() {
        nexo_sys::exit(438);
    }
}

/// Preenche uma superficie WxH (bytes Rgbx8888 = [r,g,b,0]) com uma cor solida.
fn wm_fill(base: u64, w: i32, h: i32, r: u8, g: u8, b: u8) {
    let px = (w * h) as usize;
    for i in 0..px {
        // SAFETY: base .. base+w*h*4 foi mapeada por memory_map (USER|RW) neste processo.
        unsafe {
            let p = (base as *mut u8).add(i * 4);
            p.write(r);
            p.add(1).write(g);
            p.add(2).write(b);
            p.add(3).write(0);
        }
    }
}

/// Le um pixel (r,g,b) da saida composta mapeada.
/// Le um campo `u32` do cabecalho da saida composta (layout `nexo_wm::frame`).
fn wm_hdr(base: u64, off: usize) -> u32 {
    // SAFETY: `base` e o inicio do mapeamento da saida (pagina de cabecalho) e `off` e um dos
    // offsets `frame::OFF_*`, alinhado a 4 e dentro da pagina.
    unsafe { core::ptr::read_volatile((base as usize + off) as *const u32) }
}

/// Le o pixel (x,y) do frame PUBLICADO da saida composta, pelo protocolo do seqlock: espera
/// `seq` par, le do buffer da frente e reconfere `seq` — nunca observa um frame rasgado.
fn wm_px(base: u64, stride: i32, x: i32, y: i32) -> (u8, u8, u8) {
    use nexo_wm::frame;
    let (w, h) = (wm_hdr(base, frame::OFF_W), wm_hdr(base, frame::OFF_H));
    loop {
        let s1 = wm_hdr(base, frame::OFF_SEQ);
        if s1 & 1 == 1 {
            core::hint::spin_loop();
            continue;
        }
        let front = wm_hdr(base, frame::OFF_FRONT);
        let off = frame::buf_offset(w, h, front) + ((y * stride + x) * 4) as u64;
        // SAFETY: leitura dentro do buffer da frente da saida mapeada (w*h*4 bytes).
        let px = unsafe {
            let p = (base as *const u8).add(off as usize);
            (
                p.read_volatile(),
                p.add(1).read_volatile(),
                p.add(2).read_volatile(),
            )
        };
        if wm_hdr(base, frame::OFF_SEQ) == s1 {
            return px;
        }
    }
}

/// Modo 18: consumidor (handle 0 = canal). Recebe o handle do objeto de memoria, mapeia,
/// confere o marcador do produtor e escreve uma resposta na mesma memoria.
fn shmem_consumer() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut buf = [0u8; 16];
    let mut hs = [0u32; 1];
    let mem = match nexo_sys::channel_recv(ch, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"mem" => hs[0],
        _ => nexo_sys::exit(420),
    };
    let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| nexo_sys::exit(421));
    let mut marker = [0u8; 8];
    // SAFETY: base foi mapeada por memory_map; confere o marcador do produtor.
    unsafe {
        core::ptr::copy_nonoverlapping(base as *const u8, marker.as_mut_ptr(), 8);
    }
    if &marker != b"SHMEM-OK" {
        nexo_sys::exit(422);
    }
    // escreve a resposta no offset 64 (visivel para o produtor)
    // SAFETY: mesma pagina compartilhada.
    unsafe {
        core::ptr::copy_nonoverlapping(b"REPLY".as_ptr(), (base + 64) as *mut u8, 5);
    }
    nexo_sys::log("utest: shmem consumidor ok — leu o marcador e respondeu");
    nexo_sys::exit(0)
}

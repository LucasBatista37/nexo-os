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
    let (st, kernel_size, _) = call(1, 0, 0, b"/nexo/kernel.elf", &mut ereq, &mut ereply, hs);
    if st != 0 || kernel_size == 0 {
        nexo_sys::exit(195);
    }
    let (st, r, n) = call(2, 0, 4, b"/nexo/kernel.elf", &mut ereq, &mut ereply, hs);
    if st != 0 || r != 4 || n != 4 || &ereply[12..16] != b"\x7fELF" {
        nexo_rt::log!("utest: esp: cabecalho do kernel: status {} {} bytes", st, r);
        nexo_sys::exit(196);
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
    let (kino, n) = a.ok(0, 0, 0, 0, b"/boot/nexo/kernel.elf", 212);
    let ksize = u64::from_le_bytes(a.data(n)[1..9].try_into().unwrap());
    if a.data(n)[0] != 1 || ksize == 0 {
        nexo_sys::exit(213);
    }
    let kino = kino as u32;
    let (r, n) = a.ok(4, kino, 0, 4, &[], 214);
    if r != 4 || a.data(n) != b"\x7fELF" {
        nexo_sys::exit(215);
    }
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
    let (st, _, _) = b.call(0, 0, 0, 0, b"/boot/nexo/kernel.elf");
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
        // SAFETY: leitura da mesma pagina mapeada.
        let mut reply = [0u8; 5];
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
    let m = nexo_proto::input::SubscribeRequest { chan: push_drv }
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
    buf: &mut [u8; 128],
    hs: &mut [u32; 1],
) -> nexo_proto::wm::KeyEvent {
    let (n, _) = nexo_sys::channel_recv(ch, buf, hs).unwrap_or_else(|_| nexo_sys::exit(562));
    nexo_proto::wm::decode_key_event(&buf[..n]).unwrap_or_else(|_| nexo_sys::exit(563))
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
fn wm_px(base: u64, stride: i32, x: i32, y: i32) -> (u8, u8, u8) {
    // SAFETY: leitura dentro da saida mapeada (w*h*4 bytes).
    unsafe {
        let p = (base as *const u8).add(((y * stride + x) * 4) as usize);
        (p.read(), p.add(1).read(), p.add(2).read())
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
    // SAFETY: base foi mapeada por memory_map; confere o marcador do produtor.
    let mut marker = [0u8; 8];
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

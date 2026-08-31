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
        if n == SYS_EXIT || n == SYS_CHANNEL_RECV || n == SYS_PROCESS_WAIT {
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
    nexo_sys::exit(0)
}

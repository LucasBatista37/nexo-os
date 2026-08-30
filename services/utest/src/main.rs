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
        7 => syscall_fuzz(),
        8 => block_client(),
        9 => fs_client(),
        10 => fs_churn(),
        11 => devmgr_client(),
        12 => vfs_client(),
        13 => input_client(),
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
fn syscall_fuzz() -> ! {
    use nexo_sys::abi::*;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
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
fn block_client() -> ! {
    let ch: nexo_sys::Handle = 0;
    let mut req = [0u8; 4096];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    fn header(req: &mut [u8], op: u8, sector: u64, count: u32) {
        req[0] = op;
        req[1..4].copy_from_slice(&[0, 0, 0]);
        req[4..12].copy_from_slice(&sector.to_le_bytes());
        req[12..16].copy_from_slice(&count.to_le_bytes());
    }
    // 0. Capacidade: os testes crus usam a area reservada nos ultimos 256 setores
    //    (o `fs` nao a administra), para nao colidir com o volume NexoFS.
    header(&mut req, 2, 0, 0);
    if nexo_sys::channel_send(ch, &req[..16], &[]) != Status::Ok {
        nexo_sys::exit(108);
    }
    let base = match nexo_sys::channel_recv(ch, &mut reply, &mut hs) {
        Ok((9, _)) if reply[0] == 0 => {
            u64::from_le_bytes(reply[1..9].try_into().unwrap()).saturating_sub(256)
        }
        _ => nexo_sys::exit(109),
    };
    // 1. Escreve padrao em 4 setores (2048 bytes) em um pedido.
    header(&mut req, 1, base, 4);
    for i in 0..2048usize {
        req[16 + i] = (i as u8) ^ 0x5a ^ ((i / 512) as u8);
    }
    if nexo_sys::channel_send(ch, &req[..16 + 2048], &[]) != Status::Ok {
        nexo_sys::exit(110);
    }
    match nexo_sys::channel_recv(ch, &mut reply, &mut hs) {
        Ok((1, _)) if reply[0] == 0 => {}
        Ok((n, _)) => {
            nexo_rt::log!("utest: escrita falhou: status {} ({} bytes)", reply[0], n);
            nexo_sys::exit(111)
        }
        Err(_) => nexo_sys::exit(112),
    }
    // 2. Le de volta e compara.
    header(&mut req, 0, base, 4);
    if nexo_sys::channel_send(ch, &req[..16], &[]) != Status::Ok {
        nexo_sys::exit(113);
    }
    match nexo_sys::channel_recv(ch, &mut reply, &mut hs) {
        Ok((n, _)) if n == 1 + 2048 && reply[0] == 0 => {}
        Ok((n, _)) => {
            nexo_rt::log!("utest: leitura falhou: status {} ({} bytes)", reply[0], n);
            nexo_sys::exit(114)
        }
        Err(_) => nexo_sys::exit(115),
    }
    for i in 0..2048usize {
        if reply[1 + i] != (i as u8) ^ 0x5a ^ ((i / 512) as u8) {
            nexo_rt::log!("utest: dado divergente no byte {}", i);
            nexo_sys::exit(116);
        }
    }
    // 3. Marcador de persistencia no setor base+8.
    header(&mut req, 0, base + 8, 1);
    if nexo_sys::channel_send(ch, &req[..16], &[]) != Status::Ok {
        nexo_sys::exit(117);
    }
    let marker = b"NEXO-PERSIST-v1";
    match nexo_sys::channel_recv(ch, &mut reply, &mut hs) {
        Ok((n, _)) if n == 1 + 512 && reply[0] == 0 => {
            if reply[1..1 + marker.len()] == marker[..] {
                nexo_sys::log("utest: bloco persistente encontrado (boot anterior)");
            } else {
                header(&mut req, 1, base + 8, 1);
                req[16..16 + 512].fill(0);
                req[16..16 + marker.len()].copy_from_slice(marker);
                if nexo_sys::channel_send(ch, &req[..16 + 512], &[]) != Status::Ok {
                    nexo_sys::exit(118);
                }
                match nexo_sys::channel_recv(ch, &mut reply, &mut hs) {
                    Ok((1, _)) if reply[0] == 0 => {
                        nexo_sys::log("utest: marcador de persistencia gravado");
                    }
                    _ => nexo_sys::exit(119),
                }
            }
        }
        _ => nexo_sys::exit(120),
    }
    // 4. Pedido invalido (alem da capacidade) deve ser recusado, nao derrubar o driver.
    header(&mut req, 0, u64::MAX / 2, 1);
    let _ = nexo_sys::channel_send(ch, &req[..16], &[]);
    match nexo_sys::channel_recv(ch, &mut reply, &mut hs) {
        Ok((1, _)) if reply[0] == 2 => {}
        _ => nexo_sys::exit(121),
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
    /// Envia `op` e devolve (status, valor, tamanho dos dados em `reply[12..]`).
    fn call(
        &mut self,
        op: u8,
        ino: u32,
        offset: u64,
        len: u32,
        payload: &[u8],
    ) -> (u8, u64, usize) {
        self.req[0] = op;
        self.req[1..4].fill(0);
        self.req[4..8].copy_from_slice(&ino.to_le_bytes());
        self.req[8..16].copy_from_slice(&offset.to_le_bytes());
        self.req[16..20].copy_from_slice(&len.to_le_bytes());
        let n = 20 + payload.len();
        self.req[20..n].copy_from_slice(payload);
        if nexo_sys::channel_send(self.ch, &self.req[..n], &[]) != Status::Ok {
            nexo_sys::exit(130);
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(self.ch, &mut self.reply, &mut hs) {
            Ok((n, _)) if n >= 12 => {
                let value = u64::from_le_bytes(self.reply[4..12].try_into().unwrap());
                (self.reply[0], value, n - 12)
            }
            _ => nexo_sys::exit(131),
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
    let mut r1 = [0u8; 64];
    let mut r2 = [0u8; 64];
    for (i, out) in [&mut r1, &mut r2].into_iter().enumerate() {
        if nexo_sys::channel_send(rng, &32u32.to_le_bytes(), &[]) != Status::Ok {
            nexo_sys::exit(184);
        }
        match nexo_sys::channel_recv(rng, &mut buf, &mut hs) {
            Ok((33, _)) if buf[0] == 0 => out[..32].copy_from_slice(&buf[1..33]),
            Ok((n, _)) => {
                nexo_rt::log!(
                    "utest: rng: resposta {} inesperada ({} bytes, status {})",
                    i,
                    n,
                    buf[0]
                );
                nexo_sys::exit(185)
            }
            Err(_) => nexo_sys::exit(186),
        }
    }
    if r1[..32].iter().all(|&b| b == 0) || r1[..32] == r2[..32] {
        nexo_sys::log("utest: rng: bytes nulos ou repetidos");
        nexo_sys::exit(187);
    }
    // pedido invalido e recusado sem derrubar o driver
    let _ = nexo_sys::channel_send(rng, &4096u32.to_le_bytes(), &[]);
    match nexo_sys::channel_recv(rng, &mut buf, &mut hs) {
        Ok((1, _)) if buf[0] == 1 => {}
        _ => nexo_sys::exit(188),
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
                ereq: &mut [u8; 64],
                ereply: &mut [u8; 4096],
                hs: &mut [u32; 1]|
     -> (u8, u64, usize) {
        ereq[0] = op;
        ereq[1..4].fill(0);
        ereq[4..12].copy_from_slice(&offset.to_le_bytes());
        ereq[12..16].copy_from_slice(&len.to_le_bytes());
        ereq[16..16 + path.len()].copy_from_slice(path);
        if nexo_sys::channel_send(esp, &ereq[..16 + path.len()], &[]) != Status::Ok {
            nexo_sys::exit(190);
        }
        match nexo_sys::channel_recv(esp, ereply, hs) {
            Ok((n, _)) if n >= 12 => (
                ereply[0],
                u64::from_le_bytes(ereply[4..12].try_into().unwrap()),
                n - 12,
            ),
            _ => nexo_sys::exit(191),
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
    let req = [0u8; 1];
    let mut reply = [0u8; 4096];
    let mut hs = [0u32; 1];
    let mut keys = 0u32;
    let mut last_code = 0u16;
    let start = nexo_sys::time_now();
    loop {
        if nexo_sys::channel_send(0, &req, &[]) != Status::Ok {
            nexo_sys::exit(240);
        }
        let n = match nexo_sys::channel_recv(0, &mut reply, &mut hs) {
            Ok((n, _)) if n >= 1 && reply[0] == 0 => n - 1,
            _ => nexo_sys::exit(241),
        };
        let mut off = 0usize;
        while off + 8 <= n {
            let ty = u16::from_le_bytes(reply[1 + off..3 + off].try_into().unwrap());
            let code = u16::from_le_bytes(reply[3 + off..5 + off].try_into().unwrap());
            let value = u32::from_le_bytes(reply[5 + off..9 + off].try_into().unwrap());
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

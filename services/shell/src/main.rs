//! `shell` — shell de diagnóstico na console VirtIO. Handle 0 = canal do `consoledev`
//! (protocolo tipado `nexo.console` v1.0), handle 1 = canal de um `vfs` (`nexo.fs` v0).
//! Comandos: `ajuda`, `info`, `tempo`, `ls [caminho]`, `cat <caminho>`,
//! `escreve <caminho> <texto>`, `remove <caminho>`, `eco <texto>`, `sair`; qualquer outro
//! nome tenta o utilitário POSIX `<nome>-c` do initrd (convenção de argv/stdout do Nexo,
//! com o vfs duplicado como serviço principal — saída bombeada para a console).
#![no_std]
#![no_main]

use core::fmt::Write;
use nexo_proto::console::{ReadRequest, WriteRequest, decode_read_response, decode_write_response};
use nexo_rt::Buf;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const CON: Handle = 0;
const VFS: Handle = 1;

fn con_write(data: &[u8]) {
    let mut msg = [0u8; 4096];
    let n = data.len().min(2048);
    let mut w = WriteRequest {
        data: [0; 3500],
        data_len: n as u32,
    };
    w.data[..n].copy_from_slice(&data[..n]);
    let m = w.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(CON, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(70);
    }
    let mut hs = [0u32; 1];
    match nexo_sys::channel_recv(CON, &mut msg, &mut hs) {
        Ok((rn, _)) if decode_write_response(&msg[..rn]).is_ok() => {}
        _ => nexo_sys::exit(74),
    }
}

/// Le o que houver na console para `out`; devolve o tamanho.
fn con_poll(out: &mut [u8; 4096]) -> usize {
    let mut msg = [0u8; 64];
    let m = ReadRequest {}.encode_msg(&mut msg).unwrap_or(0);
    if nexo_sys::channel_send(CON, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(71);
    }
    let mut hs = [0u32; 1];
    match nexo_sys::channel_recv(CON, out, &mut hs) {
        Ok((n, _)) => match decode_read_response(&out[..n]) {
            Ok(r) => {
                let len = r.data().len();
                let mut data = [0u8; 3500];
                data[..len].copy_from_slice(r.data());
                out[..len].copy_from_slice(&data[..len]);
                len
            }
            Err(_) => 0,
        },
        _ => 0,
    }
}

/// Pedido `nexo.fs` (tipado) ao vfs; devolve (status, valor, dados em `reply[12..]`)
/// no formato legado (0 stat, 1 create, 2 mkdir, 3 unlink, 4 read, 5 write, 6 list,
/// 7 sync, 8 info, 9 truncate).
fn vfs_call(
    op: u8,
    ino: u32,
    offset: u64,
    len: u32,
    payload: &[u8],
    reply: &mut [u8; 4096],
) -> (u8, u64, usize) {
    use nexo_proto::ProtoError;
    use nexo_proto::fs as pfs;
    let mut pbuf = [0u8; 256];
    let pn = payload.len().min(256);
    pbuf[..pn].copy_from_slice(&payload[..pn]);
    let mut msg = [0u8; 4096];
    let m = match op {
        0 => pfs::StatRequest {
            path: pbuf,
            path_len: pn as u32,
        }
        .encode_msg(&mut msg),
        1 => pfs::CreateRequest {
            path: pbuf,
            path_len: pn as u32,
        }
        .encode_msg(&mut msg),
        2 => pfs::MkdirRequest {
            path: pbuf,
            path_len: pn as u32,
        }
        .encode_msg(&mut msg),
        3 => pfs::UnlinkRequest {
            path: pbuf,
            path_len: pn as u32,
        }
        .encode_msg(&mut msg),
        4 => pfs::ReadRequest { ino, offset, len }.encode_msg(&mut msg),
        5 => {
            let mut rq = pfs::WriteRequest {
                ino,
                offset,
                data: [0; 3900],
                data_len: payload.len().min(3900) as u32,
            };
            rq.data[..payload.len().min(3900)].copy_from_slice(&payload[..payload.len().min(3900)]);
            rq.encode_msg(&mut msg)
        }
        6 => pfs::ListRequest {
            path: pbuf,
            path_len: pn as u32,
        }
        .encode_msg(&mut msg),
        7 => pfs::SyncRequest {}.encode_msg(&mut msg),
        8 => pfs::InfoRequest {}.encode_msg(&mut msg),
        _ => pfs::TruncateRequest { ino, size: offset }.encode_msg(&mut msg),
    }
    .unwrap_or(0);
    if nexo_sys::channel_send(VFS, &msg[..m], &[]) != Status::Ok {
        nexo_sys::exit(72);
    }
    let mut hs = [0u32; 1];
    let n = match nexo_sys::channel_recv(VFS, reply, &mut hs) {
        Ok((n, _)) => n,
        _ => nexo_sys::exit(73),
    };
    fn remote(e: ProtoError) -> (u8, u64, usize) {
        match e {
            ProtoError::Remote(c) => (c as u8, 0, 0),
            _ => (0xfd, 0, 0),
        }
    }
    let copy: [u8; 4096] = *reply;
    let msg = &copy[..n];
    match op {
        0 => match pfs::decode_stat_response(msg) {
            Ok(r) => {
                reply[12] = r.kind;
                reply[13..21].copy_from_slice(&r.size.to_le_bytes());
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
                reply[12..12 + dl].copy_from_slice(r.data());
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
                reply[12..12 + dl].copy_from_slice(r.entries());
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
                reply[12..20].copy_from_slice(&r.total_blocks.to_le_bytes());
                reply[20..28].copy_from_slice(&r.free_blocks.to_le_bytes());
                reply[28..36].copy_from_slice(&r.repairs.to_le_bytes());
                reply[36..44].copy_from_slice(&r.generation.to_le_bytes());
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

fn print(args: core::fmt::Arguments<'_>) {
    let mut b = Buf::<1024>::new();
    let _ = b.write_fmt(args);
    con_write(b.as_bytes());
}

macro_rules! outln {
    ($($t:tt)*) => { print(format_args!($($t)*)); con_write(b"\r\n"); };
}

fn cmd_info() {
    let cpus = nexo_sys::debug_info(0);
    let uptime = nexo_sys::debug_info(1);
    let syscalls = nexo_sys::debug_info(2);
    let handles = nexo_sys::debug_info(3);
    let procs = nexo_sys::debug_info(4);
    outln!("cpus: {}", cpus);
    outln!("uptime: {} ms", uptime);
    outln!("processos vivos: {}", procs);
    outln!("handles deste processo: {}", handles);
    outln!("syscalls deste processo: {}", syscalls);
}

fn cmd_ls(path: &[u8], reply: &mut [u8; 4096]) {
    let path = if path.is_empty() { b"/" } else { path };
    let (st, count, n) = vfs_call(6, 0, 0, 0, path, reply);
    if st != 0 {
        outln!("ls: erro {}", st);
        return;
    }
    let mut pos = 0usize;
    let data_copy: [u8; 4096] = *reply;
    while pos + 6 <= n {
        let kind = data_copy[12 + pos + 4];
        let nl = data_copy[12 + pos + 5] as usize;
        let name = &data_copy[12 + pos + 6..12 + pos + 6 + nl];
        let mut b = Buf::<128>::new();
        let _ = b.write_fmt(format_args!(
            "{} {}",
            if kind == 2 { 'd' } else { '-' },
            core::str::from_utf8(name).unwrap_or("?")
        ));
        con_write(b.as_bytes());
        con_write(b"\r\n");
        pos += 6 + nl;
    }
    outln!("{} entrada(s)", count);
}

fn cmd_cat(path: &[u8], reply: &mut [u8; 4096]) {
    let (st, ino, n) = vfs_call(0, 0, 0, 0, path, reply);
    if st != 0 {
        outln!("cat: erro {}", st);
        return;
    }
    if reply[12] == 2 {
        outln!("cat: e diretorio");
        return;
    }
    let size = u64::from_le_bytes(reply[13..21].try_into().unwrap());
    let _ = n;
    let mut off = 0u64;
    let mut shown = 0u64;
    while off < size && shown < 2048 {
        let (st, r, dn) = vfs_call(4, ino as u32, off, 1024, &[], reply);
        if st != 0 || r == 0 {
            break;
        }
        let data_copy: [u8; 4096] = *reply;
        con_write(&data_copy[12..12 + dn]);
        off += r;
        shown += r;
    }
    con_write(b"\r\n");
    if shown < size {
        outln!("... ({} de {} bytes)", shown, size);
    }
}

fn cmd_write(rest: &[u8], reply: &mut [u8; 4096]) {
    let Some(sp) = rest.iter().position(|&c| c == b' ') else {
        outln!("uso: escreve <caminho> <texto>");
        return;
    };
    let (path, text) = (&rest[..sp], &rest[sp + 1..]);
    let (st, ino, _) = vfs_call(0, 0, 0, 0, path, reply);
    let ino = if st == 0 {
        ino as u32
    } else if st == 3 {
        let (st2, i, _) = vfs_call(1, 0, 0, 0, path, reply);
        if st2 != 0 {
            outln!("escreve: criar falhou ({})", st2);
            return;
        }
        i as u32
    } else {
        outln!("escreve: erro {}", st);
        return;
    };
    let (st, w, _) = vfs_call(5, ino, 0, 0, text, reply);
    if st != 0 {
        outln!("escreve: erro {}", st);
        return;
    }
    let _ = vfs_call(9, ino, text.len() as u64, 0, &[], reply);
    outln!("{} byte(s) escritos", w);
}

/// Comando não-builtin: tenta o utilitário `<nome>-c` do initrd com a convenção do Nexo —
/// h0 = o vfs DUPLICADO (o utilitário fala o mesmo `nexo.fs`; caminhos `/disk` `/boot`
/// `/tmp`), h1 = argv (uma mensagem, palavras separadas por `\0`), h2 = stdin vazio,
/// h3 = stdout por canal, bombeado para a console até o EOF (`\n` vira `\r\n`). O shell não
/// toca o vfs enquanto o filho roda — respostas iriam ao leitor errado. Devolve false se o
/// utilitário não existe (o chamador imprime o "comando desconhecido" de sempre).
fn cmd_run(name: &[u8], rest: &[u8]) -> bool {
    if name.len() > 32 {
        return false;
    }
    let mut nome = [0u8; 34];
    nome[..name.len()].copy_from_slice(name);
    nome[name.len()..name.len() + 2].copy_from_slice(b"-c");
    let Ok(nome_str) = core::str::from_utf8(&nome[..name.len() + 2]) else {
        return false;
    };
    let mut bloco = [0u8; 300];
    bloco[..name.len()].copy_from_slice(name);
    let mut bn = name.len();
    bloco[bn] = 0;
    bn += 1;
    for w in rest.split(|&c| c == b' ') {
        if w.is_empty() {
            continue;
        }
        if bn + w.len() + 1 > bloco.len() {
            return false;
        }
        bloco[bn..bn + w.len()].copy_from_slice(w);
        bn += w.len();
        bloco[bn] = 0;
        bn += 1;
    }
    let Ok((argv_tx, argv_rx)) = nexo_sys::channel_create() else {
        return false;
    };
    let _ = nexo_sys::channel_send(argv_tx, &bloco[..bn], &[]);
    nexo_sys::handle_close(argv_tx);
    let Ok((stdin_tx, stdin_rx)) = nexo_sys::channel_create() else {
        return false;
    };
    nexo_sys::handle_close(stdin_tx); // ponta fechada = EOF imediato
    let Ok((stdout_tx, stdout_rx)) = nexo_sys::channel_create() else {
        return false;
    };
    let Ok(h0) =
        nexo_sys::handle_info(VFS).and_then(|(rights, _)| nexo_sys::handle_duplicate(VFS, rights))
    else {
        return false;
    };
    let proc = match nexo_sys::process_spawn(nome_str, 0, &[h0, argv_rx, stdin_rx, stdout_tx]) {
        Ok(p) => p,
        Err(_) => {
            nexo_sys::handle_close(stdout_rx);
            return false;
        }
    };
    let mut buf = [0u8; 4096];
    let mut cru = [0u8; 1024];
    let mut hs = [0u32; 1];
    while let Ok((n, _)) = nexo_sys::channel_recv(stdout_rx, &mut cru, &mut hs) {
        let mut m = 0;
        for &c in &cru[..n] {
            if c == b'\n' {
                buf[m] = b'\r';
                m += 1;
            }
            buf[m] = c;
            m += 1;
        }
        if m > 0 {
            con_write(&buf[..m]);
        }
    }
    nexo_sys::handle_close(stdout_rx);
    let code = nexo_sys::process_wait(proc).unwrap_or(-1);
    nexo_sys::handle_close(proc);
    if code != 0 {
        outln!("{} saiu com {}", nome_str, code);
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut inbuf = [0u8; 4096];
    let mut reply = [0u8; 4096];
    let mut line = [0u8; 256];
    let mut len = 0usize;
    con_write(b"\r\nNexo OS - shell de diagnostico (digite 'ajuda')\r\n> ");
    loop {
        let n = con_poll(&mut inbuf);
        if n == 0 {
            nexo_sys::sleep_ns(20_000_000);
            continue;
        }
        for &c in &inbuf[..n] {
            match c {
                b'\r' | b'\n' => {
                    con_write(b"\r\n");
                    let cmd = &line[..len];
                    len = 0;
                    let (name, rest) = match cmd.iter().position(|&c| c == b' ') {
                        Some(i) => (&cmd[..i], &cmd[i + 1..]),
                        None => (cmd, &b""[..]),
                    };
                    match name {
                        b"" => {}
                        b"ajuda" => {
                            outln!(
                                "comandos: ajuda info tempo ls cat escreve remove eco sair (outro nome = utilitario do initrd)"
                            );
                        }
                        b"info" => cmd_info(),
                        b"tempo" => {
                            outln!("{} ms desde o boot", nexo_sys::time_now() / 1_000_000);
                        }
                        b"ls" => cmd_ls(rest, &mut reply),
                        b"cat" => cmd_cat(rest, &mut reply),
                        b"escreve" => cmd_write(rest, &mut reply),
                        b"remove" => {
                            let (st, _, _) = vfs_call(3, 0, 0, 0, rest, &mut reply);
                            if st == 0 {
                                outln!("removido");
                            } else {
                                outln!("remove: erro {}", st);
                            }
                        }
                        b"eco" => {
                            con_write(rest);
                            con_write(b"\r\n");
                        }
                        b"sair" => {
                            con_write(b"ate mais\r\n");
                            nexo_sys::exit(0)
                        }
                        _ => {
                            if !cmd_run(name, rest) {
                                outln!("comando desconhecido (tente 'ajuda')");
                            }
                        }
                    }
                    con_write(b"> ");
                }
                0x7f | 0x08 => {
                    if len > 0 {
                        len -= 1;
                        con_write(b"\x08 \x08");
                    }
                }
                c if len < line.len() && (0x20..0x7f).contains(&c) => {
                    line[len] = c;
                    len += 1;
                    con_write(&[c]);
                }
                _ => {}
            }
        }
    }
}

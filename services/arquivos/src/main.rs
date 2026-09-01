//! `arquivos` — gerenciador de arquivos (Plano §Fase 6: "criar gerenciador de arquivos"). MVP
//! de navegação: lista um diretório do `nexo.fs` (uma entrada por linha; diretórios em acento,
//! arquivos em branco), clique numa pasta **entra nela** e clique num arquivo pede ao
//! orquestrador que o abra ("abrir <caminho>") — o gerenciador não abre nada sozinho: quem
//! decide o app é quem tem as capacidades (o shell).
//! Handle 0 = canal do orquestrador: "sess", depois "abre <dir>" + canal `nexo.fs`; responde
//! "pronto"; navegação emite "pasta <dir>"; abertura emite "abrir <caminho>". Pipe fechado = sair.
#![no_std]
#![no_main]

use nexo_gfx::text::draw_glyph;
use nexo_gfx::{Color, PixelFormat, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const COLS: usize = 8;
const ROWS: usize = 6;
const W: i32 = (COLS * 8) as i32;
const H: i32 = (ROWS * 8) as i32;
const NAME_MAX: usize = 24;
const PATH_MAX: usize = 96;

fn fail(code: i64, what: &str) -> ! {
    log!("arquivos: falha: {}", what);
    nexo_sys::exit(code)
}

/// Uma entrada listada: nome + tipo (2 = diretório, como no `nexo.fs`).
#[derive(Clone, Copy)]
struct Entry {
    name: [u8; NAME_MAX],
    len: usize,
    dir: bool,
}

/// Cliente `nexo.fs`: só `list`.
struct Fs {
    ch: Handle,
    req: [u8; 4096],
    reply: [u8; 4096],
}

impl Fs {
    fn list(&mut self, path: &str, out: &mut [Entry; ROWS]) -> Option<usize> {
        use nexo_proto::fs as pfs;
        let mut p = [0u8; 256];
        let n = path.len().min(256);
        p[..n].copy_from_slice(&path.as_bytes()[..n]);
        let m = pfs::ListRequest {
            path: p,
            path_len: n as u32,
        }
        .encode_msg(&mut self.req)
        .ok()?;
        if nexo_sys::channel_send(self.ch, &self.req[..m], &[]) != Status::Ok {
            return None;
        }
        let mut hs = [0u32; 1];
        let (rn, _) = nexo_sys::channel_recv(self.ch, &mut self.reply, &mut hs).ok()?;
        let r = pfs::decode_list_response(&self.reply[..rn]).ok()?;
        let entries = r.entries();
        let mut count = 0usize;
        let mut pos = 0usize;
        while pos + 6 <= entries.len() && count < ROWS {
            let kind = entries[pos + 4];
            let nl = entries[pos + 5] as usize;
            if pos + 6 + nl > entries.len() {
                break;
            }
            let mut e = Entry {
                name: [0; NAME_MAX],
                len: nl.min(NAME_MAX),
                dir: kind == 2,
            };
            e.name[..e.len].copy_from_slice(&entries[pos + 6..pos + 6 + e.len]);
            out[count] = e;
            count += 1;
            pos += 6 + nl;
        }
        Some(count)
    }
}

/// Repinta a listagem: uma entrada por linha, diretórios em acento.
fn redraw(base: u64, entries: &[Entry; ROWS], count: usize) {
    // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
    let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
        .unwrap_or_else(|| fail(20, "superficie"));
    s.clear(Color::rgb(0x14, 0x15, 0x18));
    for (r, e) in entries[..count].iter().enumerate() {
        let color = if e.dir {
            Color::rgb(0x6f, 0x9f, 0xff)
        } else {
            Color::rgb(255, 255, 255)
        };
        for (c, &ch) in e.name[..e.len.min(COLS)].iter().enumerate() {
            draw_glyph(
                &mut s,
                ch as char,
                (c * 8) as i32,
                (r * 8) as i32,
                1,
                color,
                None,
            );
        }
    }
}

/// Junta `dir` + "/" + nome da entrada em `buf`; devolve o caminho.
fn join<'a>(dir: &str, e: &Entry, buf: &'a mut [u8; PATH_MAX]) -> &'a str {
    let mut pl = 0;
    buf[..dir.len()].copy_from_slice(dir.as_bytes());
    pl += dir.len();
    if !dir.ends_with('/') {
        buf[pl] = b'/';
        pl += 1;
    }
    buf[pl..pl + e.len].copy_from_slice(&e.name[..e.len]);
    pl += e.len;
    core::str::from_utf8(&buf[..pl]).unwrap_or("")
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut buf = [0u8; 384];
    let mut out = [0u8; 256];
    let mut hs = [0u32; 1];
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(21, "sessao nao recebida"),
    };
    let (dir_len, vfs): (usize, Handle) = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if n > 5 && &buf[..5] == b"abre " => (n - 5, hs[0]),
        _ => fail(22, "abre nao recebido"),
    };
    let mut dir = [0u8; PATH_MAX];
    let mut dl = dir_len.min(PATH_MAX);
    dir[..dl].copy_from_slice(&buf[5..5 + dl]);

    let mut fs = Fs {
        ch: vfs,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let mut entries = [Entry {
        name: [0; NAME_MAX],
        len: 0,
        dir: false,
    }; ROWS];
    let d = core::str::from_utf8(&dir[..dl]).unwrap_or_else(|_| fail(23, "dir"));
    let mut count = fs.list(d, &mut entries).unwrap_or_else(|| fail(24, "list"));

    let req = wm::CreateSurfaceRequest {
        x: 0,
        y: 0,
        w: W,
        h: H,
        z: 10,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(25, "enc create"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(26, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(sess, &mut buf, &mut hs).unwrap_or_else(|_| fail(27, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(28, "dec create"));
    if nh != 1 {
        fail(29, "sem handle");
    }
    let id = cs.id;
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(30, "map"));
    let mut title = wm::SetTitleRequest {
        id,
        title: [0; 32],
        title_len: 8,
    };
    title.title[..8].copy_from_slice(b"arquivos");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(31, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    redraw(base, &entries, count);
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(32, "enc commit"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    log!("arquivos: '{}' listado ({} entradas)", d, count);
    let _ = nexo_sys::channel_send(PIPE, b"pronto", &[]);

    loop {
        // cordão de vida
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
            Ok((n, _)) => {
                let Ok(ev) = wm::decode_pointer_event(&buf[..n]) else {
                    continue;
                };
                if ev.surface != id || ev.y < 0 {
                    continue;
                }
                let row = (ev.y / 8) as usize;
                if row >= count {
                    continue;
                }
                let e = entries[row];
                let d = core::str::from_utf8(&dir[..dl]).unwrap_or_else(|_| fail(33, "dir"));
                let mut pb = [0u8; PATH_MAX];
                let path = join(d, &e, &mut pb);
                if e.dir {
                    // navega: o diretório clicado vira o corrente
                    let nd = path.len().min(PATH_MAX);
                    let mut tmp = [0u8; PATH_MAX];
                    tmp[..nd].copy_from_slice(&path.as_bytes()[..nd]);
                    dir[..nd].copy_from_slice(&tmp[..nd]);
                    dl = nd;
                    let d = core::str::from_utf8(&dir[..dl]).unwrap_or_else(|_| fail(34, "dir"));
                    count = fs.list(d, &mut entries).unwrap_or_else(|| fail(35, "list"));
                    redraw(base, &entries, count);
                    let m = wm::CommitRequest { id }
                        .encode_msg(&mut out)
                        .unwrap_or_else(|_| fail(36, "enc commit2"));
                    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
                    let mut msg = [0u8; PATH_MAX + 6];
                    msg[..6].copy_from_slice(b"pasta ");
                    msg[6..6 + dl].copy_from_slice(&dir[..dl]);
                    let _ = nexo_sys::channel_send(PIPE, &msg[..6 + dl], &[]);
                } else {
                    // abrir e decisão do orquestrador: o gerenciador só aponta
                    let mut msg = [0u8; PATH_MAX + 6];
                    msg[..6].copy_from_slice(b"abrir ");
                    msg[6..6 + path.len()].copy_from_slice(path.as_bytes());
                    let _ = nexo_sys::channel_send(PIPE, &msg[..6 + path.len()], &[]);
                }
            }
            Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        let _ = nexo_sys::channel_wait_any(&[PIPE, sess]);
    }
}

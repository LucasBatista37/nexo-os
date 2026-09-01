//! `editor` — editor de texto (Plano §Fase 6: "criar editor de texto"). MVP honesto de notas:
//! abre um arquivo do `nexo.fs`, mostra o texto numa grade de glifos (`nexo-textgrid`, a mesma
//! do terminal) e edita **no fim do texto** — teclas imprimíveis acrescentam, backspace apaga,
//! Enter quebra linha; **F2 salva** (truncate + write no arquivo real). A janela é a prova: a
//! grade é uma função pura do texto, repintada a cada edição.
//! Handle 0 = canal do orquestrador: "sess" (sessão wm), depois "abre <caminho>" + canal
//! `nexo.fs`; responde "pronto"; a cada salvamento emite "salvo"; em "fecha" devolve o canal
//! do fs ("fs" + handle) e encerra — as capacidades voltam para quem as emprestou.
#![no_std]
#![no_main]

use nexo_gfx::text::draw_glyph;
use nexo_gfx::{Color, PixelFormat, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_textgrid::{Grid as TextGrid, evdev_char};

const PIPE: Handle = 0;
const COLS: usize = 8;
const ROWS: usize = 6;
const W: i32 = (COLS * 8) as i32;
const H: i32 = (ROWS * 8) as i32;
const TEXT_MAX: usize = 4096;
const KEY_F2: u32 = 60;

type Grid = TextGrid<COLS, ROWS>;

fn fail(code: i64, what: &str) -> ! {
    log!("editor: falha: {}", what);
    nexo_sys::exit(code)
}

/// Cliente `nexo.fs`: ler tudo, e salvar (truncate + write).
struct Fs {
    ch: Handle,
    req: [u8; 4096],
    reply: [u8; 4096],
}

impl Fs {
    fn rpc(&mut self, m: usize) -> Option<usize> {
        if nexo_sys::channel_send(self.ch, &self.req[..m], &[]) != Status::Ok {
            return None;
        }
        let mut hs = [0u32; 1];
        nexo_sys::channel_recv(self.ch, &mut self.reply, &mut hs)
            .ok()
            .map(|(n, _)| n)
    }

    fn stat(&mut self, path: &str) -> Option<(u32, usize)> {
        use nexo_proto::fs as pfs;
        let mut p = [0u8; 256];
        let n = path.len().min(256);
        p[..n].copy_from_slice(&path.as_bytes()[..n]);
        let m = pfs::StatRequest {
            path: p,
            path_len: n as u32,
        }
        .encode_msg(&mut self.req)
        .ok()?;
        let rn = self.rpc(m)?;
        let st = pfs::decode_stat_response(&self.reply[..rn]).ok()?;
        Some((st.ino, st.size as usize))
    }

    fn read_all(&mut self, ino: u32, size: usize, out: &mut [u8]) -> Option<usize> {
        use nexo_proto::fs as pfs;
        if size > out.len() {
            return None;
        }
        let mut off = 0usize;
        while off < size {
            let want = (size - off).min(3900) as u32;
            let m = pfs::ReadRequest {
                ino,
                offset: off as u64,
                len: want,
            }
            .encode_msg(&mut self.req)
            .ok()?;
            let rn = self.rpc(m)?;
            let r = pfs::decode_read_response(&self.reply[..rn]).ok()?;
            let dl = r.data().len();
            if dl == 0 {
                return None;
            }
            out[off..off + dl].copy_from_slice(r.data());
            off += dl;
        }
        Some(size)
    }

    fn save(&mut self, ino: u32, data: &[u8]) -> Option<()> {
        use nexo_proto::fs as pfs;
        let m = pfs::TruncateRequest { ino, size: 0 }
            .encode_msg(&mut self.req)
            .ok()?;
        let rn = self.rpc(m)?;
        pfs::decode_truncate_response(&self.reply[..rn]).ok()?;
        let mut off = 0usize;
        while off < data.len() {
            let n = (data.len() - off).min(3900);
            let mut rq = pfs::WriteRequest {
                ino,
                offset: off as u64,
                data: [0; 3900],
                data_len: n as u32,
            };
            rq.data[..n].copy_from_slice(&data[off..off + n]);
            let m = rq.encode_msg(&mut self.req).ok()?;
            let rn = self.rpc(m)?;
            let w = pfs::decode_write_response(&self.reply[..rn]).ok()?;
            if w.written as usize != n {
                return None;
            }
            off += n;
        }
        let m = pfs::TruncateRequest {
            ino,
            size: data.len() as u64,
        }
        .encode_msg(&mut self.req)
        .ok()?;
        let rn = self.rpc(m)?;
        pfs::decode_truncate_response(&self.reply[..rn]).ok()?;
        Some(())
    }
}

/// Repinta: a grade é reconstruída do texto (função pura), com um cursor de acento.
fn redraw(base: u64, text: &[u8]) {
    let mut grid = Grid::new();
    grid.feed_all(text);
    // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
    let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
        .unwrap_or_else(|| fail(20, "superficie"));
    s.clear(Color::rgb(0x14, 0x15, 0x18));
    for (r, row) in grid.cells.iter().enumerate() {
        for (c, &ch) in row.iter().enumerate() {
            if ch != b' ' {
                draw_glyph(
                    &mut s,
                    ch as char,
                    (c * 8) as i32,
                    (r * 8) as i32,
                    1,
                    Color::rgb(255, 255, 255),
                    None,
                );
            }
        }
    }
    // cursor: risco de acento sob a próxima célula
    s.fill_rect(
        nexo_gfx::Rect::new((grid.cx * 8) as i32, (grid.cy * 8 + 7) as i32, 8, 1),
        Color::rgb(0x6f, 0x9f, 0xff),
    );
}

static mut TEXT: [u8; TEXT_MAX] = [0; TEXT_MAX];

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut buf = [0u8; 384];
    let mut out = [0u8; 256];
    let mut hs = [0u32; 1];
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(21, "sessao nao recebida"),
    };
    let (path_len, vfs): (usize, Handle) = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if n > 5 && &buf[..5] == b"abre " => (n - 5, hs[0]),
        _ => fail(22, "abre nao recebido"),
    };
    let mut path = [0u8; 256];
    path[..path_len].copy_from_slice(&buf[5..5 + path_len]);
    let path = core::str::from_utf8(&path[..path_len]).unwrap_or_else(|_| fail(23, "caminho"));

    let mut fs = Fs {
        ch: vfs,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let (ino, size) = fs.stat(path).unwrap_or_else(|| fail(24, "stat"));
    // SAFETY: unico acesso, processo de uma so thread.
    let text = unsafe { &mut *core::ptr::addr_of_mut!(TEXT) };
    let mut len = fs
        .read_all(ino, size, text)
        .unwrap_or_else(|| fail(25, "read"));

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
        .unwrap_or_else(|_| fail(26, "enc create"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(27, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(sess, &mut buf, &mut hs).unwrap_or_else(|_| fail(28, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(29, "dec create"));
    if nh != 1 {
        fail(30, "sem handle");
    }
    let id = cs.id;
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(31, "map"));
    let mut title = wm::SetTitleRequest {
        id,
        title: [0; 32],
        title_len: 6,
    };
    title.title[..6].copy_from_slice(b"editor");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(32, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    redraw(base, &text[..len]);
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(33, "enc commit"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    log!("editor: '{}' aberto ({} bytes)", path, len);
    let _ = nexo_sys::channel_send(PIPE, b"pronto", &[]);

    loop {
        // orquestrador: "fecha" devolve o fs e encerra; pipe fechado = sair
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok((n, _)) if &buf[..n] == b"fecha" => {
                let _ = nexo_sys::channel_send(PIPE, b"fs", &[fs.ch]);
                nexo_sys::exit(0)
            }
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        // teclas: editar no fim do texto; F2 salva
        let mut dirty = false;
        loop {
            match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
                Ok((n, _)) => {
                    let Ok(k) = wm::decode_key_event(&buf[..n]) else {
                        continue;
                    };
                    if k.value != 1 {
                        continue;
                    }
                    if k.code == KEY_F2 {
                        fs.save(ino, &text[..len])
                            .unwrap_or_else(|| fail(34, "salvar"));
                        log!("editor: salvo ({} bytes)", len);
                        let _ = nexo_sys::channel_send(PIPE, b"salvo", &[]);
                        continue;
                    }
                    match evdev_char(k.code as u16) {
                        Some(0x08) => {
                            if len > 0 {
                                len -= 1;
                                dirty = true;
                            }
                        }
                        Some(ch) if len < TEXT_MAX => {
                            text[len] = ch;
                            len += 1;
                            dirty = true;
                        }
                        Some(_) => {}
                        None => {}
                    }
                }
                Err(Status::WouldBlock) => break,
                Err(_) => nexo_sys::exit(0),
            }
        }
        if dirty {
            redraw(base, &text[..len]);
            let m = wm::CommitRequest { id }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(35, "enc commit2"));
            let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
            // resposta do commit chega na próxima drenagem (eventos e respostas se toleram)
        }
        let _ = nexo_sys::channel_wait_any(&[PIPE, sess]);
    }
}

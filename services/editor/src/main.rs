//! `editor` — editor de texto (Plano §Fase 6: "criar editor de texto"). MVP honesto de notas:
//! abre um arquivo do `nexo.fs`, mostra o texto numa grade de glifos (`nexo-textgrid`, a mesma
//! do terminal) e edita com **cursor livre** — setas ESQ/DIR movem o cursor, imprimíveis
//! INSEREM na posição, backspace remove à esquerda, Enter quebra linha; **F2 salva**
//! (truncate + write no arquivo real); textos maiores que a janela **rolam** para o cursor
//! ficar sempre visível. A janela é a prova: a grade é uma função pura da tripla (texto,
//! cursor, primeira linha visível), repintada a cada edição.
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
const KEY_LEFT: u32 = 105;
const KEY_RIGHT: u32 = 106;

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

/// Linha ABSOLUTA do cursor (índice `cur` no texto), pelas mesmas regras de quebra da grade.
fn linha_do_cursor(text: &[u8], cur: usize) -> usize {
    let mut g = Grid::new();
    g.feed_all(&text[..cur.min(text.len())]);
    g.scrolled + g.cy
}

/// Ajusta a primeira linha visível para o cursor caber na janela (rolagem mínima).
fn enquadra(topo: usize, linha: usize) -> usize {
    if linha < topo {
        linha
    } else if linha >= topo + ROWS {
        linha + 1 - ROWS
    } else {
        topo
    }
}

/// Repinta a janela de ROWS linhas a partir da linha lógica `topo`: alimenta o texto até a
/// última linha visível, parando ANTES da quebra que abriria a linha `topo + ROWS` — assim a
/// grade rola exatamente `topo` vezes e mostra as linhas [topo, topo+ROWS). O cursor é
/// capturado como (coluna, linha ABSOLUTA) ao passar por `cur` e desenhado em linha − topo.
fn redraw(base: u64, text: &[u8], cur: usize, topo: usize) {
    let mut grid = Grid::new();
    let cur = cur.min(text.len());
    let mut cursor = (0usize, 0usize);
    let mut i = 0;
    loop {
        if i == cur {
            cursor = (grid.cx, grid.scrolled + grid.cy);
        }
        if i == text.len() {
            break;
        }
        let b = text[i];
        let ultima = grid.scrolled + grid.cy + 1 == topo + ROWS;
        let imprimivel = (0x20..=0x7e).contains(&b);
        if ultima && (b == b'\n' || (imprimivel && grid.cx + 1 == COLS)) {
            if imprimivel {
                grid.cells[grid.cy][grid.cx] = b; // a célula entra; a quebra ficaria fora
            }
            break;
        }
        grid.feed(b);
        i += 1;
    }
    let (ccx, ccy) = (cursor.0, cursor.1.saturating_sub(topo).min(ROWS - 1));
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
    // cursor: risco de acento sob a célula onde a próxima inserção cai
    s.fill_rect(
        nexo_gfx::Rect::new((ccx * 8) as i32, (ccy * 8 + 7) as i32, 8, 1),
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

    let mut cur = len; // cursor livre: comeca no fim do texto
    let mut topo = enquadra(0, linha_do_cursor(&text[..len], cur)); // rolagem: o fim fica visivel
    redraw(base, &text[..len], cur, topo);
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
                    if k.code == KEY_LEFT {
                        if cur > 0 {
                            cur -= 1;
                            dirty = true;
                        }
                        continue;
                    }
                    if k.code == KEY_RIGHT {
                        if cur < len {
                            cur += 1;
                            dirty = true;
                        }
                        continue;
                    }
                    match evdev_char(k.code as u16) {
                        Some(0x08) => {
                            if cur > 0 {
                                text.copy_within(cur..len, cur - 1);
                                cur -= 1;
                                len -= 1;
                                dirty = true;
                            }
                        }
                        Some(ch) if len < TEXT_MAX => {
                            text.copy_within(cur..len, cur + 1);
                            text[cur] = ch;
                            cur += 1;
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
            topo = enquadra(topo, linha_do_cursor(&text[..len], cur));
            redraw(base, &text[..len], cur, topo);
            let m = wm::CommitRequest { id }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(35, "enc commit2"));
            let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
            // resposta do commit chega na próxima drenagem (eventos e respostas se toleram)
        }
        let _ = nexo_sys::channel_wait_any(&[PIPE, sess]);
    }
}

//! `term` — terminal gráfico (Plano §Fase 6: "criar terminal e shell"). Uma janela que **serve**
//! o protocolo `nexo.console` v1.0: o shell de diagnóstico existente roda dentro dela sem mudar
//! uma linha — as escritas do shell viram texto numa grade de glifos 8×8 (`nexo-font`) e as
//! teclas que o compositor entrega à janela em foco viram a leitura da console. A mediação do
//! compositor vale aqui também: o shell só "ouve" o teclado enquanto o terminal tem o foco.
//! Handle 0 = canal do orquestrador (recebe "sess"; cordão de vida; emite "pronto").
//! Handle 1 = extremidade de console servida ao shell.
#![no_std]
#![no_main]

use nexo_gfx::text::draw_glyph;
use nexo_gfx::{Color, PixelFormat, Surface};
use nexo_proto::console::{self, ReadResponse, WriteResponse};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const CON: Handle = 1;
const COLS: usize = 8;
const ROWS: usize = 6;
const W: i32 = (COLS * 8) as i32;
const H: i32 = (ROWS * 8) as i32;

fn fail(code: i64, what: &str) -> ! {
    log!("term: falha: {}", what);
    nexo_sys::exit(code)
}

/// Grade de texto do terminal: quebra automática em `COLS`, `\r`/`\n`/backspace e rolagem
/// (a linha de baixo nasce limpa). É o "estado de tela" completo — a pintura é uma função pura
/// desta grade, o que mantém os pixels determinísticos e testáveis.
struct Grid {
    cells: [[u8; COLS]; ROWS],
    cx: usize,
    cy: usize,
}

impl Grid {
    fn new() -> Self {
        Grid {
            cells: [[b' '; COLS]; ROWS],
            cx: 0,
            cy: 0,
        }
    }
    fn newline(&mut self) {
        self.cy += 1;
        if self.cy == ROWS {
            self.cells.copy_within(1.., 0);
            self.cells[ROWS - 1] = [b' '; COLS];
            self.cy = ROWS - 1;
        }
    }
    fn feed(&mut self, b: u8) {
        match b {
            b'\r' => self.cx = 0,
            b'\n' => self.newline(),
            0x08 => self.cx = self.cx.saturating_sub(1),
            0x20..=0x7e => {
                self.cells[self.cy][self.cx] = b;
                self.cx += 1;
                if self.cx == COLS {
                    self.cx = 0;
                    self.newline();
                }
            }
            _ => {}
        }
    }
}

fn redraw(base: u64, grid: &Grid) {
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
}

/// Tradução mínima de scancodes evdev (pressão) para ASCII.
fn key_char(code: u16) -> Option<u8> {
    Some(match code {
        16..=25 => b"qwertyuiop"[code as usize - 16],
        30..=38 => b"asdfghjkl"[code as usize - 30],
        44..=50 => b"zxcvbnm"[code as usize - 44],
        2..=10 => b"123456789"[code as usize - 2],
        11 => b'0',
        57 => b' ',
        28 => b'\n',
        14 => 0x08,
        _ => return None,
    })
}

/// Envia o commit e espera a resposta tolerando eventos intercalados na sessão: teclas viram
/// entrada pendente (`keys`), ponteiro é ignorado, qualquer outra mensagem é a resposta.
fn commit_and_sync(sess: Handle, id: u32, keys: &mut ([u8; 64], usize)) {
    let mut out = [0u8; 128];
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(30, "enc commit"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(31, "send commit");
    }
    loop {
        let (n, _) = match nexo_sys::channel_recv(sess, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(_) => nexo_sys::exit(0),
        };
        if let Ok(k) = wm::decode_key_event(&buf[..n]) {
            stash_key(keys, k.code as u16, k.value);
            continue;
        }
        if wm::decode_pointer_event(&buf[..n]).is_ok() {
            continue;
        }
        return; // resposta do commit
    }
}

fn stash_key(keys: &mut ([u8; 64], usize), code: u16, value: u32) {
    if value != 1 {
        return;
    }
    if let Some(ch) = key_char(code)
        && keys.1 < keys.0.len()
    {
        keys.0[keys.1] = ch;
        keys.1 += 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut buf = [0u8; 4096];
    let mut out = [0u8; 4096];
    let mut hs = [0u32; 1];
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(21, "sessao nao recebida"),
    };

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
        .unwrap_or_else(|_| fail(22, "enc create"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(23, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(sess, &mut buf, &mut hs).unwrap_or_else(|_| fail(24, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(25, "dec create"));
    if nh != 1 {
        fail(26, "sem handle");
    }
    let id = cs.id;
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(27, "map"));
    let mut title = wm::SetTitleRequest {
        id,
        title: [0; 32],
        title_len: 8,
    };
    title.title[..8].copy_from_slice(b"terminal");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(28, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    let mut grid = Grid::new();
    let mut keys: ([u8; 64], usize) = ([0; 64], 0);
    redraw(base, &grid);
    commit_and_sync(sess, id, &mut keys);
    log!("term: pronto (janela id {}, grade {}x{})", id, COLS, ROWS);
    let _ = nexo_sys::channel_send(PIPE, b"pronto", &[]);

    loop {
        // cordão de vida do orquestrador
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        // teclas entregues pela sessão (só com foco — mediação do compositor)
        loop {
            match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
                Ok((n, _)) => {
                    if let Ok(k) = wm::decode_key_event(&buf[..n]) {
                        stash_key(&mut keys, k.code as u16, k.value);
                    }
                }
                Err(Status::WouldBlock) => break,
                Err(_) => nexo_sys::exit(0),
            }
        }
        // pedidos de console do shell
        let served = match nexo_sys::channel_try_recv(CON, &mut buf, &mut hs) {
            Ok((n, _)) => {
                match console::decode_request(&buf[..n]) {
                    Ok(console::Request::Write(w)) => {
                        for &b in w.data() {
                            grid.feed(b);
                        }
                        redraw(base, &grid);
                        commit_and_sync(sess, id, &mut keys);
                        let resp = WriteResponse {
                            written: w.data().len() as u32,
                        };
                        let m = resp
                            .encode_msg(&mut out)
                            .unwrap_or_else(|_| fail(32, "enc write resp"));
                        let _ = nexo_sys::channel_send(CON, &out[..m], &[]);
                    }
                    Ok(console::Request::Read(_)) => {
                        let mut resp = ReadResponse {
                            data: [0; 3500],
                            data_len: keys.1 as u32,
                        };
                        resp.data[..keys.1].copy_from_slice(&keys.0[..keys.1]);
                        keys.1 = 0;
                        let m = resp
                            .encode_msg(&mut out)
                            .unwrap_or_else(|_| fail(33, "enc read resp"));
                        let _ = nexo_sys::channel_send(CON, &out[..m], &[]);
                    }
                    Err(_) => {}
                }
                true
            }
            Err(Status::WouldBlock) => false,
            Err(_) => {
                // shell saiu: avisa o orquestrador (handshake de encerramento) e encerra
                let _ = nexo_sys::channel_send(PIPE, b"fim", &[]);
                nexo_sys::exit(0)
            }
        };
        if !served {
            let _ = nexo_sys::channel_wait_any(&[PIPE, sess, CON]);
        }
    }
}

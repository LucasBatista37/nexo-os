//! `calc` — calculadora gráfica (Plano §Fase 6: "criar calculadora, calendário e utilitários").
//! O primeiro aplicativo de verdade da plataforma: uma janela com um visor e botões (`nexo-ui`)
//! acionados pelos eventos `pointer` do compositor (clique em coordenadas locais). Suporta
//! `1`, `+`, `2` e `=` nesta versão; ao calcular, escreve o resultado no visor e no **clipboard
//! mediado** (permitido porque o clique deu o foco à calculadora).
//! Handle 0 = canal com o orquestrador: recebe a sessão `nexo.wm` ("sess") e avisa "eq" após o
//! cálculo (sincronização dos testes).
#![no_std]
#![no_main]

use nexo_gfx::{PixelFormat, Rect, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_ui::{Button, Label, Theme};

const PIPE: Handle = 0;
const W: i32 = 32;
const H: i32 = 24;
/// Rótulos dos botões, na ordem das células.
const KEYS: [&str; 4] = ["1", "+", "2", "="];

fn fail(code: i64, what: &str) -> ! {
    log!("calc: falha: {}", what);
    nexo_sys::exit(code)
}

/// Retângulo (local) do botão `k`.
fn key_rect(k: usize) -> Rect {
    Rect::new(1 + k as i32 * 8, 14, 7, 8)
}

/// Repinta a janela: visor com `text` e os quatro botões.
fn redraw(base: u64, theme: &Theme, text: &str) {
    // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
    let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
        .unwrap_or_else(|| fail(20, "superficie"));
    s.clear(theme.bg);
    s.fill_rect(Rect::new(1, 1, W - 2, 10), theme.surface);
    Label::new(text).draw(&mut s, 2, 2, theme);
    for (k, label) in KEYS.iter().enumerate() {
        Button::new(key_rect(k), label).draw(&mut s, theme);
    }
}

/// Formata `v` (>= 0) em ASCII; devolve a fatia válida.
fn itoa(v: i64, buf: &mut [u8; 20]) -> &[u8] {
    if v == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let mut n = v;
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    buf.copy_within(i.., 0);
    let len = 20 - i;
    &buf[..len]
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let theme = Theme::dark();
    let mut buf = [0u8; 384];
    let mut hs = [0u32; 1];
    // recebe a sessão nexo.wm do orquestrador
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(21, "sessao nao recebida"),
    };

    // cria a janela
    let mut out = [0u8; 384];
    let req = wm::CreateSurfaceRequest {
        x: 8,
        y: 8,
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
        title_len: 4,
    };
    title.title[..4].copy_from_slice(b"calc");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(28, "enc title"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(29, "send title");
    }
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    let commit = |out: &mut [u8; 384], buf: &mut [u8; 384], hs: &mut [u32; 1]| {
        let m = wm::CommitRequest { id }
            .encode_msg(out)
            .unwrap_or_else(|_| fail(30, "enc commit"));
        if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
            fail(31, "send commit");
        }
        // a resposta pode vir intercalada com eventos; consome ate achar uma resposta
        loop {
            match nexo_sys::channel_recv(sess, buf, hs) {
                Ok((n, _)) => {
                    if wm::decode_pointer_event(&buf[..n]).is_ok()
                        || wm::decode_key_event(&buf[..n]).is_ok()
                    {
                        continue; // cliques durante o repaint chegam depois pela fila do estado
                    }
                    break;
                }
                Err(_) => fail(32, "recv commit"),
            }
        }
    };
    redraw(base, &theme, "0");
    commit(&mut out, &mut buf, &mut hs);
    log!("calc: pronta (janela id {})", id);

    // estado da conta
    let mut acc: i64 = 0;
    let mut cur: i64 = 0;
    let mut has_op = false;
    loop {
        // o canal com o orquestrador é o cordão de vida do app: caiu, encerra
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        let (n, _) = match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(Status::WouldBlock) => {
                let _ = nexo_sys::channel_wait_any(&[sess, PIPE]);
                continue;
            }
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(33, "recv"),
        };
        let Ok(ev) = wm::decode_pointer_event(&buf[..n]) else {
            continue; // teclas e outros: ignora nesta versão
        };
        if ev.surface != id {
            continue;
        }
        let Some(k) = (0..KEYS.len()).find(|&k| key_rect(k).contains(ev.x, ev.y)) else {
            continue;
        };
        match KEYS[k] {
            "1" => cur = cur * 10 + 1,
            "2" => cur = cur * 10 + 2,
            "+" => {
                acc = cur;
                cur = 0;
                has_op = true;
            }
            _ => {
                // "=": calcula, mostra e poe no clipboard (temos o foco: o clique veio para cá)
                let res = if has_op { acc + cur } else { cur };
                acc = 0;
                cur = res;
                has_op = false;
                let mut nbuf = [0u8; 20];
                let txt = itoa(res, &mut nbuf);
                let mut show = [0u8; 20];
                show[..txt.len()].copy_from_slice(txt);
                let s = core::str::from_utf8(&show[..txt.len()]).unwrap_or("?");
                redraw(base, &theme, s);
                commit(&mut out, &mut buf, &mut hs);
                let mut rq = wm::ClipboardSetRequest {
                    data: [0; 256],
                    data_len: txt.len() as u32,
                };
                rq.data[..txt.len()].copy_from_slice(txt);
                let m = rq
                    .encode_msg(&mut out)
                    .unwrap_or_else(|_| fail(34, "enc clip"));
                if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
                    fail(35, "send clip");
                }
                let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
                log!("calc: resultado {} no visor e no clipboard", s);
                let _ = nexo_sys::channel_send(PIPE, b"eq", &[]);
                continue;
            }
        }
        let mut nbuf = [0u8; 20];
        let txt = itoa(cur, &mut nbuf);
        let mut show = [0u8; 20];
        show[..txt.len()].copy_from_slice(txt);
        redraw(
            base,
            &theme,
            core::str::from_utf8(&show[..txt.len()]).unwrap_or("?"),
        );
        commit(&mut out, &mut buf, &mut hs);
    }
}

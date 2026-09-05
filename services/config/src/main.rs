//! `config` — Configurações (Plano §Fase 6: "criar configurações"). Janela com três *toggles*
//! reais: **movimento reduzido** (`set_reduce_motion`), **não-perturbe** (`set_dnd`) e **tema**
//! (`set_theme`: escuro/claro — a própria janela repinta com o Theme novo na hora). O clique
//! que aciona o toggle é o que dá o foco à janela — e a posse da entrada é exatamente o que as
//! APIs mediadas exigem: a mediação do compositor trabalhando a favor do app.
//! Handle 0 = canal do orquestrador (recebe "sess"; cordão de vida; emite "pronto" e o estado
//! de cada toggle — "rm1"/"rm0"/"np1"/"np0"/"tm1"/"tm0" — para sincronização).
#![no_std]
#![no_main]

use nexo_gfx::{PixelFormat, Rect, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_ui::{Button, ButtonState, Theme};

const PIPE: Handle = 0;
const W: i32 = 32;
const H: i32 = 24;

fn fail(code: i64, what: &str) -> ! {
    log!("config: falha: {}", what);
    nexo_sys::exit(code)
}

/// Retângulos (locais) dos toggles.
fn rm_rect() -> Rect {
    Rect::new(1, 4, 14, 8)
}
fn np_rect() -> Rect {
    Rect::new(17, 4, 14, 8)
}
fn tm_rect() -> Rect {
    Rect::new(1, 14, 30, 8)
}

/// Repinta: cada toggle "pressionado" quando ligado.
fn redraw(base: u64, theme: &Theme, rm: bool, np: bool, tm: bool) {
    // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
    let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
        .unwrap_or_else(|| fail(20, "superficie"));
    s.clear(theme.bg);
    let mut b = Button::new(rm_rect(), "RM");
    b.state = if rm {
        ButtonState::Pressed
    } else {
        ButtonState::Normal
    };
    b.draw(&mut s, theme);
    let mut b = Button::new(np_rect(), "NP");
    b.state = if np {
        ButtonState::Pressed
    } else {
        ButtonState::Normal
    };
    b.draw(&mut s, theme);
    let mut b = Button::new(tm_rect(), "TM");
    b.state = if tm {
        ButtonState::Pressed
    } else {
        ButtonState::Normal
    };
    b.draw(&mut s, theme);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut theme = Theme::dark();
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(21, "sessao nao recebida"),
    };

    let mut out = [0u8; 256];
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
        title_len: 6,
    };
    title.title[..6].copy_from_slice(b"config");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(28, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    let mut rm = false;
    let mut np = false;
    let mut tm = false;
    redraw(base, &theme, rm, np, tm);
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(29, "enc commit"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    log!("config: pronta (janela id {})", id);
    let _ = nexo_sys::channel_send(PIPE, b"pronto", &[]);

    loop {
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0), // cordão de vida
        }
        let (n, _) = match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(Status::WouldBlock) => {
                let _ = nexo_sys::channel_wait_any(&[sess, PIPE]);
                continue;
            }
            Err(_) => nexo_sys::exit(0),
        };
        let Ok(ev) = wm::decode_pointer_event(&buf[..n]) else {
            continue;
        };
        if ev.surface != id {
            continue;
        }
        // o clique deu o foco a esta janela: as APIs mediadas estão liberadas
        if rm_rect().contains(ev.x, ev.y) {
            rm = !rm;
            let m = wm::SetReduceMotionRequest { enabled: rm as u8 }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(30, "enc rm"));
            if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
                fail(31, "send rm");
            }
            let ok = matches!(nexo_sys::channel_recv(sess, &mut buf, &mut hs),
                Ok((n, _)) if wm::decode_set_reduce_motion_response(&buf[..n]).is_ok());
            if !ok {
                fail(32, "rm recusado");
            }
            let _ = nexo_sys::channel_send(PIPE, if rm { b"rm1" } else { b"rm0" }, &[]);
        } else if np_rect().contains(ev.x, ev.y) {
            np = !np;
            let m = wm::SetDndRequest { enabled: np as u8 }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(33, "enc np"));
            if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
                fail(34, "send np");
            }
            let ok = matches!(nexo_sys::channel_recv(sess, &mut buf, &mut hs),
                Ok((n, _)) if wm::decode_set_dnd_response(&buf[..n]).is_ok());
            if !ok {
                fail(35, "np recusado");
            }
            let _ = nexo_sys::channel_send(PIPE, if np { b"np1" } else { b"np0" }, &[]);
        } else if tm_rect().contains(ev.x, ev.y) {
            tm = !tm;
            let m = wm::SetThemeRequest { theme: tm as u8 }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(37, "enc tm"));
            if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
                fail(38, "send tm");
            }
            let ok = matches!(nexo_sys::channel_recv(sess, &mut buf, &mut hs),
                Ok((n, _)) if wm::decode_set_theme_response(&buf[..n]).is_ok());
            if !ok {
                fail(39, "tm recusado");
            }
            // o tema do sistema mudou: esta janela repinta com o Theme novo na hora
            theme = if tm { Theme::light() } else { Theme::dark() };
            let _ = nexo_sys::channel_send(PIPE, if tm { b"tm1" } else { b"tm0" }, &[]);
        } else {
            continue;
        }
        redraw(base, &theme, rm, np, tm);
        let m = wm::CommitRequest { id }
            .encode_msg(&mut out)
            .unwrap_or_else(|_| fail(36, "enc commit2"));
        let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
        let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    }
}

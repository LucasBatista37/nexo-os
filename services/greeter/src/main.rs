//! `greeter` — tela de login/bloqueio (Plano §Fase 5: "criar login, bloqueio e sessão").
//! Handle 0 = canal com o orquestrador da sessão (recebe a sessão `nexo.wm` e reporta o estado).
//! Cria uma superfície de login em tela cheia (pintada com `nexo-ui`), **captura** a entrada
//! (`grab` — nenhuma outra janela recebe as teclas da senha nem rouba o foco por clique) e lê a
//! senha pelos eventos `key` do compositor. Senha errada: reporta e continua bloqueado. Senha
//! certa: solta a captura, destrói a tela de login e reporta o desbloqueio.
//! Nesta versão a credencial é fixa ("nexo" + Enter); armazenamento seguro de credenciais e
//! gestão de sessão/estado vêm com o modelo de usuários (Fase 6).
#![no_std]
#![no_main]

use nexo_gfx::{PixelFormat, Rect, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_ui::{Label, Theme};

const PIPE: Handle = 0;
/// Senha demo: "nexo" em códigos evdev (n, e, x, o) + Enter.
const PASSWORD: [u32; 4] = [49, 18, 45, 24];
const KEY_ENTER: u32 = 28;
const W: i32 = 64;
const H: i32 = 48;

fn fail(code: i64, what: &str) -> ! {
    log!("greeter: falha: {}", what);
    nexo_sys::exit(code)
}

/// RPC simples na sessão do wm (envia `msg`, devolve a resposta em `buf`).
fn rpc(sess: Handle, msg: &[u8], extra: &[u32], buf: &mut [u8]) -> (usize, usize) {
    if nexo_sys::channel_send(sess, msg, extra) != Status::Ok {
        fail(30, "send rpc");
    }
    let mut hs = [0u32; 1];
    match nexo_sys::channel_recv(sess, buf, &mut hs) {
        Ok((n, nh)) => {
            if nh == 1 {
                buf[..0].fill(0); // handles chegam via hs; o chamador refaz o recv se precisar
            }
            (n, hs[0] as usize)
        }
        Err(_) => fail(31, "recv rpc"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    // Recebe a sessão nexo.wm do orquestrador.
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(32, "sessao nao recebida"),
    };

    // Tela de login em tela cheia, acima de tudo.
    let mut out = [0u8; 128];
    let req = wm::CreateSurfaceRequest {
        x: 0,
        y: 0,
        w: W,
        h: H,
        z: 1000,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(33, "enc create"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(34, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(sess, &mut buf, &mut hs).unwrap_or_else(|_| fail(35, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(36, "dec create"));
    if nh != 1 {
        fail(37, "sem handle da superficie");
    }
    let id = cs.id;
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(38, "map superficie"));

    // Pinta a tela de bloqueio (tema escuro + rótulo) com o toolkit.
    let theme = Theme::dark();
    {
        // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
        let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
        let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
            .unwrap_or_else(|| fail(39, "superficie"));
        s.clear(theme.bg);
        s.stroke_rect(Rect::new(0, 0, W, H), theme.accent);
        Label::new("senha").draw(&mut s, 12, 20, &theme);
    }
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(40, "enc commit"));
    let _ = rpc(sess, &out[..m], &[], &mut buf);

    // Captura a entrada: a partir daqui a senha não pode ser roubada.
    let m = wm::GrabRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(41, "enc grab"));
    let (n, _) = rpc(sess, &out[..m], &[], &mut buf);
    if wm::decode_grab_response(&buf[..n]).is_err() {
        fail(42, "grab recusado");
    }
    log!("greeter: bloqueado — aguardando a senha (captura ativa)");
    if nexo_sys::channel_send(PIPE, b"locked", &[]) != Status::Ok {
        fail(43, "send locked");
    }

    // Lê a senha pelos eventos `key` da captura.
    let mut typed = [0u32; 16];
    let mut len = 0usize;
    loop {
        let (n, _) = match nexo_sys::channel_recv(sess, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(_) => fail(44, "recv teclas"),
        };
        let Ok(ev) = wm::decode_key_event(&buf[..n]) else {
            continue;
        };
        if ev.value != 1 {
            continue;
        }
        if ev.code == KEY_ENTER {
            let ok = len == PASSWORD.len() && typed[..len] == PASSWORD;
            if ok {
                break;
            }
            log!(
                "greeter: senha incorreta ({} tecla(s)); continua bloqueado",
                len
            );
            len = 0;
            if nexo_sys::channel_send(PIPE, b"wrong", &[]) != Status::Ok {
                fail(45, "send wrong");
            }
            continue;
        }
        if len < typed.len() {
            typed[len] = ev.code;
            len += 1;
        }
    }

    // Senha certa: solta a captura, remove a tela de login e devolve a entrada à sessão.
    let m = wm::UngrabRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(46, "enc ungrab"));
    let _ = rpc(sess, &out[..m], &[], &mut buf);
    let m = wm::DestroyRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(47, "enc destroy"));
    let _ = rpc(sess, &out[..m], &[], &mut buf);
    log!("greeter: sessao desbloqueada — entrada devolvida");
    if nexo_sys::channel_send(PIPE, b"unlocked", &[]) != Status::Ok {
        fail(48, "send unlocked");
    }
    nexo_sys::exit(0)
}

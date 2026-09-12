//! `greeter` — tela de login/bloqueio (Plano §Fase 5: "criar login, bloqueio e sessão").
//! Handle 0 = canal com o orquestrador da sessão (recebe a sessão `nexo.wm` e reporta o estado).
//! Cria uma superfície de login em tela cheia (pintada com `nexo-ui`), **captura** a entrada
//! (`grab` — nenhuma outra janela recebe as teclas da senha nem rouba o foco por clique) e lê a
//! senha pelos eventos `key` do compositor. Senha errada: reporta e continua bloqueado. Senha
//! certa: solta a captura, destrói a tela de login e reporta o desbloqueio.
//! Handle 0 pode trazer antes um canal `nexo.fs` ("fs"): carrega as preferências do sistema
//! pelo compositor e, na PRIMEIRA execução (sem `/disk/prefs.txt`), pergunta o idioma (1/2)
//! e guarda-o — o começo do onboarding.
//! Nesta versão a credencial é fixa ("nexo" + Enter); armazenamento seguro de credenciais e
//! gestão de sessão/estado vêm com o modelo de usuários (Fase 6).
#![no_std]
#![no_main]

use nexo_gfx::{PixelFormat, Rect, Surface};
use nexo_i18n::{Idioma, texto};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_ui::{Label, Theme};

const PIPE: Handle = 0;
/// Senha demo: "nexo" em códigos evdev (n, e, x, o) + Enter.
const PASSWORD: [u32; 4] = [49, 18, 45, 24];
const KEY_ENTER: u32 = 28;
/// Teclas `1` e `2` (evdev): a escolha do idioma na primeira execução.
const KEY_1: u32 = 2;
const KEY_2: u32 = 3;
/// Onde o compositor guarda as preferências (o mesmo caminho do `wm`): existir ou não é o
/// que distingue a primeira execução de todas as outras.
const PREFS_PATH: &[u8] = b"/disk/prefs.txt";
const W: i32 = 64;
const H: i32 = 48;

fn fail(code: i64, what: &str) -> ! {
    log!("greeter: falha: {}", what);
    nexo_sys::exit(code)
}

/// RPC na sessão do wm (envia `msg`, devolve a resposta em `buf` e o handle que ela trouxer).
/// Tolerante a eventos: com a captura ativa, as teclas chegam por este mesmo canal — o
/// key-up do "2" da escolha do idioma chegava no meio do `prefs_save` e era lido como a
/// resposta ("NAO guardado" sem recusa nenhuma; a resposta real baralhava o rpc seguinte).
fn rpc(sess: Handle, msg: &[u8], extra: &[u32], buf: &mut [u8]) -> (usize, usize) {
    if nexo_sys::channel_send(sess, msg, extra) != Status::Ok {
        fail(30, "send rpc");
    }
    let mut hs = [0u32; 1];
    loop {
        match nexo_sys::channel_recv(sess, buf, &mut hs) {
            Ok((n, nh)) => {
                if wm::decode_key_event(&buf[..n]).is_ok()
                    || wm::decode_pointer_event(&buf[..n]).is_ok()
                {
                    continue;
                }
                return (n, if nh == 1 { hs[0] as usize } else { usize::MAX });
            }
            Err(_) => fail(31, "recv rpc"),
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    // Recebe do orquestrador, opcionalmente, um canal `nexo.fs` ("fs" + handle) — para
    // carregar as preferências do sistema e, na primeira execução, guardar o idioma — e
    // depois a sessão nexo.wm ("sess" + handle).
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];
    let mut fs: Option<Handle> = None;
    let sess: Handle = loop {
        match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
            Ok((n, 1)) if &buf[..n] == b"sess" => break hs[0],
            Ok((n, 1)) if &buf[..n] == b"fs" => fs = Some(hs[0]),
            _ => fail(32, "sessao nao recebida"),
        }
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

    // Captura a entrada já aqui: a partir daqui a senha não pode ser roubada — e a posse da
    // entrada é o que o compositor exige para carregar/guardar preferências e mudar o idioma.
    let m = wm::GrabRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(41, "enc grab"));
    let (n, _) = rpc(sess, &out[..m], &[], &mut buf);
    if wm::decode_grab_response(&buf[..n]).is_err() {
        fail(42, "grab recusado");
    }

    // Preferências do sistema: carregadas do disco pelo compositor com o `nexo.fs` que o
    // orquestrador emprestou (o handle vai e volta). Sem arquivo é a PRIMEIRA EXECUÇÃO:
    // pergunta-se o idioma antes de mais nada, e a escolha é guardada para os boots
    // seguintes — o começo do onboarding (Plano §Fase 5).
    let theme = Theme::dark();
    if let Some(fs_h) = fs {
        let m = wm::PrefsLoadRequest { fs: fs_h }
            .encode_msg(&mut out)
            .unwrap_or_else(|_| fail(50, "enc prefs_load"));
        let (n, devolvido) = rpc(sess, &out[..m], &[fs_h], &mut buf);
        if devolvido == usize::MAX {
            fail(58, "prefs_load nao devolveu o fs");
        }
        let fs_h = devolvido as Handle; // o empréstimo volta (mesmo em recusa)
        if wm::decode_prefs_load_response(&buf[..n]).is_err() {
            log!("greeter: compositor recusou carregar preferencias; padroes em vigor");
        }
        let existe = {
            use nexo_proto::fs::{StatRequest, decode_stat_response};
            let mut path = [0u8; 256];
            path[..PREFS_PATH.len()].copy_from_slice(PREFS_PATH);
            let mut req = [0u8; 4096];
            let m = StatRequest {
                path,
                path_len: PREFS_PATH.len() as u32,
            }
            .encode_msg(&mut req)
            .unwrap_or_else(|_| fail(51, "enc stat"));
            if nexo_sys::channel_send(fs_h, &req[..m], &[]) != Status::Ok {
                fail(52, "send stat");
            }
            let mut hs2 = [0u32; 1];
            match nexo_sys::channel_recv(fs_h, &mut req, &mut hs2) {
                Ok((n, _)) => decode_stat_response(&req[..n]).is_ok(),
                Err(_) => fail(53, "recv stat"),
            }
        };
        if !existe {
            log!("greeter: primeira execucao — a perguntar o idioma (1 pt-BR, 2 en-US)");
            {
                // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) aqui.
                let px = unsafe {
                    core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize)
                };
                let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
                    .unwrap_or_else(|| fail(39, "superficie"));
                s.clear(theme.bg);
                s.stroke_rect(Rect::new(0, 0, W, H), theme.accent);
                Label::new(texto(Idioma::PtBr, "greeter.idioma")).draw(&mut s, 4, 20, &theme);
            }
            let m = wm::CommitRequest { id }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(40, "enc commit"));
            let _ = rpc(sess, &out[..m], &[], &mut buf);
            let escolha = loop {
                let (n, _) = match nexo_sys::channel_recv(sess, &mut buf, &mut hs) {
                    Ok(v) => v,
                    Err(_) => fail(54, "recv teclas do idioma"),
                };
                match wm::decode_key_event(&buf[..n]) {
                    Ok(ev) if ev.value == 1 && ev.code == KEY_1 => break 0u8,
                    Ok(ev) if ev.value == 1 && ev.code == KEY_2 => break 1u8,
                    _ => continue,
                }
            };
            let m = wm::SetIdiomaRequest { idioma: escolha }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(55, "enc set_idioma"));
            let (n, _) = rpc(sess, &out[..m], &[], &mut buf);
            if wm::decode_set_idioma_response(&buf[..n]).is_err() {
                fail(56, "set_idioma recusado");
            }
            let m = wm::PrefsSaveRequest { fs: fs_h }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(57, "enc prefs_save"));
            let (n, devolvido) = rpc(sess, &out[..m], &[fs_h], &mut buf);
            let guardado = wm::decode_prefs_save_response(&buf[..n]).is_ok();
            if devolvido != usize::MAX {
                let _ = nexo_sys::handle_close(devolvido as Handle);
            }
            log!(
                "greeter: idioma escolhido: {} ({})",
                if escolha == 1 { "en-US" } else { "pt-BR" },
                if guardado {
                    "guardado"
                } else {
                    "NAO guardado — voltara a perguntar"
                }
            );
        } else {
            let _ = nexo_sys::handle_close(fs_h);
        }
    }

    // Idioma da interface: preferência do sistema, lida do compositor (`nexo.wm` v1.22). É a
    // mesma via do tema — quem decide é o sistema, não cada aplicativo.
    let idioma = {
        let m = wm::PrefsRequest {}
            .encode_msg(&mut out)
            .unwrap_or_else(|_| fail(41, "enc prefs"));
        let (n, _) = rpc(sess, &out[..m], &[], &mut buf);
        match wm::decode_prefs_response(&buf[..n]) {
            Ok(p) => {
                if p.idioma == 1 {
                    Idioma::EnUs
                } else {
                    Idioma::PtBr
                }
            }
            // Sem preferência legível, o sistema fala português — nunca fica sem rótulo.
            Err(_) => Idioma::PtBr,
        }
    };
    log!("greeter: idioma da interface: {}", idioma.codigo());

    // Pinta a tela de bloqueio (tema escuro + rótulo) com o toolkit.
    {
        // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
        let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
        let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
            .unwrap_or_else(|| fail(39, "superficie"));
        s.clear(theme.bg);
        s.stroke_rect(Rect::new(0, 0, W, H), theme.accent);
        // Rótulo do catálogo, no idioma do sistema (escolhido na primeira execução e
        // guardado nas preferências do compositor).
        Label::new(texto(idioma, "greeter.senha")).draw(&mut s, 12, 20, &theme);
    }
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(40, "enc commit"));
    let _ = rpc(sess, &out[..m], &[], &mut buf);
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

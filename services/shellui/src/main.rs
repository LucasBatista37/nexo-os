//! `shellui` — o shell gráfico (Plano §Fase 5: "Faixa de Atividades"). Handle 0 = a sessão
//! **bootstrap** do `nexo.wm` (privilégio de shell), handle 1 = canal com os apps/orquestrador.
//! Desenha a **Faixa de Atividades**: uma barra no rodapé do display 0 com uma célula por janela
//! (via `surface_info`); o clique numa célula chega como evento `pointer` e o shell **ativa** a
//! janela (`activate`: Contexto + frente + foco). O clique na **zona direita** da barra abre e
//! fecha a **Central de Ações**: um painel que lista as notificações do registro do compositor
//! (inclusive as suprimidas pelo não-perturbe). Também faz *broker* de sessões: um app pede
//! "sess" pelo canal e recebe a ponta de uma sessão `nexo.wm` nova.
#![no_std]
#![no_main]

use nexo_gfx::{PixelFormat, Rect, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_ui::Theme;

const WM: Handle = 0;
const PIPE: Handle = 1;
/// Geometria da barra (rodapé do display 0 de 64×48).
const BAR_X: i32 = 0;
const BAR_Y: i32 = 38;
const BAR_W: i32 = 64;
const BAR_H: i32 = 10;
/// Células: `x = 2 + k*14`, 12×6 a partir de y local 2.
const CELL_X0: i32 = 2;
const CELL_STEP: i32 = 14;
const CELL_W: i32 = 12;
const CELL_Y: i32 = 2;
const CELL_H: i32 = 6;
/// Zona direita da barra: abre/fecha a Central de Ações.
const CENTER_ZONE_X: i32 = 50;
/// Painel da Central (na tela).
const PANEL_X: i32 = 16;
const PANEL_Y: i32 = 8;
const PANEL_W: i32 = 40;
const PANEL_H: i32 = 28;

fn fail(code: i64, what: &str) -> ! {
    log!("shellui: falha: {}", what);
    nexo_sys::exit(code)
}

/// RPC na sessão do wm tolerante a eventos: `pointer` intercalado vai para `pending` (1 slot; um
/// clique por vez basta para a barra), teclas são ignoradas.
fn rpc(
    msg: &[u8],
    extra: &[u32],
    buf: &mut [u8],
    hs: &mut [u32; 1],
    pending: &mut Option<wm::PointerEvent>,
) -> (usize, usize) {
    if nexo_sys::channel_send(WM, msg, extra) != Status::Ok {
        fail(60, "send rpc");
    }
    loop {
        let (n, nh) = match nexo_sys::channel_recv(WM, buf, hs) {
            Ok(v) => v,
            Err(_) => fail(61, "recv rpc"),
        };
        if let Ok(ev) = wm::decode_pointer_event(&buf[..n]) {
            *pending = Some(ev);
            continue;
        }
        if wm::decode_key_event(&buf[..n]).is_ok() {
            continue;
        }
        return (n, nh);
    }
}

/// Painel da Central de Ações (aberto/fechado).
struct Panel {
    id: u32,
}

/// O que um clique na barra pediu.
enum BarAction {
    None,
    Activated,
    ToggleCenter,
}

struct Bar {
    id: u32,
    base: u64,
    /// Janela de cada célula, na ordem desenhada.
    cells: [u32; 8],
    cell_n: usize,
}

/// Redesenha a barra: fundo + uma célula por janela (exceto a própria barra) e commit.
fn redraw(
    bar: &mut Bar,
    theme: &Theme,
    buf: &mut [u8],
    hs: &mut [u32; 1],
    pending: &mut Option<wm::PointerEvent>,
) {
    // enumera as janelas pelo privilégio de shell
    bar.cell_n = 0;
    let mut out = [0u8; 128];
    for idx in 0..8u32 {
        let m = wm::SurfaceInfoRequest { index: idx }
            .encode_msg(&mut out)
            .unwrap_or_else(|_| fail(62, "enc info"));
        let (n, _) = rpc(&out[..m], &[], buf, hs, pending);
        let info = wm::decode_surface_info_response(&buf[..n]).unwrap_or_else(|_| fail(63, "info"));
        if info.used == 1 && info.id != bar.id && bar.cell_n < bar.cells.len() {
            bar.cells[bar.cell_n] = info.id;
            bar.cell_n += 1;
        }
    }
    // pinta
    {
        // SAFETY: base .. base+BAR_W*BAR_H*4 foi mapeada por memory_map (USER|RW) neste processo.
        let px = unsafe {
            core::slice::from_raw_parts_mut(bar.base as *mut u8, (BAR_W * BAR_H * 4) as usize)
        };
        let mut s = Surface::new(
            px,
            BAR_W as u32,
            BAR_H as u32,
            BAR_W as u32,
            PixelFormat::Rgbx8888,
        )
        .unwrap_or_else(|| fail(64, "superficie da barra"));
        s.clear(theme.surface);
        for k in 0..bar.cell_n {
            s.fill_rect(
                Rect::new(CELL_X0 + k as i32 * CELL_STEP, CELL_Y, CELL_W, CELL_H),
                theme.accent,
            );
        }
    }
    let m = wm::CommitRequest { id: bar.id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(65, "enc commit"));
    let _ = rpc(&out[..m], &[], buf, hs, pending);
}

/// Clique na barra (coordenadas locais): ativa a janela da célula atingida, ou alterna a Central
/// de Ações na zona direita.
fn bar_click(
    bar: &Bar,
    x: i32,
    y: i32,
    buf: &mut [u8],
    hs: &mut [u32; 1],
    pending: &mut Option<wm::PointerEvent>,
) -> BarAction {
    if x >= CENTER_ZONE_X {
        return BarAction::ToggleCenter;
    }
    if !(CELL_Y..CELL_Y + CELL_H).contains(&y) {
        return BarAction::None;
    }
    for k in 0..bar.cell_n {
        let cx = CELL_X0 + k as i32 * CELL_STEP;
        if x >= cx && x < cx + CELL_W {
            let mut out = [0u8; 64];
            let m = wm::ActivateRequest { id: bar.cells[k] }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(66, "enc activate"));
            let (n, _) = rpc(&out[..m], &[], buf, hs, pending);
            if wm::decode_activate_response(&buf[..n]).is_ok() {
                return BarAction::Activated;
            }
            return BarAction::None;
        }
    }
    BarAction::None
}

/// Abre a Central de Ações: cria o painel, pinta fundo/borda e um marcador (bullet) por
/// notificação do registro do compositor, e commita.
fn open_center(
    theme: &Theme,
    buf: &mut [u8],
    hs: &mut [u32; 1],
    pending: &mut Option<wm::PointerEvent>,
) -> Panel {
    let mut out = [0u8; 128];
    let req = wm::CreateSurfaceRequest {
        x: PANEL_X,
        y: PANEL_Y,
        w: PANEL_W,
        h: PANEL_H,
        z: 6000,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(81, "enc painel"));
    if nexo_sys::channel_send(WM, &out[..m], &[]) != Status::Ok {
        fail(82, "send painel");
    }
    // resposta com handle: recebida fora do rpc (que não devolve handles), pulando eventos
    let (id, base) = loop {
        let (n, nh) = match nexo_sys::channel_recv(WM, buf, hs) {
            Ok(v) => v,
            Err(_) => fail(83, "recv painel"),
        };
        if let Ok(ev) = wm::decode_pointer_event(&buf[..n]) {
            *pending = Some(ev);
            continue;
        }
        if wm::decode_key_event(&buf[..n]).is_ok() {
            continue;
        }
        let cs = wm::decode_create_surface_response(&buf[..n])
            .unwrap_or_else(|_| fail(84, "dec painel"));
        if nh != 1 {
            fail(85, "sem handle do painel");
        }
        break (
            cs.id,
            nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(86, "map painel")),
        );
    };
    // lista as notificações (até 3) pelo privilégio de shell
    let mut bullets = 0usize;
    for idx in 0..3u32 {
        let m = wm::NotificationInfoRequest { index: idx }
            .encode_msg(&mut out)
            .unwrap_or_else(|_| fail(87, "enc ninfo"));
        let (n, _) = rpc(&out[..m], &[], buf, hs, pending);
        let info =
            wm::decode_notification_info_response(&buf[..n]).unwrap_or_else(|_| fail(88, "ninfo"));
        if info.used == 1 {
            bullets += 1;
        }
    }
    {
        // SAFETY: base .. base+PANEL_W*PANEL_H*4 foi mapeada por memory_map neste processo.
        let px = unsafe {
            core::slice::from_raw_parts_mut(base as *mut u8, (PANEL_W * PANEL_H * 4) as usize)
        };
        let mut s = Surface::new(
            px,
            PANEL_W as u32,
            PANEL_H as u32,
            PANEL_W as u32,
            PixelFormat::Rgbx8888,
        )
        .unwrap_or_else(|| fail(89, "superficie do painel"));
        s.clear(theme.surface);
        s.stroke_rect(Rect::new(0, 0, PANEL_W, PANEL_H), theme.accent);
        for k in 0..bullets {
            s.fill_rect(Rect::new(3, 3 + k as i32 * 8, 4, 4), theme.accent);
        }
    }
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(90, "enc commit painel"));
    let _ = rpc(&out[..m], &[], buf, hs, pending);
    let _ = base; // o painel é repintado só na reabertura; o mapeamento fica até o destroy
    Panel { id }
}

/// Processa um clique na barra: ativa janela ou alterna a Central de Ações (avisando pelo canal).
#[allow(clippy::too_many_arguments)]
fn handle_bar_click(
    bar: &Bar,
    x: i32,
    y: i32,
    theme: &Theme,
    panel: &mut Option<Panel>,
    buf: &mut [u8],
    hs: &mut [u32; 1],
    pending: &mut Option<wm::PointerEvent>,
) {
    match bar_click(bar, x, y, buf, hs, pending) {
        BarAction::Activated => {
            let _ = nexo_sys::channel_send(PIPE, b"activated", &[]);
        }
        BarAction::ToggleCenter => {
            if let Some(p) = panel.take() {
                let mut out = [0u8; 64];
                let m = wm::DestroyRequest { id: p.id }
                    .encode_msg(&mut out)
                    .unwrap_or_else(|_| fail(91, "enc destroy painel"));
                let _ = rpc(&out[..m], &[], buf, hs, pending);
                let _ = nexo_sys::channel_send(PIPE, b"cclosed", &[]);
            } else {
                *panel = Some(open_center(theme, buf, hs, pending));
                let _ = nexo_sys::channel_send(PIPE, b"copen", &[]);
            }
        }
        BarAction::None => {}
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let theme = Theme::dark();
    let mut buf = [0u8; 256];
    let mut hs = [0u32; 1];
    let mut pending: Option<wm::PointerEvent> = None;

    // Cria a barra no rodapé, acima de tudo.
    let mut out = [0u8; 128];
    let req = wm::CreateSurfaceRequest {
        x: BAR_X,
        y: BAR_Y,
        w: BAR_W,
        h: BAR_H,
        z: 5000,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(67, "enc create"));
    if nexo_sys::channel_send(WM, &out[..m], &[]) != Status::Ok {
        fail(68, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(WM, &mut buf, &mut hs).unwrap_or_else(|_| fail(69, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(70, "dec create"));
    if nh != 1 {
        fail(71, "sem handle da barra");
    }
    let mut bar = Bar {
        id: cs.id,
        base: nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(72, "map barra")),
        cells: [0; 8],
        cell_n: 0,
    };
    redraw(&mut bar, &theme, &mut buf, &mut hs, &mut pending);
    let mut panel: Option<Panel> = None;
    log!("shellui: Faixa de Atividades pronta (barra id {})", bar.id);

    loop {
        // clique pendente capturado durante um RPC?
        if let Some(ev) = pending.take()
            && ev.surface == bar.id
        {
            handle_bar_click(
                &bar,
                ev.x,
                ev.y,
                &theme,
                &mut panel,
                &mut buf,
                &mut hs,
                &mut pending,
            );
        }
        let mut worked = false;
        // pedidos dos apps pelo canal
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok((n, _)) => {
                worked = true;
                match &buf[..n] {
                    b"sess" => {
                        // broker: abre uma sessao nova no wm e entrega a ponta ao app
                        let (mine, theirs) =
                            nexo_sys::channel_create().unwrap_or_else(|_| fail(73, "canal sessao"));
                        let m = wm::OpenRequest { chan: theirs }
                            .encode_msg(&mut out)
                            .unwrap_or_else(|_| fail(74, "enc open"));
                        if nexo_sys::channel_send(WM, &out[..m], &[theirs]) != Status::Ok {
                            fail(75, "send open");
                        }
                        let (n, _) = {
                            let mut hs2 = [0u32; 1];
                            let mut b2 = [0u8; 128];
                            match nexo_sys::channel_recv(WM, &mut b2, &mut hs2) {
                                Ok((n, _)) => {
                                    buf[..n].copy_from_slice(&b2[..n]);
                                    (n, 0)
                                }
                                Err(_) => fail(76, "recv open"),
                            }
                        };
                        if wm::decode_open_response(&buf[..n]).is_err() {
                            fail(77, "open recusado");
                        }
                        if nexo_sys::channel_send(PIPE, b"sess", &[mine]) != Status::Ok {
                            fail(78, "send sessao");
                        }
                    }
                    b"sync" => {
                        redraw(&mut bar, &theme, &mut buf, &mut hs, &mut pending);
                        let _ = nexo_sys::channel_send(PIPE, b"ok", &[]);
                    }
                    _ => {
                        let _ = nexo_sys::channel_send(PIPE, b"?", &[]);
                    }
                }
            }
            Err(Status::WouldBlock) => {}
            Err(Status::PeerClosed) => {
                log!("shellui: orquestrador desconectou; encerrando");
                nexo_sys::exit(0)
            }
            Err(_) => fail(79, "recv pipe"),
        }
        // eventos do compositor (cliques na barra)
        match nexo_sys::channel_try_recv(WM, &mut buf, &mut hs) {
            Ok((n, _)) => {
                worked = true;
                if let Ok(ev) = wm::decode_pointer_event(&buf[..n])
                    && ev.surface == bar.id
                {
                    handle_bar_click(
                        &bar,
                        ev.x,
                        ev.y,
                        &theme,
                        &mut panel,
                        &mut buf,
                        &mut hs,
                        &mut pending,
                    );
                }
            }
            Err(Status::WouldBlock) => {}
            Err(Status::PeerClosed) => nexo_sys::exit(0),
            Err(_) => fail(80, "recv wm"),
        }
        if !worked && pending.is_none() {
            let _ = nexo_sys::channel_wait_any(&[WM, PIPE]);
        }
    }
}

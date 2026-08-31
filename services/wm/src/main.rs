//! `wm` — compositor de janelas em modo usuário. Handle 0 = canal da primeira sessão de cliente
//! (`nexo.wm`). Várias sessões coexistem: um cliente abre outra sessão com `open`, transferindo a
//! ponta de um canal novo (até [`MAX_CLIENTS`]). Cada `create_surface` cria um `MemoryObject`
//! compartilhado (o cliente escreve os pixels, o wm os lê); o wm compõe **todas** as superfícies
//! de **todas** as sessões com `nexo-wm` numa **saída** (outro `MemoryObject`), devolvida por
//! `output`. As superfícies pertencem à sessão que as criou (só ela pode commit/move/destroy), e
//! são liberadas quando a sessão desconecta. A apresentação num framebuffer real fica para a
//! integração com o serviço de vídeo.
#![no_std]
#![no_main]

use nexo_gfx::{Color, PixelFormat, Rect, Surface};
use nexo_proto::wm::{self, Request};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_wm::{Damage, Window, composite};

const OUT_W: i32 = 64;
const OUT_H: i32 = 48;
const MAX_SURFACES: usize = 8;
const MAX_CLIENTS: usize = 8;

/// Códigos evdev usados pela fonte de entrada (formato Linux input).
const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const BTN_LEFT: u16 = 0x110;
/// Tecla modificadora dos atalhos globais (Super/Meta) e Tab (cicla o foco).
const KEY_TAB: u16 = 15;
const KEY_LEFTMETA: u16 = 125;

/// Alvo de apresentação: o framebuffer real, mapeado via concessão do dispositivo de vídeo
/// (handle 1, quando presente) + `mmio_map` do BAR que o contém.
struct FbOut {
    /// Base virtual do mapeamento.
    base: u64,
    w: u32,
    h: u32,
    stride: u32,
    format: PixelFormat,
}

/// Erros remotos do protocolo `nexo.wm`.
const E_INVALID: u32 = 1;
const E_NO_RES: u32 = 2;
const E_NO_SURFACE: u32 = 3;
const E_NO_SESSION: u32 = 4;
const E_GRABBED: u32 = 5;

struct Slot {
    used: bool,
    owner: usize,
    rect: Rect,
    z: i32,
    /// Opacidade da janela (255 = opaca).
    alpha: u8,
    /// Dimensões do conteúdo no buffer (podem diferir de `rect.w/h` — ex.: mosaico —, e então a
    /// composição escala).
    buf_w: i32,
    buf_h: i32,
    /// Retângulo salvo antes de maximizar (para `restore`).
    saved: Option<Rect>,
    /// Handle do wm para o `MemoryObject` (mantido para ler os pixels e liberar no fim).
    mem: Handle,
    base: u64,
    len: u64,
}

const EMPTY: Slot = Slot {
    used: false,
    owner: 0,
    rect: Rect::new(0, 0, 0, 0),
    z: 0,
    alpha: 255,
    buf_w: 0,
    buf_h: 0,
    saved: None,
    mem: 0,
    base: 0,
    len: 0,
};

/// Realoca o buffer da superfície `i` para `new_w`×`new_h`: desmapeia/libera o antigo (o cliente
/// ainda tem o dele até fechar o handle), cria e mapeia o novo e atualiza `rect.w/h`. Devolve o
/// handle a entregar ao cliente, ou `None` se a alocação falhar (a superfície perde o buffer e é
/// removida).
fn realloc_surface(
    surfaces: &mut [Slot; MAX_SURFACES],
    i: usize,
    new_w: i32,
    new_h: i32,
) -> Option<Handle> {
    let old_pages = surfaces[i].len.div_ceil(4096);
    let _ = nexo_sys::memory_unmap(surfaces[i].base, old_pages * 4096);
    let _ = nexo_sys::handle_close(surfaces[i].mem);
    let bytes = (new_w * new_h * 4) as u64;
    let pages = bytes.div_ceil(4096);
    let mem = match nexo_sys::memory_create(pages) {
        Ok(h) => h,
        Err(_) => {
            surfaces[i].used = false;
            return None;
        }
    };
    let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| fail(59, "map realloc"));
    surfaces[i].rect.w = new_w;
    surfaces[i].rect.h = new_h;
    surfaces[i].buf_w = new_w;
    surfaces[i].buf_h = new_h;
    surfaces[i].mem = mem;
    surfaces[i].base = base;
    surfaces[i].len = bytes;
    Some(
        nexo_sys::handle_duplicate(mem, nexo_sys::abi::RIGHTS_MEMORY_DEFAULT)
            .unwrap_or_else(|_| fail(60, "dup realloc")),
    )
}

fn fail(code: i64, what: &str) -> ! {
    log!("wm: falha: {}", what);
    nexo_sys::exit(code)
}

/// Página(s) de um MemoryObject como fatia de bytes (mapeada USER|RW neste processo).
fn as_slice<'a>(base: u64, len: u64) -> &'a [u8] {
    // SAFETY: `base..base+len` foi mapeado por `memory_map` neste processo (USER|RW).
    unsafe { core::slice::from_raw_parts(base as *const u8, len as usize) }
}
fn as_slice_mut<'a>(base: u64, len: u64) -> &'a mut [u8] {
    // SAFETY: idem; único mapeamento mutável no wm para a saída.
    unsafe { core::slice::from_raw_parts_mut(base as *mut u8, len as usize) }
}

/// Recompõe a saída a partir de todas as superfícies visíveis (ordem-Z sobre fundo preto).
fn recompose(surfaces: &[Slot; MAX_SURFACES], out_base: u64, out_bytes: u64, fb: Option<&FbOut>) {
    let out_pixels = as_slice_mut(out_base, out_bytes);
    let mut out = Surface::new(
        out_pixels,
        OUT_W as u32,
        OUT_H as u32,
        OUT_W as u32,
        PixelFormat::Rgbx8888,
    )
    .unwrap_or_else(|| fail(52, "superficie de saida"));
    let mut wins = [Window {
        rect: Rect::new(0, 0, 0, 0),
        z: 0,
        pixels: &[],
        stride: 0,
        src_w: 0,
        src_h: 0,
        format: PixelFormat::Rgbx8888,
        alpha: 255,
    }; MAX_SURFACES];
    let mut n = 0;
    for s in surfaces.iter() {
        if s.used {
            wins[n] = Window {
                rect: s.rect,
                z: s.z,
                pixels: as_slice(s.base, s.len),
                stride: s.buf_w as u32,
                src_w: s.buf_w,
                src_h: s.buf_h,
                format: PixelFormat::Rgbx8888,
                alpha: s.alpha,
            };
            n += 1;
        }
    }
    let mut dmg = Damage::new();
    dmg.add(Rect::new(0, 0, OUT_W, OUT_H));
    composite(&mut out, &wins[..n], dmg.bounds(), Color::rgb(0, 0, 0));
    // Apresenta no framebuffer real, se mapeado (a saida composta e copiada com conversao de
    // formato para o canto superior esquerdo da tela).
    if let Some(fb) = fb {
        let fb_bytes = (fb.stride as u64) * (fb.h as u64) * 4;
        let fb_pixels = as_slice_mut(fb.base, fb_bytes);
        if let Some(mut screen) = Surface::new(fb_pixels, fb.w, fb.h, fb.stride, fb.format) {
            screen.blit(&out, Rect::new(0, 0, OUT_W, OUT_H), 0, 0);
        }
    }
}

/// Libera as superfícies da sessão `owner` (fecha o handle do wm e marca o slot livre).
fn free_session_surfaces(surfaces: &mut [Slot; MAX_SURFACES], owner: usize) {
    for s in surfaces.iter_mut() {
        if s.used && s.owner == owner {
            let _ = nexo_sys::handle_close(s.mem);
            s.used = false;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    // Saída composta: um MemoryObject de OUT_W*OUT_H*4 bytes.
    let out_bytes = (OUT_W * OUT_H * 4) as u64;
    let out_pages = out_bytes.div_ceil(4096);
    let out_mem =
        nexo_sys::memory_create(out_pages).unwrap_or_else(|_| fail(50, "memory_create saida"));
    let out_base = nexo_sys::memory_map(out_mem).unwrap_or_else(|_| fail(51, "memory_map saida"));
    let mut surfaces = [EMPTY; MAX_SURFACES];
    let mut sessions: [Option<Handle>; MAX_CLIENTS] = [None; MAX_CLIENTS];
    sessions[0] = Some(0); // primeira sessão = canal no handle 0
    log!(
        "wm: compositor pronto ({}x{}, ate {} superficies, {} sessoes)",
        OUT_W,
        OUT_H,
        MAX_SURFACES,
        MAX_CLIENTS
    );

    // Apresentação no framebuffer real: se recebemos a concessão do dispositivo de vídeo
    // (handle 1), consulta o layout (fb_info) e mapeia o BAR do framebuffer (mmio_map). Sem a
    // concessão (ou sem framebuffer), o compositor segue compondo só na saída compartilhada.
    let fb: Option<FbOut> = nexo_sys::fb_info().ok().and_then(|info| {
        if info.bytes_per_pixel != 4 {
            return None;
        }
        let format = PixelFormat::from_u32(info.format);
        if matches!(format, PixelFormat::Unknown) {
            return None;
        }
        let bytes = (info.stride as u64) * (info.height as u64) * 4;
        let len = bytes.div_ceil(4096) * 4096;
        let base = nexo_sys::mmio_map(1, info.base, len).ok()?;
        log!(
            "wm: apresentando no framebuffer {}x{} (stride {})",
            info.width,
            info.height,
            info.stride
        );
        Some(FbOut {
            base,
            w: info.width,
            h: info.height,
            stride: info.stride,
            format,
        })
    });

    // Fonte de entrada (mouse/teclado) e estado do ponteiro/foco.
    let mut input_ch: Option<Handle> = None;
    let mut px: i32 = 0;
    let mut py: i32 = 0;
    let mut focused: Option<usize> = None;
    let mut grabbed: Option<usize> = None;
    let mut meta_down = false;

    let mut buf = [0u8; 512];
    let mut out = [0u8; 512];
    let mut hbuf = [0u32; 1];
    loop {
        let mut worked = false;
        // Solta o foco se a superfície focada foi destruída/desconectada (evita apontar para um
        // slot reutilizado por uma superfície nova antes do próximo `create`).
        if let Some(i) = focused
            && !surfaces[i].used
        {
            focused = None;
        }
        if let Some(i) = grabbed
            && !surfaces[i].used
        {
            grabbed = None;
        }
        for slot in 0..MAX_CLIENTS {
            let Some(ch) = sessions[slot] else {
                continue;
            };
            let (n, nh) = match nexo_sys::channel_try_recv(ch, &mut buf, &mut hbuf) {
                Ok(v) => v,
                Err(Status::WouldBlock) => continue,
                Err(Status::PeerClosed) => {
                    free_session_surfaces(&mut surfaces, slot);
                    if slot != 0 {
                        let _ = nexo_sys::handle_close(ch);
                    }
                    sessions[slot] = None;
                    recompose(&surfaces, out_base, out_bytes, fb.as_ref());
                    if sessions.iter().all(|s| s.is_none()) {
                        log!("wm: ultima sessao desconectou; encerrando");
                        nexo_sys::exit(0)
                    }
                    continue;
                }
                Err(_) => fail(53, "recv"),
            };
            worked = true;
            let request = match wm::decode_request_with_handles(&buf[..n], &hbuf[..nh]) {
                Ok(r) => r,
                Err(_) => {
                    let m = wm::encode_error(0, E_INVALID, &mut out).unwrap_or(0);
                    let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
                    continue;
                }
            };
            // `open` registra o canal transferido como mais uma sessão.
            if let Request::Open(rq) = &request {
                let placed = (0..MAX_CLIENTS).find(|&i| sessions[i].is_none());
                let m = match placed {
                    Some(i) => {
                        sessions[i] = Some(rq.chan);
                        wm::OpenResponse {}.encode_msg(&mut out).unwrap_or(0)
                    }
                    None => {
                        let _ = nexo_sys::handle_close(rq.chan);
                        wm::encode_error(wm::OpenRequest::METHOD_ID, E_NO_SESSION, &mut out)
                            .unwrap_or(0)
                    }
                };
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
                continue;
            }
            // `set_input` registra a fonte de entrada (mouse/teclado).
            if let Request::SetInput(rq) = &request {
                if let Some(old) = input_ch.replace(rq.chan) {
                    let _ = nexo_sys::handle_close(old);
                }
                let m = wm::SetInputResponse {}.encode_msg(&mut out).unwrap_or(0);
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
                continue;
            }
            serve(
                request,
                slot,
                ch,
                &mut surfaces,
                out_mem,
                out_base,
                out_bytes,
                &mut out,
                fb.as_ref(),
                &mut focused,
                &mut grabbed,
            );
        }
        // Processa eventos de entrada (evdev crus, 8 bytes cada).
        if let Some(ich) = input_ch {
            match nexo_sys::channel_try_recv(ich, &mut buf, &mut hbuf) {
                Ok((n, _)) => {
                    worked = true;
                    let mut off = 0;
                    while off + 8 <= n {
                        let ty = u16::from_le_bytes([buf[off], buf[off + 1]]);
                        let code = u16::from_le_bytes([buf[off + 2], buf[off + 3]]);
                        let value = u32::from_le_bytes([
                            buf[off + 4],
                            buf[off + 5],
                            buf[off + 6],
                            buf[off + 7],
                        ]);
                        off += 8;
                        match (ty, code, value) {
                            (EV_ABS, ABS_X, v) => px = v as i32,
                            (EV_ABS, ABS_Y, v) => py = v as i32,
                            (EV_KEY, BTN_LEFT, 1) => {
                                // captura em vigor: cliques são engolidos (ninguém rouba o foco)
                                if grabbed.is_some() {
                                    continue;
                                }
                                // foco por clique: traz para a frente a superfície sob o ponteiro.
                                let hit = surfaces
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, s)| s.used && s.rect.contains(px, py))
                                    .max_by_key(|(_, s)| s.z)
                                    .map(|(i, _)| i);
                                if let Some(i) = hit {
                                    let top = surfaces
                                        .iter()
                                        .filter(|s| s.used)
                                        .map(|s| s.z)
                                        .max()
                                        .unwrap_or(0);
                                    surfaces[i].z = top.saturating_add(1);
                                    focused = Some(i);
                                    recompose(&surfaces, out_base, out_bytes, fb.as_ref());
                                }
                            }
                            (EV_KEY, BTN_LEFT, _) => {} // release do botão: ignora
                            (EV_KEY, KEY_LEFTMETA, v) => meta_down = v == 1, // modificador: não entrega
                            (EV_KEY, KEY_TAB, 1) if meta_down => {
                                // atalho global Meta+Tab: cicla o foco (traz a janela de trás para a frente).
                                let bottom = surfaces
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, s)| s.used)
                                    .min_by_key(|(_, s)| s.z)
                                    .map(|(i, _)| i);
                                if let Some(i) = bottom {
                                    let top = surfaces
                                        .iter()
                                        .filter(|s| s.used)
                                        .map(|s| s.z)
                                        .max()
                                        .unwrap_or(0);
                                    surfaces[i].z = top.saturating_add(1);
                                    focused = Some(i);
                                    recompose(&surfaces, out_base, out_bytes, fb.as_ref());
                                }
                            }
                            (EV_KEY, c, v) => {
                                // tecla comum: a captura tem precedência sobre o foco.
                                if let Some(i) = grabbed.or(focused)
                                    && surfaces[i].used
                                    && let Some(sess) = sessions[surfaces[i].owner]
                                {
                                    let ev = wm::KeyEvent {
                                        surface: i as u32,
                                        code: c as u32,
                                        value: v,
                                    };
                                    let m = ev.encode_msg(&mut out).unwrap_or(0);
                                    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(Status::WouldBlock) => {}
                Err(Status::PeerClosed) => {
                    let _ = nexo_sys::handle_close(ich);
                    input_ch = None;
                }
                Err(_) => fail(62, "recv entrada"),
            }
        }
        if !worked {
            let mut waits = [0 as Handle; MAX_CLIENTS + 1];
            let mut wn = 0;
            for s in sessions.iter().flatten() {
                waits[wn] = *s;
                wn += 1;
            }
            if let Some(ich) = input_ch {
                waits[wn] = ich;
                wn += 1;
            }
            let _ = nexo_sys::channel_wait_any(&waits[..wn]);
        }
    }
}

/// Atende um pedido de uma sessão (exceto `open`, tratado no laço). Responde no canal `ch`.
#[allow(clippy::too_many_arguments)]
fn serve(
    request: Request,
    owner: usize,
    ch: Handle,
    surfaces: &mut [Slot; MAX_SURFACES],
    out_mem: Handle,
    out_base: u64,
    out_bytes: u64,
    out: &mut [u8; 512],
    fb: Option<&FbOut>,
    focused: &mut Option<usize>,
    grabbed: &mut Option<usize>,
) {
    // Uma superfície só pode ser tocada pela sessão que a criou.
    let mine = |surfaces: &[Slot; MAX_SURFACES], id: u32| -> bool {
        (id as usize) < MAX_SURFACES
            && surfaces[id as usize].used
            && surfaces[id as usize].owner == owner
    };
    match request {
        Request::CreateSurface(rq) => {
            if rq.w <= 0 || rq.h <= 0 || rq.w > OUT_W || rq.h > OUT_H {
                reply_err(ch, wm::CreateSurfaceRequest::METHOD_ID, E_INVALID, out);
                return;
            }
            let Some(id) = (0..MAX_SURFACES).find(|&i| !surfaces[i].used) else {
                reply_err(ch, wm::CreateSurfaceRequest::METHOD_ID, E_NO_RES, out);
                return;
            };
            let bytes = (rq.w * rq.h * 4) as u64;
            let pages = bytes.div_ceil(4096);
            let mem = match nexo_sys::memory_create(pages) {
                Ok(h) => h,
                Err(_) => {
                    reply_err(ch, wm::CreateSurfaceRequest::METHOD_ID, E_NO_RES, out);
                    return;
                }
            };
            let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| fail(54, "map superficie"));
            surfaces[id] = Slot {
                used: true,
                owner,
                rect: Rect::new(rq.x, rq.y, rq.w, rq.h),
                z: rq.z,
                alpha: 255,
                buf_w: rq.w,
                buf_h: rq.h,
                saved: None,
                mem,
                base,
                len: bytes,
            };
            // foco inicial: a primeira janela criada (sem foco previo) passa a receber o teclado
            if focused.is_none() {
                *focused = Some(id);
            }
            // duplica o handle para o cliente (o wm mantém o seu para ler os pixels)
            let client_mem = nexo_sys::handle_duplicate(mem, nexo_sys::abi::RIGHTS_MEMORY_DEFAULT)
                .unwrap_or_else(|_| fail(55, "dup handle"));
            let resp = wm::CreateSurfaceResponse {
                id: id as u32,
                mem: client_mem,
            };
            let m = resp.encode_msg(out).unwrap_or(0);
            if nexo_sys::channel_send(ch, &out[..m], &resp.handles()) != Status::Ok {
                fail(56, "send create");
            }
        }
        Request::Commit(rq) => {
            if mine(surfaces, rq.id) {
                recompose(surfaces, out_base, out_bytes, fb);
                let m = wm::CommitResponse {}.encode_msg(out).unwrap_or(0);
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
            } else {
                reply_err(ch, wm::CommitRequest::METHOD_ID, E_NO_SURFACE, out);
            }
        }
        Request::Move(rq) => {
            if mine(surfaces, rq.id) {
                surfaces[rq.id as usize].rect.x = rq.x;
                surfaces[rq.id as usize].rect.y = rq.y;
                recompose(surfaces, out_base, out_bytes, fb);
                let m = wm::MoveResponse {}.encode_msg(out).unwrap_or(0);
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
            } else {
                reply_err(ch, wm::MoveRequest::METHOD_ID, E_NO_SURFACE, out);
            }
        }
        Request::Destroy(rq) => {
            if mine(surfaces, rq.id) {
                let _ = nexo_sys::handle_close(surfaces[rq.id as usize].mem);
                surfaces[rq.id as usize].used = false;
                recompose(surfaces, out_base, out_bytes, fb);
                let m = wm::DestroyResponse {}.encode_msg(out).unwrap_or(0);
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
            } else {
                reply_err(ch, wm::DestroyRequest::METHOD_ID, E_NO_SURFACE, out);
            }
        }
        Request::SetAlpha(rq) => {
            if mine(surfaces, rq.id) {
                surfaces[rq.id as usize].alpha = rq.alpha;
                recompose(surfaces, out_base, out_bytes, fb);
                let m = wm::SetAlphaResponse {}.encode_msg(out).unwrap_or(0);
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
            } else {
                reply_err(ch, wm::SetAlphaRequest::METHOD_ID, E_NO_SURFACE, out);
            }
        }
        Request::Raise(rq) => {
            if mine(surfaces, rq.id) {
                let top = surfaces
                    .iter()
                    .filter(|s| s.used)
                    .map(|s| s.z)
                    .max()
                    .unwrap_or(0);
                surfaces[rq.id as usize].z = top.saturating_add(1);
                recompose(surfaces, out_base, out_bytes, fb);
                let m = wm::RaiseResponse {}.encode_msg(out).unwrap_or(0);
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
            } else {
                reply_err(ch, wm::RaiseRequest::METHOD_ID, E_NO_SURFACE, out);
            }
        }
        Request::Lower(rq) => {
            if mine(surfaces, rq.id) {
                let bottom = surfaces
                    .iter()
                    .filter(|s| s.used)
                    .map(|s| s.z)
                    .min()
                    .unwrap_or(0);
                surfaces[rq.id as usize].z = bottom.saturating_sub(1);
                recompose(surfaces, out_base, out_bytes, fb);
                let m = wm::LowerResponse {}.encode_msg(out).unwrap_or(0);
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
            } else {
                reply_err(ch, wm::LowerRequest::METHOD_ID, E_NO_SURFACE, out);
            }
        }
        Request::Resize(rq) => {
            if !mine(surfaces, rq.id) {
                reply_err(ch, wm::ResizeRequest::METHOD_ID, E_NO_SURFACE, out);
                return;
            }
            if rq.w <= 0 || rq.h <= 0 || rq.w > OUT_W || rq.h > OUT_H {
                reply_err(ch, wm::ResizeRequest::METHOD_ID, E_INVALID, out);
                return;
            }
            let i = rq.id as usize;
            match realloc_surface(surfaces, i, rq.w, rq.h) {
                Some(client_mem) => {
                    recompose(surfaces, out_base, out_bytes, fb);
                    let resp = wm::ResizeResponse { mem: client_mem };
                    let m = resp.encode_msg(out).unwrap_or(0);
                    if nexo_sys::channel_send(ch, &out[..m], &resp.handles()) != Status::Ok {
                        fail(61, "send resize");
                    }
                }
                None => {
                    recompose(surfaces, out_base, out_bytes, fb);
                    reply_err(ch, wm::ResizeRequest::METHOD_ID, E_NO_RES, out);
                }
            }
        }
        Request::Maximize(rq) => {
            if !mine(surfaces, rq.id) {
                reply_err(ch, wm::MaximizeRequest::METHOD_ID, E_NO_SURFACE, out);
                return;
            }
            let i = rq.id as usize;
            if surfaces[i].saved.is_none() {
                surfaces[i].saved = Some(surfaces[i].rect);
            }
            surfaces[i].rect.x = 0;
            surfaces[i].rect.y = 0;
            match realloc_surface(surfaces, i, OUT_W, OUT_H) {
                Some(client_mem) => {
                    recompose(surfaces, out_base, out_bytes, fb);
                    let resp = wm::MaximizeResponse { mem: client_mem };
                    let m = resp.encode_msg(out).unwrap_or(0);
                    if nexo_sys::channel_send(ch, &out[..m], &resp.handles()) != Status::Ok {
                        fail(63, "send maximize");
                    }
                }
                None => {
                    recompose(surfaces, out_base, out_bytes, fb);
                    reply_err(ch, wm::MaximizeRequest::METHOD_ID, E_NO_RES, out);
                }
            }
        }
        Request::Restore(rq) => {
            if !mine(surfaces, rq.id) {
                reply_err(ch, wm::RestoreRequest::METHOD_ID, E_NO_SURFACE, out);
                return;
            }
            let i = rq.id as usize;
            let Some(saved) = surfaces[i].saved.take() else {
                reply_err(ch, wm::RestoreRequest::METHOD_ID, E_INVALID, out);
                return;
            };
            surfaces[i].rect.x = saved.x;
            surfaces[i].rect.y = saved.y;
            match realloc_surface(surfaces, i, saved.w, saved.h) {
                Some(client_mem) => {
                    recompose(surfaces, out_base, out_bytes, fb);
                    let resp = wm::RestoreResponse { mem: client_mem };
                    let m = resp.encode_msg(out).unwrap_or(0);
                    if nexo_sys::channel_send(ch, &out[..m], &resp.handles()) != Status::Ok {
                        fail(64, "send restore");
                    }
                }
                None => {
                    recompose(surfaces, out_base, out_bytes, fb);
                    reply_err(ch, wm::RestoreRequest::METHOD_ID, E_NO_RES, out);
                }
            }
        }
        Request::Tile(_) => {
            // Mosaico: grade cobrindo a saída, na ordem dos slots; só muda os retângulos de
            // exibição (o conteúdo é escalado na composição). Gesto global de layout.
            let n = surfaces.iter().filter(|s| s.used).count() as i32;
            if n > 0 {
                let mut cols = 1i32;
                while cols * cols < n {
                    cols += 1;
                }
                let rows = (n + cols - 1) / cols;
                let (cw, chh) = (OUT_W / cols, OUT_H / rows);
                let mut k = 0i32;
                for s in surfaces.iter_mut() {
                    if s.used {
                        s.rect = Rect::new((k % cols) * cw, (k / cols) * chh, cw, chh);
                        k += 1;
                    }
                }
                recompose(surfaces, out_base, out_bytes, fb);
            }
            let m = wm::TileResponse {}.encode_msg(out).unwrap_or(0);
            let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
        }
        Request::Grab(rq) => {
            if !mine(surfaces, rq.id) {
                reply_err(ch, wm::GrabRequest::METHOD_ID, E_NO_SURFACE, out);
                return;
            }
            let i = rq.id as usize;
            let m = if grabbed.is_some() && *grabbed != Some(i) {
                wm::encode_error(wm::GrabRequest::METHOD_ID, E_GRABBED, out).unwrap_or(0)
            } else {
                *grabbed = Some(i);
                wm::GrabResponse {}.encode_msg(out).unwrap_or(0)
            };
            let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
        }
        Request::Ungrab(rq) => {
            if !mine(surfaces, rq.id) {
                reply_err(ch, wm::UngrabRequest::METHOD_ID, E_NO_SURFACE, out);
                return;
            }
            let m = if *grabbed == Some(rq.id as usize) {
                *grabbed = None;
                wm::UngrabResponse {}.encode_msg(out).unwrap_or(0)
            } else {
                wm::encode_error(wm::UngrabRequest::METHOD_ID, E_INVALID, out).unwrap_or(0)
            };
            let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
        }
        Request::Output(_) => {
            let client_out =
                nexo_sys::handle_duplicate(out_mem, nexo_sys::abi::RIGHTS_MEMORY_DEFAULT)
                    .unwrap_or_else(|_| fail(57, "dup saida"));
            let resp = wm::OutputResponse {
                w: OUT_W,
                h: OUT_H,
                mem: client_out,
            };
            let m = resp.encode_msg(out).unwrap_or(0);
            if nexo_sys::channel_send(ch, &out[..m], &resp.handles()) != Status::Ok {
                fail(58, "send output");
            }
        }
        Request::Open(_) | Request::SetInput(_) => {} // tratados no laço principal
    }
}

fn reply_err(ch: Handle, method: u32, code: u32, out: &mut [u8; 512]) {
    let m = wm::encode_error(method, code, out).unwrap_or(0);
    let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
}

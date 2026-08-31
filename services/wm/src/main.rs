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

/// Erros remotos do protocolo `nexo.wm`.
const E_INVALID: u32 = 1;
const E_NO_RES: u32 = 2;
const E_NO_SURFACE: u32 = 3;
const E_NO_SESSION: u32 = 4;

struct Slot {
    used: bool,
    owner: usize,
    rect: Rect,
    z: i32,
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
    mem: 0,
    base: 0,
    len: 0,
};

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
fn recompose(surfaces: &[Slot; MAX_SURFACES], out_base: u64, out_bytes: u64) {
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
        format: PixelFormat::Rgbx8888,
    }; MAX_SURFACES];
    let mut n = 0;
    for s in surfaces.iter() {
        if s.used {
            wins[n] = Window {
                rect: s.rect,
                z: s.z,
                pixels: as_slice(s.base, s.len),
                stride: s.rect.w as u32,
                format: PixelFormat::Rgbx8888,
            };
            n += 1;
        }
    }
    let mut dmg = Damage::new();
    dmg.add(Rect::new(0, 0, OUT_W, OUT_H));
    composite(&mut out, &wins[..n], dmg.bounds(), Color::rgb(0, 0, 0));
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

    let mut buf = [0u8; 512];
    let mut out = [0u8; 512];
    let mut hbuf = [0u32; 1];
    loop {
        let mut worked = false;
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
                    recompose(&surfaces, out_base, out_bytes);
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
            serve(
                request,
                slot,
                ch,
                &mut surfaces,
                out_mem,
                out_base,
                out_bytes,
                &mut out,
            );
        }
        if !worked {
            let mut waits = [0 as Handle; MAX_CLIENTS];
            let mut wn = 0;
            for s in sessions.iter().flatten() {
                waits[wn] = *s;
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
                mem,
                base,
                len: bytes,
            };
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
                recompose(surfaces, out_base, out_bytes);
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
                recompose(surfaces, out_base, out_bytes);
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
                recompose(surfaces, out_base, out_bytes);
                let m = wm::DestroyResponse {}.encode_msg(out).unwrap_or(0);
                let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
            } else {
                reply_err(ch, wm::DestroyRequest::METHOD_ID, E_NO_SURFACE, out);
            }
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
        Request::Open(_) => {} // tratado no laço principal
    }
}

fn reply_err(ch: Handle, method: u32, code: u32, out: &mut [u8; 512]) {
    let m = wm::encode_error(method, code, out).unwrap_or(0);
    let _ = nexo_sys::channel_send(ch, &out[..m], &[]);
}

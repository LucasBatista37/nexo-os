//! `wm` — compositor de janelas em modo usuário. Handle 0 = canal do cliente (`nexo.wm`).
//! Cada `create_surface` cria um `MemoryObject` compartilhado (o cliente escreve os pixels, o
//! wm os lê) e o wm compõe todas as superfícies com `nexo-wm` numa **saída** (outro
//! `MemoryObject`), devolvida por `output`. Um cliente por instância; a apresentação num
//! framebuffer real fica para a integração com o serviço de vídeo.
#![no_std]
#![no_main]

use nexo_gfx::{Color, PixelFormat, Rect, Surface};
use nexo_proto::wm::{self, Request};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_wm::{Damage, Window, composite};

const CLIENT: Handle = 0;
const OUT_W: i32 = 64;
const OUT_H: i32 = 48;
const MAX_SURFACES: usize = 8;

struct Slot {
    used: bool,
    rect: Rect,
    z: i32,
    base: u64,
    len: u64,
}

const EMPTY: Slot = Slot {
    used: false,
    rect: Rect::new(0, 0, 0, 0),
    z: 0,
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

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    // Saída composta: um MemoryObject de OUT_W*OUT_H*4 bytes (=> 12 páginas).
    let out_bytes = (OUT_W * OUT_H * 4) as u64;
    let out_pages = out_bytes.div_ceil(4096);
    let out_mem =
        nexo_sys::memory_create(out_pages).unwrap_or_else(|_| fail(50, "memory_create saida"));
    let out_base = nexo_sys::memory_map(out_mem).unwrap_or_else(|_| fail(51, "memory_map saida"));
    let mut surfaces = [EMPTY; MAX_SURFACES];
    log!(
        "wm: compositor pronto ({}x{}, ate {} superficies)",
        OUT_W,
        OUT_H,
        MAX_SURFACES
    );

    let recompose = |surfaces: &[Slot; MAX_SURFACES]| {
        let out_pixels = as_slice_mut(out_base, out_bytes);
        let mut out = Surface::new(
            out_pixels,
            OUT_W as u32,
            OUT_H as u32,
            OUT_W as u32,
            PixelFormat::Rgbx8888,
        )
        .unwrap_or_else(|| fail(52, "superficie de saida"));
        // Monta a lista de janelas visíveis a partir dos slots.
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
    };

    let mut buf = [0u8; 512];
    let mut out = [0u8; 512];
    let mut hbuf = [0u32; 1];
    loop {
        let (n, _) = match nexo_sys::channel_recv(CLIENT, &mut buf, &mut hbuf) {
            Ok(v) => v,
            Err(Status::PeerClosed) => {
                log!("wm: cliente desconectou");
                nexo_sys::exit(0)
            }
            Err(_) => fail(53, "recv"),
        };
        let request = match wm::decode_request(&buf[..n]) {
            Ok(r) => r,
            Err(_) => {
                let m = wm::encode_error(0, 1, &mut out).unwrap_or(0);
                let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
                continue;
            }
        };
        match request {
            Request::CreateSurface(rq) => {
                if rq.w <= 0 || rq.h <= 0 || rq.w > OUT_W || rq.h > OUT_H {
                    let m = wm::encode_error(wm::CreateSurfaceRequest::METHOD_ID, 1, &mut out)
                        .unwrap_or(0);
                    let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
                    continue;
                }
                let Some(id) = (0..MAX_SURFACES).find(|&i| !surfaces[i].used) else {
                    let m = wm::encode_error(wm::CreateSurfaceRequest::METHOD_ID, 2, &mut out)
                        .unwrap_or(0);
                    let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
                    continue;
                };
                let bytes = (rq.w * rq.h * 4) as u64;
                let pages = bytes.div_ceil(4096);
                let mem = match nexo_sys::memory_create(pages) {
                    Ok(h) => h,
                    Err(_) => {
                        let m = wm::encode_error(wm::CreateSurfaceRequest::METHOD_ID, 2, &mut out)
                            .unwrap_or(0);
                        let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
                        continue;
                    }
                };
                let base = nexo_sys::memory_map(mem).unwrap_or_else(|_| fail(54, "map superficie"));
                surfaces[id] = Slot {
                    used: true,
                    rect: Rect::new(rq.x, rq.y, rq.w, rq.h),
                    z: rq.z,
                    base,
                    len: bytes,
                };
                // duplica o handle para o cliente (o wm mantém o seu para ler os pixels)
                let client_mem =
                    nexo_sys::handle_duplicate(mem, nexo_sys::abi::RIGHTS_MEMORY_DEFAULT)
                        .unwrap_or_else(|_| fail(55, "dup handle"));
                let resp = wm::CreateSurfaceResponse {
                    id: id as u32,
                    mem: client_mem,
                };
                let m = resp.encode_msg(&mut out).unwrap_or(0);
                if nexo_sys::channel_send(CLIENT, &out[..m], &resp.handles()) != Status::Ok {
                    fail(56, "send create");
                }
            }
            Request::Commit(rq) => {
                let ok = (rq.id as usize) < MAX_SURFACES && surfaces[rq.id as usize].used;
                let m = if ok {
                    recompose(&surfaces);
                    wm::CommitResponse {}.encode_msg(&mut out).unwrap_or(0)
                } else {
                    wm::encode_error(wm::CommitRequest::METHOD_ID, 3, &mut out).unwrap_or(0)
                };
                let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
            }
            Request::Move(rq) => {
                let i = rq.id as usize;
                let m = if i < MAX_SURFACES && surfaces[i].used {
                    surfaces[i].rect.x = rq.x;
                    surfaces[i].rect.y = rq.y;
                    recompose(&surfaces);
                    wm::MoveResponse {}.encode_msg(&mut out).unwrap_or(0)
                } else {
                    wm::encode_error(wm::MoveRequest::METHOD_ID, 3, &mut out).unwrap_or(0)
                };
                let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
            }
            Request::Destroy(rq) => {
                let i = rq.id as usize;
                let m = if i < MAX_SURFACES && surfaces[i].used {
                    surfaces[i].used = false;
                    recompose(&surfaces);
                    wm::DestroyResponse {}.encode_msg(&mut out).unwrap_or(0)
                } else {
                    wm::encode_error(wm::DestroyRequest::METHOD_ID, 3, &mut out).unwrap_or(0)
                };
                let _ = nexo_sys::channel_send(CLIENT, &out[..m], &[]);
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
                let m = resp.encode_msg(&mut out).unwrap_or(0);
                if nexo_sys::channel_send(CLIENT, &out[..m], &resp.handles()) != Status::Ok {
                    fail(58, "send output");
                }
            }
        }
    }
}

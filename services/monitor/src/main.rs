//! `monitor` — monitor de sistema (Plano §Fase 6: "criar monitor de sistema"). Janela que mostra
//! a saúde do sistema lida do kernel via `debug_info`: CPUs online, uptime, processos vivos e
//! memória física (quadros livres/utilizáveis). Cada estatística vira uma célula verde (sã) ou
//! vermelha (anômala); a última célula é um *heartbeat* que alterna de cor a cada atualização —
//! prova visível (e testável de fora) de que o monitor está vivo e relendo o kernel.
//! Handle 0 = canal do orquestrador (recebe "sess"; cordão de vida; emite "pronto").
#![no_std]
#![no_main]

use nexo_gfx::{Color, PixelFormat, Rect, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const W: i32 = 44;
const H: i32 = 10;
/// Célula k (0..4): 6x6 em x=2+8k, y=2. As quatro primeiras são estatísticas; a 5ª é o heartbeat.
fn cell(k: i32) -> Rect {
    Rect::new(2 + 8 * k, 2, 6, 6)
}

fn fail(code: i64, what: &str) -> ! {
    log!("monitor: falha: {}", what);
    nexo_sys::exit(code)
}

/// Lê o kernel e devolve a sanidade de cada estatística.
fn read_stats() -> [bool; 4] {
    let cpus = nexo_sys::debug_info(0);
    let uptime = nexo_sys::debug_info(1);
    let procs = nexo_sys::debug_info(4);
    let free = nexo_sys::debug_info(5);
    let total = nexo_sys::debug_info(6);
    [
        cpus >= 1,
        uptime > 0,
        procs >= 2, // pelo menos o compositor e este monitor
        total > 0 && free > 0 && free <= total,
    ]
}

fn redraw(base: u64, oks: &[bool; 4], hb: bool) {
    // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
    let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
        .unwrap_or_else(|| fail(20, "superficie"));
    s.clear(Color::rgb(0x14, 0x15, 0x18));
    for (k, ok) in oks.iter().enumerate() {
        let c = if *ok {
            Color::rgb(0, 200, 0)
        } else {
            Color::rgb(200, 0, 0)
        };
        s.fill_rect(cell(k as i32), c);
    }
    let c = if hb {
        Color::rgb(255, 255, 255)
    } else {
        Color::rgb(255, 0, 255)
    };
    s.fill_rect(cell(4), c);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut buf = [0u8; 256];
    let mut out = [0u8; 256];
    let mut hs = [0u32; 1];
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(21, "sessao nao recebida"),
    };

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
        title_len: 7,
    };
    title.title[..7].copy_from_slice(b"monitor");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(28, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    let mut hb = false;
    let oks = read_stats();
    log!(
        "monitor: cpus={} uptime_ms={} procs={} quadros={}/{}",
        nexo_sys::debug_info(0),
        nexo_sys::debug_info(1),
        nexo_sys::debug_info(4),
        nexo_sys::debug_info(5),
        nexo_sys::debug_info(6)
    );
    redraw(base, &oks, hb);
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(29, "enc commit"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    let _ = nexo_sys::channel_send(PIPE, b"pronto", &[]);

    loop {
        // cordão de vida: o orquestrador fechou o pipe = hora de sair
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        // eventos da sessão (foco, ponteiro): drena; sessão fechada = sair
        loop {
            match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
                Ok(_) => {}
                Err(Status::WouldBlock) => break,
                Err(_) => nexo_sys::exit(0),
            }
        }
        nexo_sys::sleep_ns(100_000_000);
        hb = !hb;
        let oks = read_stats();
        redraw(base, &oks, hb);
        let m = wm::CommitRequest { id }
            .encode_msg(&mut out)
            .unwrap_or_else(|_| fail(30, "enc commit2"));
        let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
        let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    }
}

//! `agenda` — calendário (Plano §Fase 6: "criar calculadora, calendário e utilitários").
//! Mostra o mês corrente numa grade 7×6 (colunas = dias da semana, segunda primeiro): cada dia
//! é uma célula; **hoje** é a célula de acento. A data vem do relógio de parede do kernel
//! (`debug_info` seletor 7 — RTC lido no boot) e a matemática civil é da `nexo-cal` (testada
//! no host de 1970 a 2370). Handle 0 = canal do orquestrador ("sess"; cordão de vida; "pronto").
#![no_std]
#![no_main]

use nexo_cal::{civil_from_epoch, days_from_civil, days_in_month, weekday_from_days};
use nexo_gfx::{Color, PixelFormat, Rect, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const W: i32 = 64;
const H: i32 = 44;

fn fail(code: i64, what: &str) -> ! {
    log!("agenda: falha: {}", what);
    nexo_sys::exit(code)
}

/// Retângulo da célula do `slot` (0..42): 7 colunas × 6 linhas.
fn cell(slot: u8) -> Rect {
    let (col, row) = (slot as i32 % 7, slot as i32 / 7);
    Rect::new(1 + col * 9, 1 + row * 7, 8, 6)
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut buf = [0u8; 384];
    let mut out = [0u8; 256];
    let mut hs = [0u32; 1];
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(21, "sessao nao recebida"),
    };

    let epoch = nexo_sys::debug_info(7);
    if epoch == 0 {
        fail(22, "sem relogio de parede (RTC)");
    }
    let today = civil_from_epoch(epoch);
    let first_slot = weekday_from_days(days_from_civil(today.year, today.month, 1));
    let ndays = days_in_month(today.year, today.month);
    log!(
        "agenda: {:04}-{:02}, hoje dia {} ({} dias, 1o no slot {})",
        today.year,
        today.month,
        today.day,
        ndays,
        first_slot
    );

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
        .unwrap_or_else(|_| fail(23, "enc create"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(24, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(sess, &mut buf, &mut hs).unwrap_or_else(|_| fail(25, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(26, "dec create"));
    if nh != 1 {
        fail(27, "sem handle");
    }
    let id = cs.id;
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(28, "map"));
    let mut title = wm::SetTitleRequest {
        id,
        title: [0; 32],
        title_len: 6,
    };
    title.title[..6].copy_from_slice(b"agenda");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(29, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    // pinta a grade do mês
    // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
    let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
        .unwrap_or_else(|| fail(30, "superficie"));
    s.clear(Color::rgb(0x14, 0x15, 0x18));
    for day in 1..=ndays {
        let slot = first_slot + day - 1;
        if slot >= 42 {
            break;
        }
        let c = if day == today.day {
            Color::rgb(0x6f, 0x9f, 0xff) // hoje: acento
        } else {
            Color::rgb(0x50, 0x55, 0x60)
        };
        s.fill_rect(cell(slot), c);
    }
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(31, "enc commit"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    let _ = nexo_sys::channel_send(PIPE, b"pronto", &[]);

    loop {
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0), // cordão de vida
        }
        match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        let _ = nexo_sys::channel_wait_any(&[PIPE, sess]);
    }
}

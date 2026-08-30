//! `echo-client` — pede conexões ao `svcmgr` (handle 0) e faz N pedidos ao
//! serviço de eco; quando o serviço cai (PeerClosed/erro), reconecta.
#![no_std]
#![no_main]

use nexo_rt::log;
use nexo_sys::abi::Status;

const REQUESTS: u32 = 8;
const MAX_ATTEMPTS: u32 = 40;

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let ctl: nexo_sys::Handle = 0;
    let mut ok = 0u32;
    let mut attempts = 0u32;
    let mut failures = 0u32;
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 2];
    while ok < REQUESTS && attempts < MAX_ATTEMPTS {
        attempts += 1;
        if nexo_sys::channel_send(ctl, b"connect", &[]) != Status::Ok {
            nexo_sys::exit(80);
        }
        let conn = match nexo_sys::channel_recv(ctl, &mut buf, &mut hs) {
            Ok((n, 1)) if &buf[..n] == b"ok" => hs[0],
            Ok((n, _)) if &buf[..n] == b"retry" => continue,
            _ => nexo_sys::exit(81),
        };
        let mut req = nexo_rt::Buf::<32>::new();
        use core::fmt::Write as _;
        let _ = write!(req, "ping {}", ok + 1);
        if nexo_sys::channel_send(conn, req.as_bytes(), &[]) != Status::Ok {
            nexo_sys::handle_close(conn);
            failures += 1;
            continue;
        }
        match nexo_sys::channel_recv(conn, &mut buf, &mut hs) {
            Ok((n, 0)) if buf[..n].starts_with(b"echo: ping ") => {
                ok += 1;
            }
            Err(Status::PeerClosed) => {
                failures += 1;
                log!(
                    "echo-client: servico caiu no pedido {}; reconectando",
                    ok + 1
                );
            }
            other => {
                log!("echo-client: resposta inesperada: {:?}", other.map(|v| v.0));
                nexo_sys::exit(82);
            }
        }
        nexo_sys::handle_close(conn);
    }
    log!(
        "echo-client: {} respostas ok, {} falhas, {} tentativas",
        ok,
        failures,
        attempts
    );
    nexo_sys::exit(if ok == REQUESTS && failures >= 1 {
        0
    } else {
        83
    })
}

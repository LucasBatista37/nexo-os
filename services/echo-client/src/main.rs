//! `echo-client` — cliente do bring-up no protocolo tipado `nexo.svc` v1.0: pede conexões ao
//! `svcmgr` (handle 0, `connect` → resposta com o canal, ou erro remoto 2 = tente de novo) e
//! faz N pedidos `echo`; quando o serviço cai (PeerClosed), reconecta.
#![no_std]
#![no_main]

use nexo_proto::ProtoError;
use nexo_proto::svc::{ConnectRequest, EchoRequest, decode_connect_response, decode_echo_response};
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
    let mut buf = [0u8; 256];
    let mut out = [0u8; 256];
    let mut hs = [0u32; 2];
    while ok < REQUESTS && attempts < MAX_ATTEMPTS {
        attempts += 1;
        let m = ConnectRequest {}.encode_msg(&mut out).unwrap_or(0);
        if nexo_sys::channel_send(ctl, &out[..m], &[]) != Status::Ok {
            nexo_sys::exit(80);
        }
        let conn = match nexo_sys::channel_recv(ctl, &mut buf, &mut hs) {
            Ok((n, nh)) => match decode_connect_response(&buf[..n]) {
                Ok(_) if nh == 1 => hs[0], // o canal viaja no vetor de handles
                Ok(_) => nexo_sys::exit(81),
                Err(ProtoError::Remote(2)) => continue, // servico reiniciando: tenta de novo
                Err(_) => nexo_sys::exit(81),
            },
            _ => nexo_sys::exit(81),
        };
        let mut rq = EchoRequest {
            text: [0; 64],
            text_len: 0,
        };
        let mut txt = nexo_rt::Buf::<32>::new();
        use core::fmt::Write as _;
        let _ = write!(txt, "ping {}", ok + 1);
        rq.text[..txt.as_bytes().len()].copy_from_slice(txt.as_bytes());
        rq.text_len = txt.as_bytes().len() as u32;
        let m = rq.encode_msg(&mut out).unwrap_or(0);
        if nexo_sys::channel_send(conn, &out[..m], &[]) != Status::Ok {
            nexo_sys::handle_close(conn);
            failures += 1;
            continue;
        }
        match nexo_sys::channel_recv(conn, &mut buf, &mut hs) {
            Ok((n, 0)) => match decode_echo_response(&buf[..n]) {
                Ok(r) if r.text().starts_with(b"echo: ping ") => ok += 1,
                _ => {
                    log!("echo-client: resposta invalida");
                    nexo_sys::exit(82);
                }
            },
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

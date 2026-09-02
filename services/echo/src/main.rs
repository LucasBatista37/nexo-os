//! `echo` — serviço de eco, agora no protocolo tipado `nexo.svc` v1.0. Handle 0 = canal de
//! controle com o `svcmgr`: cada `serve{chan}` traz um canal de cliente; o serviço atende um
//! `echo{text}` e responde `echo: <text>`. Depois de `RDI` pedidos atendidos, cai de
//! propósito (acesso a página não mapeada) para exercitar o reinício.
#![no_std]
#![no_main]

use nexo_proto::svc::{self, EchoResponse, Request};
use nexo_rt::log;
use nexo_sys::abi::Status;

#[unsafe(no_mangle)]
pub extern "C" fn _start(crash_after: u64) -> ! {
    let control: nexo_sys::Handle = 0;
    let mut served = 0u64;
    let mut buf = [0u8; 256];
    let mut out = [0u8; 256];
    let mut hs = [0u32; 2];
    log!(
        "echo: pid {} pronto (cai apos {} pedidos)",
        nexo_sys::get_pid(),
        crash_after
    );
    loop {
        let (n, nh) = match nexo_sys::channel_recv(control, &mut buf, &mut hs) {
            Ok(v) => v,
            Err(Status::PeerClosed) => {
                log!("echo: controle fechado; saindo apos {} pedidos", served);
                nexo_sys::exit(0)
            }
            Err(_) => nexo_sys::exit(20),
        };
        let Ok(Request::Serve(rq)) = svc::decode_request_with_handles(&buf[..n], &hs[..nh]) else {
            continue;
        };
        let client = rq.chan;
        let mut none = [0u32; 1];
        if let Ok((m, _)) = nexo_sys::channel_recv(client, &mut buf, &mut none) {
            if served >= crash_after {
                log!("echo: caindo de proposito no pedido {}", served + 1);
                // SAFETY: deliberadamente inválido — o kernel encerra este processo.
                let v = unsafe { core::ptr::read_volatile(0x10 as *const u64) };
                nexo_sys::exit(21 + (v & 1) as i64)
            }
            let reply_len = match svc::decode_request_with_handles(&buf[..m], &[]) {
                Ok(Request::Echo(erq)) => {
                    let mut r = EchoResponse {
                        text: [0; 96],
                        text_len: 0,
                    };
                    let pfx = b"echo: ";
                    let t = erq.text();
                    let tl = t.len().min(96 - pfx.len());
                    r.text[..pfx.len()].copy_from_slice(pfx);
                    r.text[pfx.len()..pfx.len() + tl].copy_from_slice(&t[..tl]);
                    r.text_len = (pfx.len() + tl) as u32;
                    r.encode_msg(&mut out).unwrap_or(0)
                }
                _ => svc::encode_error(svc::EchoRequest::METHOD_ID, 1, &mut out).unwrap_or(0),
            };
            let _ = nexo_sys::channel_send(client, &out[..reply_len], &[]);
            served += 1;
        }
        nexo_sys::handle_close(client);
    }
}

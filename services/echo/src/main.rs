//! `echo` — serviço de eco. Handle 0 = canal de controle com o `svcmgr`:
//! cada mensagem `serve` traz um canal de cliente; o serviço lê um pedido e
//! responde `echo: <pedido>`. Depois de `RDI` pedidos atendidos, cai de
//! propósito (acesso a página não mapeada) para exercitar o reinício.
#![no_std]
#![no_main]

use nexo_rt::log;
use nexo_sys::abi::Status;

#[unsafe(no_mangle)]
pub extern "C" fn _start(crash_after: u64) -> ! {
    let control: nexo_sys::Handle = 0;
    let mut served = 0u64;
    let mut buf = [0u8; 128];
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
        if &buf[..n] != b"serve" || nh != 1 {
            continue;
        }
        let client = hs[0];
        let mut req = [0u8; 64];
        let mut none = [0u32; 1];
        if let Ok((m, _)) = nexo_sys::channel_recv(client, &mut req, &mut none) {
            if served >= crash_after {
                log!("echo: caindo de proposito no pedido {}", served + 1);
                // SAFETY: deliberadamente inválido — o kernel encerra este processo.
                let v = unsafe { core::ptr::read_volatile(0x10 as *const u64) };
                nexo_sys::exit(21 + (v & 1) as i64)
            }
            let mut reply = nexo_rt::Buf::<96>::new();
            use core::fmt::Write as _;
            let _ = write!(
                reply,
                "echo: {}",
                core::str::from_utf8(&req[..m]).unwrap_or("?")
            );
            let _ = nexo_sys::channel_send(client, reply.as_bytes(), &[]);
            served += 1;
        }
        nexo_sys::handle_close(client);
    }
}

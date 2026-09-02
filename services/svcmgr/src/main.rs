//! `svcmgr` — gerenciador de serviços mínimo (Fase 2): inicia o serviço `echo`
//! com um canal de controle, atende pedidos de conexão do `echo-client`
//! e, quando detecta que o serviço morreu, reinicia-o (até `MAX_RESTARTS`)
//! sem reiniciar o kernel. Sai com o número de reinícios realizados.
#![no_std]
#![no_main]

use nexo_proto::svc::{self, ConnectResponse, Request, ServeRequest};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const MAX_RESTARTS: i64 = 3;
/// O serviço cai depois de atender este número de pedidos.
const ECHO_CRASH_AFTER: u64 = 3;

struct Service {
    process: Handle,
    control: Handle,
}

fn start_echo() -> Option<Service> {
    let (mine, theirs) = nexo_sys::channel_create().ok()?;
    let process = match nexo_sys::process_spawn("echo", ECHO_CRASH_AFTER, &[theirs]) {
        Ok(h) => h,
        Err(e) => {
            log!("svcmgr: falha ao iniciar echo: {:?}", e);
            nexo_sys::handle_close(mine);
            return None;
        }
    };
    let (pid, _) = nexo_sys::process_info(process).unwrap_or((0, false));
    log!("svcmgr: echo iniciado (pid {})", pid);
    Some(Service {
        process,
        control: mine,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut restarts: i64 = 0;
    let Some(mut echo) = start_echo() else {
        nexo_sys::exit(40)
    };
    // Cliente recebe um canal de controle para pedir conexões.
    let (client_ctl, client_side) = match nexo_sys::channel_create() {
        Ok(p) => p,
        Err(_) => nexo_sys::exit(41),
    };
    let client = match nexo_sys::process_spawn("echo-client", 0, &[client_side]) {
        Ok(h) => h,
        Err(_) => nexo_sys::exit(42),
    };
    let mut buf = [0u8; 256];
    let mut out = [0u8; 256];
    let mut hs = [0u32; 2];
    loop {
        match nexo_sys::channel_recv(client_ctl, &mut buf, &mut hs) {
            Ok((n, nh))
                if matches!(
                    svc::decode_request_with_handles(&buf[..n], &hs[..nh]),
                    Ok(Request::Connect(_))
                ) =>
            {
                // Serviço vivo? Se caiu, reinicia (política: até MAX_RESTARTS).
                if let Ok((pid, true)) = nexo_sys::process_info(echo.process) {
                    let code = nexo_sys::process_wait(echo.process).unwrap_or(-99);
                    nexo_sys::handle_close(echo.process);
                    nexo_sys::handle_close(echo.control);
                    if restarts >= MAX_RESTARTS {
                        log!(
                            "svcmgr: echo (pid {}) caiu com {} e o limite de reinicios foi atingido",
                            pid,
                            code
                        );
                        nexo_sys::exit(43);
                    }
                    restarts += 1;
                    log!(
                        "svcmgr: echo (pid {}) caiu com {}; reiniciando ({}/{})",
                        pid,
                        code,
                        restarts,
                        MAX_RESTARTS
                    );
                    echo = match start_echo() {
                        Some(s) => s,
                        None => nexo_sys::exit(44),
                    };
                }
                // Conexão nova: um canal por pedido; uma ponta vai ao serviço, outra ao cliente.
                let (for_client, for_service) = match nexo_sys::channel_create() {
                    Ok(p) => p,
                    Err(_) => nexo_sys::exit(45),
                };
                let m = ServeRequest { chan: for_service }
                    .encode_msg(&mut out)
                    .unwrap_or(0);
                if nexo_sys::channel_send(echo.control, &out[..m], &[for_service]) != Status::Ok {
                    // Serviço morreu entre a checagem e o envio: o cliente tenta de novo
                    // (erro remoto 2 do nexo.svc).
                    nexo_sys::handle_close(for_client);
                    let m =
                        svc::encode_error(svc::ConnectRequest::METHOD_ID, 2, &mut out).unwrap_or(0);
                    let _ = nexo_sys::channel_send(client_ctl, &out[..m], &[]);
                    continue;
                }
                let m = ConnectResponse { chan: for_client }
                    .encode_msg(&mut out)
                    .unwrap_or(0);
                if nexo_sys::channel_send(client_ctl, &out[..m], &[for_client]) != Status::Ok {
                    nexo_sys::exit(46);
                }
            }
            Ok((n, _)) => {
                log!("svcmgr: pedido desconhecido ({} bytes)", n);
            }
            Err(Status::PeerClosed) => break, // cliente terminou
            Err(e) => {
                log!("svcmgr: erro no canal do cliente: {:?}", e);
                nexo_sys::exit(47);
            }
        }
    }
    let client_code = nexo_sys::process_wait(client).unwrap_or(-1);
    // Fechar o controle faz o serviço sair por PeerClosed.
    nexo_sys::handle_close(echo.control);
    let echo_code = nexo_sys::process_wait(echo.process).unwrap_or(-1);
    log!(
        "svcmgr: cliente saiu com {}, echo com {}, {} reinicio(s); {} processos vivos",
        client_code,
        echo_code,
        restarts,
        nexo_sys::debug_info(4)
    );
    if client_code != 0 {
        nexo_sys::exit(48);
    }
    nexo_sys::exit(restarts)
}

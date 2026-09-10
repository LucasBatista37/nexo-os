//! `svcmgr` — gerenciador de serviços (Fase 2): inicia os serviços da tabela **declarada**
//! respeitando as dependências entre eles, espera cada dependência **anunciar prontidão**
//! antes de soltar quem depende dela, atende pedidos de conexão do `echo-client` e, quando
//! detecta que um serviço morreu, reinicia-o (até `MAX_RESTARTS`) sem reiniciar o kernel.
//! Sai com o número de reinícios realizados.
//!
//! Duas coisas que o gerenciador **não** faz, de propósito:
//! - não decide a ordem de partida no código: a ordem sai de [`SERVICOS`] por
//!   `nexo_svcdep::ordem`, que recusa ciclos e dependências inexistentes antes de iniciar
//!   qualquer coisa (falha cedo e por inteiro — uma tabela errada não inicia meio sistema);
//! - não dorme esperando um serviço "provavelmente" já estar de pé: espera o anúncio de
//!   prontidão (`nexo.svc` v1.1, método `ready`), com prazo. Sem anúncio no prazo, os
//!   dependentes **não** são iniciados e o motivo vai ao log — falha fechada.
#![no_std]
#![no_main]

use nexo_proto::svc::{self, ConnectResponse, Request, ServeRequest};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const MAX_RESTARTS: i64 = 3;
/// O serviço cai depois de atender este número de pedidos.
const ECHO_CRASH_AFTER: u64 = 3;
/// Prazo para um serviço anunciar prontidão antes de seus dependentes serem abandonados.
const PRONTIDAO_NS: u64 = 2_000_000_000;
/// Teto da tabela (o resolvedor aceita até `nexo_svcdep::MAX_SERVICOS`).
const MAX_SERVICOS: usize = 8;

/// Um serviço declarado: nome no initrd, argumento inicial e de quem depende.
struct Declaracao {
    nome: &'static str,
    arg: u64,
    depende: &'static [&'static str],
}

/// A tabela. Declarada **fora** da ordem de partida de propósito: quem ordena é o resolvedor,
/// não quem escreve a lista — é essa a diferença entre uma dependência declarada e um
/// comentário dizendo "inicie isto antes daquilo".
static SERVICOS: &[Declaracao] = &[
    Declaracao {
        nome: "echo-client",
        arg: 0,
        depende: &["echo"],
    },
    Declaracao {
        nome: "echo",
        arg: ECHO_CRASH_AFTER,
        depende: &[],
    },
];

struct Service {
    nome: &'static str,
    process: Handle,
    control: Handle,
}

fn iniciar(d: &Declaracao) -> Option<Service> {
    let (mine, theirs) = nexo_sys::channel_create().ok()?;
    let process = match nexo_sys::process_spawn(d.nome, d.arg, &[theirs]) {
        Ok(h) => h,
        Err(e) => {
            log!("svcmgr: falha ao iniciar {}: {:?}", d.nome, e);
            nexo_sys::handle_close(mine);
            return None;
        }
    };
    let (pid, _) = nexo_sys::process_info(process).unwrap_or((0, false));
    log!("svcmgr: {} iniciado (pid {})", d.nome, pid);
    Some(Service {
        nome: d.nome,
        process,
        control: mine,
    })
}

/// Espera o anúncio de prontidão do serviço no seu canal de controle.
///
/// Mensagens que não sejam `ready` são ignoradas (o serviço pode falar antes de estar pronto);
/// o par fechado ou o prazo esgotado devolvem `false`, e quem depende dele não é iniciado.
fn esperar_prontidao(svc_ctl: Handle, nome: &str) -> bool {
    let t0 = nexo_sys::time_now();
    let prazo = t0 + PRONTIDAO_NS;
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 2];
    loop {
        let agora = nexo_sys::time_now();
        if agora >= prazo {
            log!("svcmgr: {} nao anunciou prontidao no prazo", nome);
            return false;
        }
        match nexo_sys::channel_wait_any_timeout(&[svc_ctl], prazo - agora) {
            Ok(_) => match nexo_sys::channel_recv(svc_ctl, &mut buf, &mut hs) {
                Ok((n, nh)) => {
                    if let Ok(Request::Ready(_)) =
                        svc::decode_request_with_handles(&buf[..n], &hs[..nh])
                    {
                        log!(
                            "svcmgr: {} pronto em {} ms",
                            nome,
                            (nexo_sys::time_now() - t0) / 1_000_000
                        );
                        return true;
                    }
                }
                Err(e) => {
                    log!("svcmgr: {} morreu antes de ficar pronto ({:?})", nome, e);
                    return false;
                }
            },
            Err(Status::TimedOut) => {
                log!("svcmgr: {} nao anunciou prontidao no prazo", nome);
                return false;
            }
            Err(e) => {
                log!("svcmgr: erro esperando prontidao de {}: {:?}", nome, e);
                return false;
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut restarts: i64 = 0;

    // 1. A ordem de partida sai da tabela, e a tabela é validada ANTES de iniciar qualquer
    //    coisa: ciclo, dependência inexistente ou nome repetido derrubam o gerenciador aqui,
    //    com o culpado nomeado, em vez de deixarem meio sistema de pé.
    let mut specs = [nexo_svcdep::Spec {
        nome: "",
        depende: &[],
    }; MAX_SERVICOS];
    if SERVICOS.len() > MAX_SERVICOS {
        log!(
            "svcmgr: tabela com {} servicos excede o teto",
            SERVICOS.len()
        );
        nexo_sys::exit(39);
    }
    for (i, d) in SERVICOS.iter().enumerate() {
        specs[i] = nexo_svcdep::Spec {
            nome: d.nome,
            depende: d.depende,
        };
    }
    let mut ordem = [0usize; MAX_SERVICOS];
    let n = match nexo_svcdep::ordem(&specs[..SERVICOS.len()], &mut ordem) {
        Ok(n) => n,
        Err(e) => {
            log!("svcmgr: tabela de servicos invalida: {:?}", e);
            nexo_sys::exit(38);
        }
    };

    // 2. Inicia na ordem resolvida. Um serviço só começa quando TODAS as suas dependências
    //    anunciaram prontidão; se alguma não anunciar, ele não começa (falha fechada).
    let mut vivos: [Option<Service>; MAX_SERVICOS] = [const { None }; MAX_SERVICOS];
    let mut prontos = [false; MAX_SERVICOS];
    for k in 0..n {
        let d = &SERVICOS[ordem[k]];
        let faltando = d.depende.iter().find(|dep| {
            !SERVICOS
                .iter()
                .position(|o| o.nome == **dep)
                .is_some_and(|j| prontos[j])
        });
        if let Some(dep) = faltando {
            log!(
                "svcmgr: {} nao iniciado: dependencia {} nao ficou pronta",
                d.nome,
                dep
            );
            continue;
        }
        let Some(svc) = iniciar(d) else {
            continue;
        };
        // Esperar prontidão só faz sentido para quem tem dependentes — para os demais seria
        // um bloqueio sem ninguém do outro lado à espera.
        let tem_dependentes = SERVICOS.iter().any(|o| o.depende.contains(&d.nome));
        if !tem_dependentes || esperar_prontidao(svc.control, d.nome) {
            prontos[ordem[k]] = true;
        }
        vivos[ordem[k]] = Some(svc);
    }

    // 3. O resto é o fluxo de bring-up de sempre: o cliente pede conexões, o eco atende, e um
    //    eco que cai é reiniciado sem reiniciar o kernel.
    let idx_echo = SERVICOS.iter().position(|d| d.nome == "echo").unwrap_or(0);
    let idx_cliente = SERVICOS
        .iter()
        .position(|d| d.nome == "echo-client")
        .unwrap_or(0);
    let Some(mut echo) = vivos[idx_echo].take() else {
        nexo_sys::exit(40)
    };
    let Some(cliente) = vivos[idx_cliente].take() else {
        nexo_sys::exit(41)
    };
    let client_ctl = cliente.control;
    let client = cliente.process;
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
                    echo = match iniciar(&SERVICOS[idx_echo]) {
                        Some(s) => s,
                        None => nexo_sys::exit(44),
                    };
                    // O serviço reiniciado volta a anunciar prontidão: só depois disso ele
                    // recebe o `serve` do cliente que está esperando.
                    if !esperar_prontidao(echo.control, echo.nome) {
                        nexo_sys::exit(49);
                    }
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

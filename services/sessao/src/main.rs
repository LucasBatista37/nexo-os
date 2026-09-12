//! `sessao` — orquestrador da sessão gráfica (Plano §Fase 5: "criar login, bloqueio e sessão").
//!
//! Até aqui cada peça do desktop (compositor, Faixa de Atividades, greeter, terminal, shell)
//! existia e era testada, mas a composição vivia nos testes: `make run` arrancava o kernel e
//! parava no `idle`. Este programa é a composição de verdade — o que o kernel inicia com
//! `desktop=1` e o que faz o sistema arrancar em si mesmo:
//!
//! 1. sobe o `wm` e assina os drivers de entrada (todos empurram no **mesmo** canal, que vira a
//!    fonte do compositor por `set_input`);
//! 2. entrega a sessão privilegiada ao `shellui` (Faixa de Atividades), que passa a ser o
//!    *broker* de sessões — a partir daqui ninguém fala com o `wm` sem passar por ele;
//! 3. exige o login: o `greeter` captura a entrada e só a devolve com a senha certa;
//! 4. abre o `term` com o `shell` de diagnóstico dentro (sobre o `vfs` montado a partir do que
//!    o `devmgr` encontrou); se o terminal morrer, reabre-o;
//! 5. bloqueia a pedido: Meta+L chega ao shell como evento do compositor, o shell manda `lock`
//!    e o greeter volta pelo mesmo caminho do arranque.
//!
//! Argumento: bits 0..7 = quantos canais de `inputdev` chegam nos handles 0..n; bit 8 = o
//! handle seguinte é o canal-cliente do `devmgr` (quando há disco); bit 9 = o handle seguinte
//! é a concessão do dispositivo de vídeo, entregue ao `wm` para apresentar na tela. Cada passo escreve um
//! marcador `sessao: …` no log: é por eles que o cenário `desktop` sabe onde a sessão chegou.
#![no_std]
#![no_main]

use nexo_proto::{input, wm};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

/// Máximo de drivers de entrada aceites.
const MAX_INPUTS: usize = 4;

fn fail(code: i64, what: &str) -> ! {
    log!("sessao: falha: {}", what);
    nexo_sys::exit(code)
}

/// RPC simples: envia `req` (com handles) e devolve o tamanho da resposta em `buf`.
fn rpc(
    ch: Handle,
    req: &[u8],
    handles: &[Handle],
    buf: &mut [u8],
    hs: &mut [u32; 1],
) -> (usize, usize) {
    if nexo_sys::channel_send(ch, req, handles) != Status::Ok {
        fail(10, "send rpc");
    }
    match nexo_sys::channel_recv(ch, buf, hs) {
        Ok(v) => v,
        Err(_) => fail(11, "recv rpc"),
    }
}

/// Pede ao broker (`shellui`) uma sessão `nexo.wm` nova. O canal do broker também traz
/// eventos não solicitados (`activated`, `copen`, `cclosed`): são ignorados até vir `sess`.
fn nova_sessao(broker: Handle, buf: &mut [u8; 256], hs: &mut [u32; 1]) -> Handle {
    if nexo_sys::channel_send(broker, b"sess", &[]) != Status::Ok {
        fail(20, "pedido de sessao ao broker");
    }
    loop {
        match nexo_sys::channel_recv(broker, buf, hs) {
            Ok((n, 1)) if &buf[..n] == b"sess" => return hs[0],
            Ok((n, nh)) => {
                // evento do shell sem interesse aqui; um handle inesperado é fechado
                if nh == 1 {
                    let _ = nexo_sys::handle_close(hs[0]);
                }
                let _ = n;
            }
            Err(_) => fail(21, "broker fechou"),
        }
    }
}

/// Espera uma mensagem exata num canal-pipe, registando as intermédias.
fn espera(pipe: Handle, quer: &[u8], buf: &mut [u8; 256], hs: &mut [u32; 1]) -> bool {
    loop {
        match nexo_sys::channel_recv(pipe, buf, hs) {
            Ok((n, _)) if &buf[..n] == quer => return true,
            Ok((n, _)) => {
                if &buf[..n] == b"wrong" {
                    log!("sessao: senha errada; continua bloqueada");
                }
            }
            Err(e) => {
                log!(
                    "sessao: pipe fechou/errou a esperar '{}': {:?}",
                    core::str::from_utf8(quer).unwrap_or("?"),
                    e
                );
                return false;
            }
        }
    }
}

/// Abre uma sessão do vfs (`open{chan, mounts}`; 0 = a mesma árvore) e devolve a nossa ponta.
fn sessao_vfs(vfs: Handle, mounts: u64, buf: &mut [u8; 256], hs: &mut [u32; 1]) -> Handle {
    use nexo_proto::fs::{OpenRequest, decode_open_response};
    let (mine, theirs) = nexo_sys::channel_create().unwrap_or_else(|_| fail(60, "canal do vfs"));
    let mut out = [0u8; 4096];
    let m = OpenRequest {
        chan: theirs,
        mounts,
    }
    .encode_msg(&mut out)
    .unwrap_or_else(|_| fail(61, "enc open vfs"));
    let (n, _) = rpc(vfs, &out[..m], &[theirs], buf, hs);
    if decode_open_response(&buf[..n]).is_err() {
        fail(62, "vfs recusou a sessao");
    }
    mine
}

/// Abre o terminal com o shell dentro; devolve o pipe do terminal (fecha quando ele morre).
fn abre_terminal(broker: Handle, vfs: Handle, buf: &mut [u8; 256], hs: &mut [u32; 1]) -> Handle {
    let sess = nova_sessao(broker, buf, hs);
    let (ta, tb) = nexo_sys::channel_create().unwrap_or_else(|_| fail(40, "canal do term"));
    let (ca, cb) = nexo_sys::channel_create().unwrap_or_else(|_| fail(41, "canal da console"));
    // o shell recebe uma SESSÃO do vfs (v1.4), não uma cópia do canal: duas cópias do mesmo
    // canal seriam dois clientes a intercalar pedidos e respostas
    let vfs_shell = sessao_vfs(vfs, 0, buf, hs);
    if nexo_sys::process_spawn("term", 0, &[ta, cb]).is_err() {
        fail(43, "spawn term");
    }
    if nexo_sys::channel_send(tb, b"sess", &[sess]) != Status::Ok {
        fail(44, "sessao ao term");
    }
    if !espera(tb, b"pronto", buf, hs) {
        fail(45, "term nao ficou pronto");
    }
    if nexo_sys::process_spawn("shell", 0, &[ca, vfs_shell]).is_err() {
        fail(46, "spawn shell");
    }
    log!("sessao: terminal aberto com o shell de diagnostico");
    tb
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(arg: u64) -> ! {
    let n_inputs = (arg & 0xff) as usize;
    let tem_devmgr = arg & 0x100 != 0;
    if n_inputs > MAX_INPUTS {
        fail(1, "drivers de entrada a mais");
    }
    let devmgr: Option<Handle> = tem_devmgr.then_some(n_inputs as Handle);
    let video: Option<Handle> =
        (arg & 0x200 != 0).then_some((n_inputs + usize::from(tem_devmgr)) as Handle);
    let mut buf = [0u8; 256];
    let mut out = [0u8; 256];
    let mut hs = [0u32; 1];

    // 1. compositor (com a tela, se o kernel a concedeu)
    let (wa, wb) = nexo_sys::channel_create().unwrap_or_else(|_| fail(2, "canal do wm"));
    let spawned = match video {
        Some(v) => nexo_sys::process_spawn("wm", 0, &[wa, v]),
        None => nexo_sys::process_spawn("wm", 0, &[wa]),
    };
    if spawned.is_err() {
        fail(3, "spawn wm");
    }
    let wm_ch: Handle = wb;
    let m = wm::OutputRequest { display: 0 }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(4, "enc output"));
    let (n, nh) = rpc(wm_ch, &out[..m], &[], &mut buf, &mut hs);
    let saida = wm::decode_output_response(&buf[..n]).unwrap_or_else(|_| fail(5, "output"));
    if nh == 1 {
        let _ = nexo_sys::handle_close(hs[0]); // a memória da saída não interessa aqui
    }
    let (w, h) = (saida.w.max(1) as u32, saida.h.max(1) as u32);

    // 2. entrada: um canal só; cada driver recebe uma cópia da ponta de escrita
    if n_inputs > 0 {
        let (push_wm, push_drv) =
            nexo_sys::channel_create().unwrap_or_else(|_| fail(6, "canal de entrada"));
        for i in 0..n_inputs {
            let drv = i as Handle;
            let ponta = if i + 1 == n_inputs {
                push_drv
            } else {
                nexo_sys::handle_duplicate(push_drv, nexo_sys::abi::RIGHTS_CHANNEL_DEFAULT)
                    .unwrap_or_else(|_| fail(7, "duplicar ponta de entrada"))
            };
            let m = input::SubscribeRequest {
                chan: ponta,
                abs_w: w,
                abs_h: h,
            }
            .encode_msg(&mut out)
            .unwrap_or_else(|_| fail(8, "enc subscribe"));
            let (n, _) = rpc(drv, &out[..m], &[ponta], &mut buf, &mut hs);
            if input::decode_subscribe_response(&buf[..n]).is_err() {
                fail(9, "subscribe recusado");
            }
        }
        let m = wm::SetInputRequest { chan: push_wm }
            .encode_msg(&mut out)
            .unwrap_or_else(|_| fail(12, "enc set_input"));
        let (n, _) = rpc(wm_ch, &out[..m], &[push_wm], &mut buf, &mut hs);
        if wm::decode_set_input_response(&buf[..n]).is_err() {
            fail(13, "set_input recusado");
        }
    }
    log!(
        "sessao: compositor {}x{} com {} driver(s) de entrada",
        w,
        h,
        n_inputs
    );

    // 3. armazenamento: o devmgr entrega fs/esp; o vfs monta o que houver
    let mut fs: Option<Handle> = None;
    let mut esp: Option<Handle> = None;
    if let Some(dm) = devmgr {
        while let Ok((n, nh)) = nexo_sys::channel_recv(dm, &mut buf, &mut hs) {
            match (&buf[..n], nh) {
                (b"fs", 1) => fs = Some(hs[0]),
                (b"esp", 1) => esp = Some(hs[0]),
                (b"rng", 1) => {
                    let _ = nexo_sys::handle_close(hs[0]);
                }
                (b"done", _) => break,
                _ => {}
            }
        }
    }
    let solto = |nome: &str| -> Handle {
        // montagem ausente: ponta sem par (o vfs responde NotFound ao uso)
        let (a, b) = nexo_sys::channel_create().unwrap_or_else(|_| fail(14, nome));
        let _ = nexo_sys::handle_close(b);
        a
    };
    let fs_h = fs.unwrap_or_else(|| solto("canal solto fs"));
    let esp_h = esp.unwrap_or_else(|| solto("canal solto esp"));
    let (vx, vy) = nexo_sys::channel_create().unwrap_or_else(|_| fail(15, "canal do vfs"));
    if nexo_sys::process_spawn("vfs", 0, &[fs_h, esp_h, vx]).is_err() {
        fail(16, "spawn vfs");
    }
    log!(
        "sessao: vfs montado ({}{})",
        if fs.is_some() { "/disk" } else { "sem /disk" },
        if esp.is_some() { ", /boot" } else { "" }
    );

    // 4. Faixa de Atividades: recebe a sessão privilegiada e passa a ser o broker
    let (pa, pb) = nexo_sys::channel_create().unwrap_or_else(|_| fail(17, "canal do shellui"));
    if nexo_sys::process_spawn("shellui", 0, &[wm_ch, pa]).is_err() {
        fail(18, "spawn shellui");
    }
    let broker: Handle = pb;
    log!("sessao: faixa de atividades no ar");

    // 5. login: o greeter captura a entrada até a senha certa
    bloqueia(broker, vy, "arranque", &mut buf, &mut hs);

    // 6. terminal com o shell; reaberto se morrer. Meta+L (via shell) bloqueia de novo.
    let mut term = abre_terminal(broker, vy, &mut buf, &mut hs);
    log!("sessao: pronta (Meta+L bloqueia)");
    loop {
        let _ = nexo_sys::channel_wait_any(&[term, broker]);
        match nexo_sys::channel_try_recv(term, &mut buf, &mut hs) {
            Ok((n, _)) if &buf[..n] == b"fim" => {}
            Ok(_) => {}
            Err(Status::WouldBlock) => {}
            Err(_) => {
                let _ = nexo_sys::handle_close(term);
                log!("sessao: terminal fechou; a reabrir");
                term = abre_terminal(broker, vy, &mut buf, &mut hs);
            }
        }
        match nexo_sys::channel_try_recv(broker, &mut buf, &mut hs) {
            Ok((n, 0)) if &buf[..n] == b"lock" => {
                log!("sessao: bloqueio a pedido");
                bloqueia(broker, vy, "a pedido", &mut buf, &mut hs);
            }
            Ok((_, 1)) => {
                let _ = nexo_sys::handle_close(hs[0]);
            }
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => fail(50, "faixa de atividades morreu"),
        }
    }
}

/// Bloqueia a sessão: sobe um `greeter` numa sessão nova do compositor; ele captura a entrada
/// e só a devolve com a senha certa. Volta quando desbloqueou. Usado no arranque e a pedido
/// (Meta+L) — a mesma tela, o mesmo caminho.
fn bloqueia(broker: Handle, vfs: Handle, motivo: &str, buf: &mut [u8; 256], hs: &mut [u32; 1]) {
    let sess_g = nova_sessao(broker, buf, hs);
    let (ga, gb) = nexo_sys::channel_create().unwrap_or_else(|_| fail(30, "canal do greeter"));
    if nexo_sys::process_spawn("greeter", 0, &[ga]).is_err() {
        fail(31, "spawn greeter");
    }
    // uma sessão do vfs para as preferências (carregar; na primeira execução, guardar)
    let fs_g = sessao_vfs(vfs, 0, buf, hs);
    if nexo_sys::channel_send(gb, b"fs", &[fs_g]) != Status::Ok {
        fail(35, "fs ao greeter");
    }
    if nexo_sys::channel_send(gb, b"sess", &[sess_g]) != Status::Ok {
        fail(32, "sessao ao greeter");
    }
    if !espera(gb, b"locked", buf, hs) {
        fail(33, "greeter nao bloqueou");
    }
    log!("sessao: bloqueada — aguardando a senha");
    if !espera(gb, b"unlocked", buf, hs) {
        fail(34, "greeter morreu antes de desbloquear");
    }
    let _ = nexo_sys::handle_close(gb);
    log!("sessao: desbloqueada ({})", motivo);
}

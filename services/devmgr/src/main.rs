//! `devmgr` — gerenciador de dispositivos. Handle 0 = concessão raiz (`ADMIN`),
//! handle 1 = canal do cliente. Enumera PCI, faz *binding* por **IDs** (VirtIO: vendor +
//! tipo) e por **propriedades** (classe/subclasse/prog_if: NVMe e AHCI), deriva uma concessão
//! restrita a cada função (`device_open`) e inicia o driver correspondente com ela.
//!
//! Todos os discos falam o mesmo `nexo.block`, então o papel de cada um é decidido **depois**
//! de perguntar a identidade (serial e somente-leitura), não pelo barramento: o disco de
//! dados é o `nexodata` (senão o primeiro gravável) e o de boot é o `nexoboot` (senão o
//! primeiro somente-leitura). Depois sobe o `fs` sobre o disco de dados e o `espfs` sobre o
//! de boot, e entrega ao cliente os canais de serviço: `fs`+handle, `rng`+handle e `done`.
#![no_std]
#![no_main]

use nexo_proto::block::{self, IdentityRequest};
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};

const ROOT: Handle = 0;
const CLIENT: Handle = 1;

/// Um disco encontrado: quem o serve, o canal `nexo.block` e a identidade que ele deu.
#[derive(Clone, Copy)]
struct Disco {
    driver: &'static str,
    canal: Handle,
    serial: [u8; 20],
    somente_leitura: bool,
}

/// Papel do driver na composição do sistema.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Papel {
    /// Serve `nexo.block` (o papel concreto — dados, boot, A/B — sai da identidade do disco).
    Bloco,
    /// Serve `nexo.rng`.
    Rng,
}

/// Binding: por **IDs** (VirtIO: vendor + tipo do dispositivo) e por **propriedades**
/// (classe/subclasse/prog_if do PCI, que identificam a interface programável independentemente
/// do fabricante). Devolve o programa do initrd e o papel dele.
fn driver_for(d: &PciInfo) -> Option<(&'static str, Papel)> {
    if d.is_virtio() {
        return match nexo_virtio::device_type(d.device) {
            Some(nexo_virtio::TYPE_BLOCK) => Some(("blockdev", Papel::Bloco)),
            Some(nexo_virtio::TYPE_RNG) => Some(("rngdev", Papel::Rng)),
            _ => None,
        };
    }
    match (d.class, d.subclass, d.prog_if) {
        // armazenamento de massa: NVM Express e SATA em modo AHCI 1.0
        (0x01, 0x08, 0x02) => Some(("nvmedev", Papel::Bloco)),
        (0x01, 0x06, 0x01) => Some(("ahcidev", Papel::Bloco)),
        _ => None,
    }
}

fn fail(code: i64, what: &str) -> ! {
    log!("devmgr: falha: {}", what);
    nexo_sys::exit(code)
}

/// Pergunta ao `blockdev` (op 3) o serial e se é somente leitura.
fn block_identity(ch: Handle) -> (bool, [u8; 20]) {
    let mut buf = [0u8; 128];
    let mut hs = [0u32; 1];
    let Ok(m) = IdentityRequest {}.encode_msg(&mut buf) else {
        return (false, [0; 20]);
    };
    if nexo_sys::channel_send(ch, &buf[..m], &[]) != Status::Ok {
        return (false, [0; 20]);
    }
    let Ok((n, _)) = nexo_sys::channel_recv(ch, &mut buf, &mut hs) else {
        return (false, [0; 20]);
    };
    match block::decode_identity_response(&buf[..n]) {
        Ok(r) => {
            let mut serial = [0u8; 20];
            serial[..r.serial().len().min(20)].copy_from_slice(r.serial());
            (r.read_only != 0, serial)
        }
        Err(_) => (false, [0; 20]),
    }
}

/// Serial legível: o campo tem 20 bytes e NVMe/SATA o preenchem com **espaços** à direita
/// (o VirtIO usa NUL), então corta no primeiro NUL e apara os espaços.
fn serial_str(serial: &[u8; 20]) -> &str {
    let len = serial.iter().position(|&b| b == 0).unwrap_or(20);
    core::str::from_utf8(&serial[..len])
        .unwrap_or("?")
        .trim_end()
}

/// Inicia `driver` para a função `d` com concessão restrita; devolve o canal de serviço.
fn start_driver(driver: &str, d: &PciInfo) -> Result<Handle, Status> {
    let grant = nexo_sys::device_open(ROOT, d.bdf)?;
    let (a, b) = nexo_sys::channel_create()?;
    let proc_h = nexo_sys::process_spawn(driver, 0, &[grant, a])?;
    let (pid, _) = nexo_sys::process_info(proc_h).unwrap_or((0, false));
    log!(
        "devmgr: {:02x}:{:02x}.{} {:04x}:{:04x} -> {} (pid {})",
        d.bdf >> 8,
        (d.bdf >> 3) & 0x1f,
        d.bdf & 7,
        d.vendor,
        d.device,
        driver,
        pid
    );
    let _ = nexo_sys::handle_close(proc_h);
    Ok(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut devs = [PciInfo::default(); 32];
    let n = nexo_sys::pci_enum(ROOT, &mut devs)
        .unwrap_or_else(|_| fail(10, "pci_enum"))
        .min(32);
    // Discos encontrados: (driver, canal, serial, somente-leitura). O papel de cada um é
    // decidido depois de conhecer todos — assim a ordem do barramento não muda o sistema.
    let mut discos: [Option<Disco>; 8] = [None; 8];
    let mut ndiscos = 0;
    let mut rng: Option<Handle> = None;
    let mut bound = 0;
    for d in &devs[..n] {
        let Some((driver, papel)) = driver_for(d) else {
            continue;
        };
        match start_driver(driver, d) {
            Ok(ch) => {
                bound += 1;
                match papel {
                    Papel::Bloco => {
                        let (ro, serial) = block_identity(ch);
                        if ndiscos < discos.len() {
                            log!(
                                "devmgr: disco '{}' por {} ({})",
                                serial_str(&serial),
                                driver,
                                if ro { "somente leitura" } else { "gravavel" }
                            );
                            discos[ndiscos] = Some(Disco {
                                driver,
                                canal: ch,
                                serial,
                                somente_leitura: ro,
                            });
                            ndiscos += 1;
                        } else {
                            let _ = nexo_sys::handle_close(ch);
                        }
                    }
                    Papel::Rng if rng.is_none() => rng = Some(ch),
                    Papel::Rng => {
                        let _ = nexo_sys::handle_close(ch);
                    }
                }
            }
            Err(e) => log!(
                "devmgr: driver {} para {:#06x} falhou: {:?}",
                driver,
                d.bdf,
                e
            ),
        }
    }
    // Papéis por identidade: o disco de dados é o `nexodata` (senão o primeiro gravável) e o
    // de boot é o `nexoboot` (senão o primeiro somente-leitura).
    let escolhe = |discos: &mut [Option<Disco>], nome: &str, ro_desejado: bool| -> Option<Disco> {
        let mut alvo = discos
            .iter()
            .position(|s| s.is_some_and(|d| serial_str(&d.serial) == nome));
        if alvo.is_none() {
            alvo = discos
                .iter()
                .position(|s| s.is_some_and(|d| d.somente_leitura == ro_desejado));
        }
        alvo.and_then(|i| discos[i].take())
    };
    let blk = escolhe(&mut discos, "nexodata", false).map(|d| {
        log!(
            "devmgr: disco de dados '{}' ({}) -> fs",
            serial_str(&d.serial),
            d.driver
        );
        d.canal
    });
    let boot = escolhe(&mut discos, "nexoboot", true).map(|d| {
        log!(
            "devmgr: disco de boot '{}' ({}, somente leitura) -> espfs",
            serial_str(&d.serial),
            d.driver
        );
        d.canal
    });
    log!(
        "devmgr: {} funcao(oes) PCI, {} driver(s) iniciado(s)",
        n,
        bound
    );
    if let Some(blk) = blk {
        let (c, d) = nexo_sys::channel_create().unwrap_or_else(|_| fail(11, "canal"));
        match nexo_sys::process_spawn("fs", 0, &[blk, c]) {
            Ok(h) => {
                let _ = nexo_sys::handle_close(h);
                if nexo_sys::channel_send(CLIENT, b"fs", &[d]) != Status::Ok {
                    fail(12, "entrega do fs");
                }
            }
            Err(e) => log!("devmgr: fs falhou: {:?}", e),
        }
    }
    if let Some(rng) = rng
        && nexo_sys::channel_send(CLIENT, b"rng", &[rng]) != Status::Ok
    {
        fail(13, "entrega do rng");
    }
    if let Some(boot) = boot {
        let (c, d) = nexo_sys::channel_create().unwrap_or_else(|_| fail(11, "canal"));
        match nexo_sys::process_spawn("espfs", 0, &[boot, c]) {
            Ok(h) => {
                let _ = nexo_sys::handle_close(h);
                if nexo_sys::channel_send(CLIENT, b"esp", &[d]) != Status::Ok {
                    fail(15, "entrega do esp");
                }
            }
            Err(e) => log!("devmgr: espfs falhou: {:?}", e),
        }
    }
    // Health check pós-boot do A/B (ADR-0010): o armazenamento subiu — o sistema chegou vivo
    // ao userspace. Sobe o `ahcidev` no SATA integrado do q35 (00:1f.2 — o disco de BOOT,
    // gravável), entrega o canal ao `upd` e manda confirmar o slot arrancado; sem esta
    // confirmação as tentativas do loader esgotam e o boot seguinte volta ao outro slot
    // (rollback automático). Tudo best-effort: sem o SATA ou sem layout A/B, só avisa.
    for slot in discos.iter_mut() {
        let Some(Disco {
            driver,
            canal: blkch,
            ..
        }) = slot.take()
        else {
            continue;
        };
        if driver != "ahcidev" {
            let _ = nexo_sys::handle_close(blkch);
            continue;
        }
        {
            {
                let mut confirmed = false;
                if let Ok((ca, cb)) = nexo_sys::channel_create() {
                    match nexo_sys::process_spawn("upd", 0, &[cb, blkch]) {
                        Ok(h) => {
                            let _ = nexo_sys::handle_close(h);
                            let mut b = [0u8; 16];
                            let mut hs2 = [0u32; 1];
                            if nexo_sys::channel_send(ca, b"confirma", &[]) == Status::Ok
                                && let Ok((rn, _)) = nexo_sys::channel_recv(ca, &mut b, &mut hs2)
                                && rn == 4
                                && b[..3] == *b"ok "
                            {
                                log!(
                                    "devmgr: A/B: slot {} confirmado (health check pos-boot)",
                                    b[3] as char
                                );
                                confirmed = true;
                            }
                        }
                        Err(e) => {
                            log!("devmgr: A/B: upd falhou: {:?}", e);
                            let _ = nexo_sys::handle_close(cb);
                            let _ = nexo_sys::handle_close(blkch);
                        }
                    }
                    let _ = nexo_sys::handle_close(ca);
                } else {
                    let _ = nexo_sys::handle_close(blkch);
                }
                if !confirmed {
                    log!("devmgr: A/B: sem confirmacao (imagem sem layout A/B?)");
                }
            }
        }
    }
    for slot in discos.iter_mut() {
        if let Some(d) = slot.take() {
            let _ = nexo_sys::handle_close(d.canal);
        }
    }
    let _ = nexo_sys::channel_send(CLIENT, b"done", &[]);
    let mut buf = [0u8; 64];
    let mut hs = [0u32; 1];
    loop {
        match nexo_sys::channel_recv(CLIENT, &mut buf, &mut hs) {
            Err(Status::PeerClosed) => {
                log!("devmgr: cliente desconectou; encerrando");
                nexo_sys::exit(0)
            }
            Err(_) => fail(14, "recv"),
            Ok(_) => {}
        }
    }
}

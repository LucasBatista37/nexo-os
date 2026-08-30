//! `devmgr` — gerenciador de dispositivos. Handle 0 = concessão raiz (`ADMIN`),
//! handle 1 = canal do cliente. Enumera PCI, faz *binding* por IDs (tabela abaixo), deriva
//! uma concessão restrita a cada função (`device_open`) e inicia o driver correspondente com
//! ela; depois sobe o `fs` sobre o driver de bloco e entrega ao cliente os canais de serviço:
//! mensagens `fs`+handle, `rng`+handle e `done`.
#![no_std]
#![no_main]

use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::{PciInfo, Status};

const ROOT: Handle = 0;
const CLIENT: Handle = 1;

/// Tabela de binding: (vendor, tipo virtio) → programa do initrd.
fn driver_for(d: &PciInfo) -> Option<&'static str> {
    if !d.is_virtio() {
        return None;
    }
    match nexo_virtio::device_type(d.device) {
        Some(nexo_virtio::TYPE_BLOCK) => Some("blockdev"),
        Some(nexo_virtio::TYPE_RNG) => Some("rngdev"),
        _ => None,
    }
}

fn fail(code: i64, what: &str) -> ! {
    log!("devmgr: falha: {}", what);
    nexo_sys::exit(code)
}

/// Pergunta ao `blockdev` (op 3) o serial e se é somente leitura.
fn block_identity(ch: Handle) -> (bool, [u8; 20]) {
    let mut req = [0u8; 16];
    req[0] = 3;
    let mut reply = [0u8; 32];
    let mut hs = [0u32; 1];
    if nexo_sys::channel_send(ch, &req, &[]) == Status::Ok
        && let Ok((22, _)) = nexo_sys::channel_recv(ch, &mut reply, &mut hs)
        && reply[0] == 0
    {
        let mut serial = [0u8; 20];
        serial.copy_from_slice(&reply[2..22]);
        return (reply[1] != 0, serial);
    }
    (false, [0; 20])
}

fn serial_str(serial: &[u8; 20]) -> &str {
    let len = serial.iter().position(|&b| b == 0).unwrap_or(20);
    core::str::from_utf8(&serial[..len]).unwrap_or("?")
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
    let mut blk: Option<Handle> = None;
    let mut boot: Option<Handle> = None;
    let mut rng: Option<Handle> = None;
    let mut bound = 0;
    for d in &devs[..n] {
        let Some(driver) = driver_for(d) else {
            continue;
        };
        match start_driver(driver, d) {
            Ok(ch) => {
                bound += 1;
                match driver {
                    "blockdev" => {
                        let (ro, serial) = block_identity(ch);
                        if serial_str(&serial) == "nexoboot" || (ro && boot.is_none()) {
                            log!("devmgr: disco de boot (somente leitura) -> espfs");
                            boot = Some(ch);
                        } else if blk.is_none() {
                            log!("devmgr: disco de dados '{}' -> fs", serial_str(&serial));
                            blk = Some(ch);
                        } else {
                            let _ = nexo_sys::handle_close(ch);
                        }
                    }
                    "rngdev" if rng.is_none() => rng = Some(ch),
                    _ => {
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

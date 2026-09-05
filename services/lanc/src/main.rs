//! `lanc` — lançador com **consentimento** (Plano §Fase 6: "implementar permissões declarativas
//! e consentimento"). Antes de executar um app instalado, mostra uma janela com as permissões
//! que o manifesto declara (uma célula de acento por permissão) e os botões **Permitir** e
//! **Negar** (e **Permitir por tempo**: a concessão vale por um prazo; ao expirar o lançador
//! corta o cordão de vida e o app encerra) — e só concede as capacidades depois do clique. O clique é entregue
//! pelo compositor à janela sob o cursor: a decisão vem do usuário, não do app. Negar significa
//! que o app nem é executado.
//! Handle 0 = canal do orquestrador: recebe "sess" (sessão do compositor), depois
//! "abre <nome>" (o 1º traz o canal `nexo.fs`); responde "pedido" → espera o clique →
//! "permitido"/"negado"; "fecha" encerra o app lançado ("fim"). Pipe fechado = sair.
#![no_std]
#![no_main]

use nexo_gfx::{Color, PixelFormat, Rect, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const W: i32 = 48;
const H: i32 = 32;
/// Prazo da permissao temporaria ("Permitir por tempo"): ao expirar, o lancador corta o
/// cordao de vida e o app encerra — a concessao tem validade, nao e para sempre.
const PRAZO_NS: u64 = 1_000_000_000;
const ELF_MAX: usize = 40960;

static mut ELF_BUF: [u8; ELF_MAX] = [0; ELF_MAX];

fn fail(code: i64, what: &str) -> ! {
    log!("lanc: falha: {}", what);
    nexo_sys::exit(code)
}

fn permit_rect() -> Rect {
    Rect::new(2, 12, 20, 8)
}
fn deny_rect() -> Rect {
    Rect::new(26, 12, 20, 8)
}
/// "Permitir por tempo": concede como Permitir, mas so por PRAZO_NS.
fn timed_rect() -> Rect {
    Rect::new(2, 22, 44, 8)
}

/// Cliente `nexo.fs` de leitura (o lançador não escreve nada).
struct Fs {
    ch: Handle,
    req: [u8; 4096],
    reply: [u8; 4096],
}

impl Fs {
    fn rpc(&mut self, m: usize) -> Option<usize> {
        if nexo_sys::channel_send(self.ch, &self.req[..m], &[]) != Status::Ok {
            return None;
        }
        let mut hs = [0u32; 1];
        nexo_sys::channel_recv(self.ch, &mut self.reply, &mut hs)
            .ok()
            .map(|(n, _)| n)
    }

    fn read_all(&mut self, path: &str, out: &mut [u8]) -> Option<usize> {
        use nexo_proto::fs as pfs;
        let mut p = [0u8; 256];
        let n = path.len().min(256);
        p[..n].copy_from_slice(&path.as_bytes()[..n]);
        let m = pfs::StatRequest {
            path: p,
            path_len: n as u32,
        }
        .encode_msg(&mut self.req)
        .ok()?;
        let rn = self.rpc(m)?;
        let st = pfs::decode_stat_response(&self.reply[..rn]).ok()?;
        let (ino, size) = (st.ino, st.size as usize);
        if size > out.len() {
            return None;
        }
        let mut off = 0usize;
        while off < size {
            let want = (size - off).min(3900) as u32;
            let m = pfs::ReadRequest {
                ino,
                offset: off as u64,
                len: want,
            }
            .encode_msg(&mut self.req)
            .ok()?;
            let rn = self.rpc(m)?;
            let r = pfs::decode_read_response(&self.reply[..rn]).ok()?;
            let dl = r.data().len();
            if dl == 0 {
                return None;
            }
            out[off..off + dl].copy_from_slice(r.data());
            off += dl;
        }
        Some(size)
    }
}

/// Versão corrente de `name` (lê `/apps/<name>.cur`).
fn current_version(fs: &mut Fs, name: &str) -> Option<u32> {
    let mut path = [0u8; 96];
    let mut pl = 0;
    for part in ["/apps/", name, ".cur"] {
        path[pl..pl + part.len()].copy_from_slice(part.as_bytes());
        pl += part.len();
    }
    let mut buf = [0u8; 16];
    let p = core::str::from_utf8(&path[..pl]).ok()?;
    let Some(n) = fs.read_all(p, &mut buf) else {
        log!("lanc: read_all('{}') falhou", p);
        return None;
    };
    let r = core::str::from_utf8(&buf[..n])
        .ok()?
        .trim()
        .trim_start_matches('v') // o ponteiro guarda "v<N>", como o diretorio
        .parse()
        .ok();
    if r.is_none() {
        log!("lanc: conteudo de '{}' nao e versao: {:?}", p, &buf[..n]);
    }
    r
}

/// Monta `/apps/<name>.v<v>/<sub>` num buffer.
fn vpath<'a>(name: &str, v: u32, sub: &str, buf: &'a mut [u8; 96]) -> &'a str {
    let mut pl = 0;
    for part in ["/apps/", name, ".v"] {
        buf[pl..pl + part.len()].copy_from_slice(part.as_bytes());
        pl += part.len();
    }
    let mut digits = [0u8; 10];
    let mut d = 0;
    let mut vv = v;
    loop {
        digits[d] = b'0' + (vv % 10) as u8;
        vv /= 10;
        d += 1;
        if vv == 0 {
            break;
        }
    }
    while d > 0 {
        d -= 1;
        buf[pl] = digits[d];
        pl += 1;
    }
    buf[pl] = b'/';
    pl += 1;
    buf[pl..pl + sub.len()].copy_from_slice(sub.as_bytes());
    pl += sub.len();
    core::str::from_utf8(&buf[..pl]).unwrap_or("")
}

/// Pinta a janela de consentimento: uma célula por permissão + Permitir/Negar.
fn draw_consent(base: u64, nperms: usize) {
    // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
    let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
        .unwrap_or_else(|| fail(20, "superficie"));
    s.clear(Color::rgb(0x14, 0x15, 0x18));
    for k in 0..nperms.min(4) {
        s.fill_rect(
            Rect::new(2 + k as i32 * 10, 2, 8, 6),
            Color::rgb(0x6f, 0x9f, 0xff),
        );
    }
    s.fill_rect(permit_rect(), Color::rgb(0x2f, 0xa0, 0x4f));
    s.fill_rect(deny_rect(), Color::rgb(0xc0, 0x30, 0x30));
    s.fill_rect(timed_rect(), Color::rgb(0xe0, 0xa0, 0x20));
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

    // janela de consentimento (uma vez; repintada por pedido)
    let req = wm::CreateSurfaceRequest {
        x: 8,
        y: 8,
        w: W,
        h: H,
        z: 100,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(22, "enc create"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(23, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(sess, &mut buf, &mut hs).unwrap_or_else(|_| fail(24, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(25, "dec create"));
    if nh != 1 {
        fail(26, "sem handle");
    }
    let id = cs.id;
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(27, "map"));
    let mut title = wm::SetTitleRequest {
        id,
        title: [0; 32],
        title_len: 13,
    };
    title.title[..13].copy_from_slice(b"consentimento");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(28, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    let mut fs: Option<Fs> = None;
    let mut app: Option<(Handle, Handle)> = None; // (pipe do app, processo)
    let mut prazo: Option<u64> = None; // permissao temporaria: instante em que expira
    // SAFETY: unico acesso, processo de uma so thread.
    let elf_buf = unsafe { &mut *core::ptr::addr_of_mut!(ELF_BUF) };

    'pedidos: loop {
        // Com uma permissao temporaria em curso, o lancador nao pode bloquear: sonda o
        // orquestrador e o relogio; ao expirar, corta o cordao de vida do app e avisa.
        let (n, nh) = if let Some(fim) = prazo {
            loop {
                match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
                    Ok(v) => break v,
                    Err(Status::WouldBlock) => {
                        if nexo_sys::time_now() >= fim {
                            if let Some((app_pipe, child)) = app.take() {
                                let _ = nexo_sys::handle_close(app_pipe);
                                let _ = nexo_sys::process_wait(child);
                            }
                            prazo = None;
                            log!("lanc: permissao temporaria EXPIROU — app encerrado");
                            let _ = nexo_sys::channel_send(PIPE, b"expirou", &[]);
                            continue 'pedidos;
                        }
                        nexo_sys::sleep_ns(20_000_000);
                    }
                    Err(_) => nexo_sys::exit(0), // cordão de vida
                }
            }
        } else {
            match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
                Ok(v) => v,
                Err(_) => nexo_sys::exit(0), // cordão de vida
            }
        };
        if n > 5 && &buf[..5] == b"abre " {
            if nh == 1 {
                fs = Some(Fs {
                    ch: hs[0],
                    req: [0; 4096],
                    reply: [0; 4096],
                });
            }
            let Some(fsr) = fs.as_mut() else {
                fail(29, "sem canal de arquivos");
            };
            let mut name_buf = [0u8; 64];
            let nl = (n - 5).min(64);
            name_buf[..nl].copy_from_slice(&buf[5..5 + nl]);
            let name = core::str::from_utf8(&name_buf[..nl]).unwrap_or_else(|_| fail(30, "nome"));

            // manifesto: as permissões que serão MOSTRADAS são as que podem ser concedidas
            let v = current_version(fsr, name).unwrap_or_else(|| fail(31, "sem versao"));
            let mut pb = [0u8; 96];
            let mpath = vpath(name, v, "manifest.txt", &mut pb);
            let mut mbuf = [0u8; 256];
            let mn = fsr
                .read_all(mpath, &mut mbuf)
                .unwrap_or_else(|| fail(32, "manifesto"));
            let manifest =
                nexo_pkg::Manifest::parse(&mbuf[..mn]).unwrap_or_else(|_| fail(33, "manifesto"));
            let nperms = manifest.perms().count();
            draw_consent(base, nperms);
            let m = wm::CommitRequest { id }
                .encode_msg(&mut out)
                .unwrap_or_else(|_| fail(34, "enc commit"));
            let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
            let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
            let _ = nexo_sys::channel_send(PIPE, b"pedido", &[]);

            // espera a DECISÃO: um clique em Permitir (1), Negar (0) ou Permitir por tempo (2)
            let decisao = loop {
                let (n, _) = match nexo_sys::channel_recv(sess, &mut buf, &mut hs) {
                    Ok(v) => v,
                    Err(_) => nexo_sys::exit(0),
                };
                let Ok(ev) = wm::decode_pointer_event(&buf[..n]) else {
                    continue;
                };
                if ev.surface != id {
                    continue;
                }
                if permit_rect().contains(ev.x, ev.y) {
                    break 1u8;
                }
                if deny_rect().contains(ev.x, ev.y) {
                    break 0u8;
                }
                if timed_rect().contains(ev.x, ev.y) {
                    break 2u8;
                }
            };
            if decisao == 0 {
                log!("lanc: '{}' NEGADO pelo usuario — nada e executado", name);
                let _ = nexo_sys::channel_send(PIPE, b"negado", &[]);
                continue;
            }
            // Permitir: concede exatamente o que o manifesto declara
            let mut pb = [0u8; 96];
            let epath = vpath(name, v, manifest.entry, &mut pb);
            let en = fsr
                .read_all(epath, elf_buf)
                .unwrap_or_else(|| fail(35, "entry"));
            let (app_pipe, app_pipe_child) =
                nexo_sys::channel_create().unwrap_or_else(|_| fail(36, "pipe do app"));
            let child = nexo_sys::process_spawn_mem(&elf_buf[..en], 0, &[app_pipe_child])
                .unwrap_or_else(|_| fail(37, "spawn"));
            if manifest.declares("janelas") {
                let (app_sess, theirs) =
                    nexo_sys::channel_create().unwrap_or_else(|_| fail(38, "sessao do app"));
                let m = wm::OpenRequest { chan: theirs }
                    .encode_msg(&mut out)
                    .unwrap_or_else(|_| fail(39, "enc open"));
                if nexo_sys::channel_send(sess, &out[..m], &[theirs]) != Status::Ok {
                    fail(40, "send open");
                }
                loop {
                    let (n, _) = match nexo_sys::channel_recv(sess, &mut buf, &mut hs) {
                        Ok(v) => v,
                        Err(_) => nexo_sys::exit(0),
                    };
                    if wm::decode_pointer_event(&buf[..n]).is_ok()
                        || wm::decode_key_event(&buf[..n]).is_ok()
                    {
                        continue;
                    }
                    if wm::decode_open_response(&buf[..n]).is_err() {
                        fail(41, "open recusado");
                    }
                    break;
                }
                if nexo_sys::channel_send(app_pipe, b"sess", &[app_sess]) != Status::Ok {
                    fail(42, "send sess ao app");
                }
            }
            if decisao == 2 {
                prazo = Some(nexo_sys::time_now() + PRAZO_NS);
                log!(
                    "lanc: '{}' PERMITIDO POR TEMPO — lancado; expira em {} ms",
                    name,
                    PRAZO_NS / 1_000_000
                );
            } else {
                log!("lanc: '{}' PERMITIDO — lancado com o que declara", name);
            }
            app = Some((app_pipe, child));
            let _ = nexo_sys::channel_send(PIPE, b"permitido", &[]);
        } else if &buf[..n] == b"fecha" {
            let Some((app_pipe, child)) = app.take() else {
                fail(43, "nada a fechar");
            };
            prazo = None;
            let _ = nexo_sys::handle_close(app_pipe); // cordão de vida do app
            if nexo_sys::process_wait(child) != Ok(0) {
                fail(44, "app nao saiu limpo");
            }
            let _ = nexo_sys::channel_send(PIPE, b"fim", &[]);
        }
    }
}

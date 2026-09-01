//! `portal` — portal de arquivos (Plano §Fase 6: "criar portal de arquivos, câmera, microfone e
//! notificações"). O app pede um arquivo ("escolhe"); o portal — que é quem tem o `nexo.fs` e a
//! janela — mostra a lista e espera o **usuário** clicar; só então lê o arquivo e devolve ao app
//! **apenas o conteúdo**. O app nunca vê o sistema de arquivos, nem o nome dos outros arquivos:
//! a escolha do usuário é o limite da concessão (o mesmo desenho dos portais de desktop).
//! Handle 0 = canal do orquestrador: "sess" (sessão wm), "serve <dir>" + canal `nexo.fs`,
//! "cliente" + canal do app; responde "pronto". Pipe fechado = sair.
#![no_std]
#![no_main]

use nexo_gfx::text::draw_glyph;
use nexo_gfx::{Color, PixelFormat, Surface};
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;

const PIPE: Handle = 0;
const COLS: usize = 8;
const ROWS: usize = 6;
const W: i32 = (COLS * 8) as i32;
const H: i32 = (ROWS * 8) as i32;
const NAME_MAX: usize = 24;
const FILE_MAX: usize = 3900;

fn fail(code: i64, what: &str) -> ! {
    log!("portal: falha: {}", what);
    nexo_sys::exit(code)
}

#[derive(Clone, Copy)]
struct Entry {
    name: [u8; NAME_MAX],
    len: usize,
    dir: bool,
}

/// Cliente `nexo.fs`: listar e ler um arquivo inteiro (pequeno).
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

    fn list(&mut self, path: &str, out: &mut [Entry; ROWS]) -> Option<usize> {
        use nexo_proto::fs as pfs;
        let mut p = [0u8; 256];
        let n = path.len().min(256);
        p[..n].copy_from_slice(&path.as_bytes()[..n]);
        let m = pfs::ListRequest {
            path: p,
            path_len: n as u32,
        }
        .encode_msg(&mut self.req)
        .ok()?;
        let rn = self.rpc(m)?;
        let r = pfs::decode_list_response(&self.reply[..rn]).ok()?;
        let entries = r.entries();
        let mut count = 0usize;
        let mut pos = 0usize;
        while pos + 6 <= entries.len() && count < ROWS {
            let kind = entries[pos + 4];
            let nl = entries[pos + 5] as usize;
            if pos + 6 + nl > entries.len() {
                break;
            }
            let mut e = Entry {
                name: [0; NAME_MAX],
                len: nl.min(NAME_MAX),
                dir: kind == 2,
            };
            e.name[..e.len].copy_from_slice(&entries[pos + 6..pos + 6 + e.len]);
            out[count] = e;
            count += 1;
            pos += 6 + nl;
        }
        Some(count)
    }

    fn read_file(&mut self, path: &str, out: &mut [u8; FILE_MAX]) -> Option<usize> {
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
        let size = (st.size as usize).min(FILE_MAX);
        let m = pfs::ReadRequest {
            ino: st.ino,
            offset: 0,
            len: size as u32,
        }
        .encode_msg(&mut self.req)
        .ok()?;
        let rn = self.rpc(m)?;
        let r = pfs::decode_read_response(&self.reply[..rn]).ok()?;
        let dl = r.data().len().min(FILE_MAX);
        out[..dl].copy_from_slice(&r.data()[..dl]);
        Some(dl)
    }
}

fn redraw(base: u64, entries: &[Entry; ROWS], count: usize) {
    // SAFETY: base .. base+W*H*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, (W * H * 4) as usize) };
    let mut s = Surface::new(px, W as u32, H as u32, W as u32, PixelFormat::Rgbx8888)
        .unwrap_or_else(|| fail(20, "superficie"));
    s.clear(Color::rgb(0x14, 0x15, 0x18));
    for (r, e) in entries[..count].iter().enumerate() {
        let color = if e.dir {
            Color::rgb(0x40, 0x45, 0x50) // pastas: apagadas (não escolhíveis no MVP)
        } else {
            Color::rgb(255, 255, 255)
        };
        for (c, &ch) in e.name[..e.len.min(COLS)].iter().enumerate() {
            draw_glyph(
                &mut s,
                ch as char,
                (c * 8) as i32,
                (r * 8) as i32,
                1,
                color,
                None,
            );
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(_arg: u64) -> ! {
    let mut buf = [0u8; 384];
    let mut out = [0u8; 4096];
    let mut hs = [0u32; 1];
    let sess: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"sess" => hs[0],
        _ => fail(21, "sessao nao recebida"),
    };
    let (dir_len, vfs): (usize, Handle) = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if n > 6 && &buf[..6] == b"serve " => (n - 6, hs[0]),
        _ => fail(22, "serve nao recebido"),
    };
    let mut dir = [0u8; 96];
    let dl = dir_len.min(90);
    dir[..dl].copy_from_slice(&buf[6..6 + dl]);
    let client: Handle = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if &buf[..n] == b"cliente" => hs[0],
        _ => fail(23, "cliente nao recebido"),
    };

    let mut fs = Fs {
        ch: vfs,
        req: [0; 4096],
        reply: [0; 4096],
    };

    // janela do portal (some/aparece conforme o pedido; MVP: fica montada)
    let req = wm::CreateSurfaceRequest {
        x: 0,
        y: 0,
        w: W,
        h: H,
        z: 200,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(24, "enc create"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(25, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(sess, &mut buf, &mut hs).unwrap_or_else(|_| fail(26, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(27, "dec create"));
    if nh != 1 {
        fail(28, "sem handle");
    }
    let id = cs.id;
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(29, "map"));
    let mut title = wm::SetTitleRequest {
        id,
        title: [0; 32],
        title_len: 6,
    };
    title.title[..6].copy_from_slice(b"portal");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(30, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    let _ = nexo_sys::channel_send(PIPE, b"pronto", &[]);

    let mut entries = [Entry {
        name: [0; NAME_MAX],
        len: 0,
        dir: false,
    }; ROWS];
    let mut count = 0usize;
    let mut pending = false; // um "escolhe" aguardando o clique do usuário

    loop {
        // cordão de vida
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        // pedido do app: mostrar a lista e esperar o usuário
        match nexo_sys::channel_try_recv(client, &mut buf, &mut hs) {
            Ok((n, _)) if &buf[..n] == b"escolhe" && !pending => {
                let d = core::str::from_utf8(&dir[..dl]).unwrap_or_else(|_| fail(31, "dir"));
                count = fs.list(d, &mut entries).unwrap_or_else(|| fail(32, "list"));
                redraw(base, &entries, count);
                let m = wm::CommitRequest { id }
                    .encode_msg(&mut out)
                    .unwrap_or_else(|_| fail(33, "enc commit"));
                let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
                pending = true;
                log!("portal: pedido do app — aguardando a escolha do usuario");
            }
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0), // app saiu
        }
        // clique do usuário decide (só com um pedido pendente)
        match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
            Ok((n, _)) => {
                if let Ok(ev) = wm::decode_pointer_event(&buf[..n])
                    && pending
                    && ev.surface == id
                    && ev.y >= 0
                {
                    let row = (ev.y / 8) as usize;
                    if row < count && !entries[row].dir {
                        let e = entries[row];
                        let d =
                            core::str::from_utf8(&dir[..dl]).unwrap_or_else(|_| fail(34, "dir"));
                        let mut pb = [0u8; 128];
                        let mut pl = 0;
                        pb[..d.len()].copy_from_slice(d.as_bytes());
                        pl += d.len();
                        if !d.ends_with('/') {
                            pb[pl] = b'/';
                            pl += 1;
                        }
                        pb[pl..pl + e.len].copy_from_slice(&e.name[..e.len]);
                        pl += e.len;
                        let path =
                            core::str::from_utf8(&pb[..pl]).unwrap_or_else(|_| fail(35, "caminho"));
                        let mut content = [0u8; FILE_MAX];
                        let cl = fs
                            .read_file(path, &mut content)
                            .unwrap_or_else(|| fail(36, "ler"));
                        // ao app vai SO o conteudo — nada de fs, nada de caminho alheio
                        let _ = nexo_sys::channel_send(client, &content[..cl], &[]);
                        pending = false;
                        log!("portal: usuario escolheu '{}' ({} bytes ao app)", path, cl);
                    }
                }
            }
            Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        let _ = nexo_sys::channel_wait_any(&[PIPE, client, sess]);
    }
}

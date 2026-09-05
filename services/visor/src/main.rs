//! `visor` — visualizador de imagens (Plano §Fase 6: "criar visualizador de imagens e
//! documentos básicos"). Recebe do orquestrador uma sessão do compositor e um canal `nexo.fs`
//! com o caminho a abrir ("abre <caminho>"), lê o arquivo, decodifica com `nexo-img` (PPM P6,
//! validação hostil sem pânico) e apresenta a imagem numa janela do tamanho exato dela.
//! **Documentos básicos**: um arquivo que NÃO é PPM é apresentado como texto simples numa grade
//! de glifos 6×4 (`nexo-textgrid`, a mesma do terminal e do editor) — bytes não imprimíveis
//! são ignorados pela grade, então qualquer arquivo abre sem pânico.
//! Handle 0 = canal do orquestrador (recebe "sess" e "abre"; cordão de vida; emite "pronto").
#![no_std]
#![no_main]

use nexo_gfx::text::draw_glyph;
use nexo_gfx::{Color, PixelFormat, Surface};
use nexo_img::Ppm;
use nexo_proto::wm;
use nexo_rt::log;
use nexo_sys::Handle;
use nexo_sys::abi::Status;
use nexo_textgrid::Grid;

const PIPE: Handle = 0;
/// Tamanho máximo de arquivo de imagem aceito.
const IMG_MAX: usize = 16384;
/// Grade do documento de texto (cabe na saída de 64×48 a partir de (8,8)).
const TXT_COLS: usize = 6;
const TXT_ROWS: usize = 4;

static mut IMG_BUF: [u8; IMG_MAX] = [0; IMG_MAX];

fn fail(code: i64, what: &str) -> ! {
    log!("visor: falha: {}", what);
    nexo_sys::exit(code)
}

/// Cliente mínimo `nexo.fs`: stat (ino + tamanho) e leitura em blocos.
struct Fs {
    ch: Handle,
    req: [u8; 4096],
    reply: [u8; 4096],
}

impl Fs {
    fn stat(&mut self, path: &[u8]) -> Option<(u32, usize)> {
        use nexo_proto::fs as pfs;
        let mut p = [0u8; 256];
        let n = path.len().min(256);
        p[..n].copy_from_slice(&path[..n]);
        let m = pfs::StatRequest {
            path: p,
            path_len: n as u32,
        }
        .encode_msg(&mut self.req)
        .ok()?;
        let rn = self.rpc(m)?;
        let r = pfs::decode_stat_response(&self.reply[..rn]).ok()?;
        Some((r.ino, r.size as usize))
    }

    /// Lê um bloco para `out`; devolve o tamanho lido.
    fn read(&mut self, ino: u32, offset: u64, len: u32, out: &mut [u8]) -> Option<usize> {
        use nexo_proto::fs as pfs;
        let m = pfs::ReadRequest { ino, offset, len }
            .encode_msg(&mut self.req)
            .ok()?;
        let rn = self.rpc(m)?;
        let r = pfs::decode_read_response(&self.reply[..rn]).ok()?;
        let dl = r.data().len();
        if dl > out.len() {
            return None;
        }
        out[..dl].copy_from_slice(r.data());
        Some(dl)
    }

    fn rpc(&mut self, m: usize) -> Option<usize> {
        if nexo_sys::channel_send(self.ch, &self.req[..m], &[]) != Status::Ok {
            return None;
        }
        let mut hs = [0u32; 1];
        match nexo_sys::channel_recv(self.ch, &mut self.reply, &mut hs) {
            Ok((rn, _)) => Some(rn),
            Err(_) => None,
        }
    }
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
    let (path_len, vfs): (usize, Handle) = match nexo_sys::channel_recv(PIPE, &mut buf, &mut hs) {
        Ok((n, 1)) if n > 5 && &buf[..5] == b"abre " => (n - 5, hs[0]),
        _ => fail(22, "abre nao recebido"),
    };
    let mut path = [0u8; 256];
    path[..path_len].copy_from_slice(&buf[5..5 + path_len]);

    // lê o arquivo inteiro
    let mut fs = Fs {
        ch: vfs,
        req: [0; 4096],
        reply: [0; 4096],
    };
    let (ino, size) = fs
        .stat(&path[..path_len])
        .unwrap_or_else(|| fail(23, "stat"));
    if size > IMG_MAX {
        fail(24, "imagem grande demais");
    }
    // SAFETY: unico acesso a IMG_BUF neste processo de uma so thread.
    let img_buf = unsafe { &mut *core::ptr::addr_of_mut!(IMG_BUF) };
    let mut off = 0usize;
    while off < size {
        let want = (size - off).min(3900) as u32;
        let dl = fs
            .read(ino, off as u64, want, &mut img_buf[off..])
            .unwrap_or_else(|| fail(25, "read"));
        if dl == 0 {
            fail(26, "read curto");
        }
        off += dl;
    }
    // PPM valido = imagem; qualquer outra coisa = documento basico (texto)
    let img = Ppm::parse(&img_buf[..size]).ok();
    let (w, h) = match &img {
        Some(i) => (i.w as i32, i.h as i32),
        None => ((TXT_COLS * 8) as i32, (TXT_ROWS * 8) as i32),
    };

    // janela do tamanho exato da imagem (ou da grade de texto)
    let req = wm::CreateSurfaceRequest {
        x: 8,
        y: 8,
        w,
        h,
        z: 10,
        display: 0,
    };
    let m = req
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(28, "enc create"));
    if nexo_sys::channel_send(sess, &out[..m], &[]) != Status::Ok {
        fail(29, "send create");
    }
    let (n, nh) =
        nexo_sys::channel_recv(sess, &mut buf, &mut hs).unwrap_or_else(|_| fail(30, "recv create"));
    let cs =
        wm::decode_create_surface_response(&buf[..n]).unwrap_or_else(|_| fail(31, "dec create"));
    if nh != 1 {
        fail(32, "sem handle");
    }
    let id = cs.id;
    let base = nexo_sys::memory_map(hs[0]).unwrap_or_else(|_| fail(33, "map"));
    let mut title = wm::SetTitleRequest {
        id,
        title: [0; 32],
        title_len: 5,
    };
    title.title[..5].copy_from_slice(b"visor");
    let m = title
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(34, "enc title"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);

    let stride = w as usize * 4;
    // SAFETY: base .. base+w*h*4 foi mapeada por memory_map (USER|RW) neste processo.
    let px = unsafe { core::slice::from_raw_parts_mut(base as *mut u8, stride * h as usize) };
    if let Some(img) = &img {
        // blit RGB -> RGBX
        for y in 0..img.h {
            for x in 0..img.w {
                let (r, g, b) = img.pixel(x, y);
                let o = y as usize * stride + x as usize * 4;
                px[o] = r;
                px[o + 1] = g;
                px[o + 2] = b;
                px[o + 3] = 0;
            }
        }
    } else {
        // documento basico: o texto passa pela grade e cada celula vira um glifo
        let mut grid: Grid<TXT_COLS, TXT_ROWS> = Grid::new();
        grid.feed_all(&img_buf[..size]);
        let mut s = Surface::new(px, w as u32, h as u32, w as u32, PixelFormat::Rgbx8888)
            .unwrap_or_else(|| fail(36, "superficie"));
        s.clear(Color::rgb(0x14, 0x15, 0x18));
        for (r, row) in grid.cells.iter().enumerate() {
            for (c, &ch) in row.iter().enumerate() {
                if ch != b' ' {
                    draw_glyph(
                        &mut s,
                        ch as char,
                        (c * 8) as i32,
                        (r * 8) as i32,
                        1,
                        Color::rgb(255, 255, 255),
                        None,
                    );
                }
            }
        }
    }
    let m = wm::CommitRequest { id }
        .encode_msg(&mut out)
        .unwrap_or_else(|_| fail(35, "enc commit"));
    let _ = nexo_sys::channel_send(sess, &out[..m], &[]);
    let _ = nexo_sys::channel_recv(sess, &mut buf, &mut hs);
    log!(
        "visor: pronto ({}x{} px, {})",
        w,
        h,
        if img.is_some() { "imagem" } else { "texto" }
    );
    let _ = nexo_sys::channel_send(PIPE, b"pronto", &[]);

    loop {
        match nexo_sys::channel_try_recv(PIPE, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0), // cordão de vida
        }
        match nexo_sys::channel_try_recv(sess, &mut buf, &mut hs) {
            Ok(_) | Err(Status::WouldBlock) => {}
            Err(_) => nexo_sys::exit(0),
        }
        let _ = nexo_sys::channel_wait_any(&[PIPE, sess]);
    }
}

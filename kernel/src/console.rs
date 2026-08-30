//! Console de texto sobre o framebuffer linear (fonte própria 8×8, escala 2).
//!
//! Sem rolagem: ao chegar à última linha volta ao topo, limpando a linha —
//! barato em TCG e suficiente para diagnóstico. Uma barra de status no topo
//! mostra o nome do sistema e a última mensagem de estado.

use core::fmt::{self, Write};
use nexo_boot_abi::{FramebufferInfo, PixelFormat};
use nexo_mm::PhysAddr;
use nexo_sync::SpinLock;

const SCALE: u32 = 2;
const CELL_W: u32 = nexo_font::GLYPH_WIDTH as u32 * SCALE;
const CELL_H: u32 = nexo_font::GLYPH_HEIGHT as u32 * SCALE;
const HEADER_H: u32 = CELL_H + 8;
const MARGIN: u32 = 8;

const BG: (u8, u8, u8) = (0x10, 0x14, 0x1c);
const FG: (u8, u8, u8) = (0xd8, 0xdc, 0xe4);
const HEADER_BG: (u8, u8, u8) = (0x1e, 0x3a, 0x5f);
const HEADER_FG: (u8, u8, u8) = (0xff, 0xff, 0xff);

struct Console {
    base: *mut u32,
    width: u32,
    height: u32,
    stride: u32,
    bgr: bool,
    cols: u32,
    rows: u32,
    col: u32,
    row: u32,
}

// SAFETY: o ponteiro do framebuffer é acessado apenas sob o lock.
unsafe impl Send for Console {}

static CONSOLE: SpinLock<Option<Console>> = SpinLock::new(None);

impl Console {
    fn pixel(&self, (r, g, b): (u8, u8, u8)) -> u32 {
        if self.bgr {
            (b as u32) | (g as u32) << 8 | (r as u32) << 16
        } else {
            (r as u32) | (g as u32) << 8 | (b as u32) << 16
        }
    }

    fn fill(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        let x1 = (x + w).min(self.width);
        let y1 = (y + h).min(self.height);
        for yy in y..y1 {
            let row = (yy * self.stride) as usize;
            for xx in x..x1 {
                // SAFETY: (xx, yy) dentro da resolução; stride*height*4 <= tamanho do fb.
                unsafe { self.base.add(row + xx as usize).write_volatile(color) };
            }
        }
    }

    fn glyph(&mut self, c: char, x: u32, y: u32, fg: u32, bg: u32) {
        let g = nexo_font::glyph(c);
        for (gy, bits) in g.iter().enumerate() {
            for gx in 0..8u32 {
                let on = bits & (0x80 >> gx) != 0;
                self.fill(
                    x + gx * SCALE,
                    y + gy as u32 * SCALE,
                    SCALE,
                    SCALE,
                    if on { fg } else { bg },
                );
            }
        }
    }

    fn cell_origin(&self, col: u32, row: u32) -> (u32, u32) {
        (MARGIN + col * CELL_W, HEADER_H + MARGIN + row * CELL_H)
    }

    fn clear_row(&mut self, row: u32) {
        let (x, y) = self.cell_origin(0, row);
        let bg = self.pixel(BG);
        self.fill(x, y, self.cols * CELL_W, CELL_H, bg);
    }

    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= self.rows {
            self.row = 0;
        }
        self.clear_row(self.row);
    }

    fn put(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            '\r' => self.col = 0,
            '\t' => {
                for _ in 0..4 {
                    self.put(' ');
                }
            }
            c => {
                if self.col >= self.cols {
                    self.newline();
                }
                let (x, y) = self.cell_origin(self.col, self.row);
                let (fg, bg) = (self.pixel(FG), self.pixel(BG));
                self.glyph(c, x, y, fg, bg);
                self.col += 1;
            }
        }
    }

    fn header(&mut self, status: &str) {
        let bg = self.pixel(HEADER_BG);
        let fg = self.pixel(HEADER_FG);
        self.fill(0, 0, self.width, HEADER_H, bg);
        let text = alloc_free_format(status);
        let mut x = MARGIN;
        for c in text.chars() {
            if x + CELL_W > self.width {
                break;
            }
            self.glyph(c, x, 4, fg, bg);
            x += CELL_W;
        }
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.put(c);
        }
        Ok(())
    }
}

/// Monta "Nexo OS 0.0.1-boot | status" sem alocar.
fn alloc_free_format(status: &str) -> HeaderText {
    let mut t = HeaderText {
        buf: [0; 96],
        len: 0,
    };
    let _ = write!(t, "{} {} | {}", crate::NAME, crate::VERSION, status);
    t
}

struct HeaderText {
    buf: [u8; 96],
    len: usize,
}

impl HeaderText {
    fn chars(&self) -> impl Iterator<Item = char> + '_ {
        core::str::from_utf8(&self.buf[..self.len])
            .unwrap_or("")
            .chars()
    }
}

impl Write for HeaderText {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// Inicializa o console se houver framebuffer utilizável e liga o espelho de log.
pub fn init() {
    let fb: &FramebufferInfo = &crate::boot::info().framebuffer;
    if !fb.is_present() || fb.bytes_per_pixel != 4 {
        kinfo!("console: framebuffer ausente ou nao suportado");
        return;
    }
    let bgr = match fb.pixel_format() {
        PixelFormat::Bgrx8888 => true,
        PixelFormat::Rgbx8888 => false,
        PixelFormat::Unknown => {
            kinfo!("console: formato de pixel desconhecido");
            return;
        }
    };
    let base = crate::mm::virt::phys_to_virt(PhysAddr::new(fb.base)).as_mut_ptr::<u32>();
    let mut c = Console {
        base,
        width: fb.width,
        height: fb.height,
        stride: fb.stride,
        bgr,
        cols: (fb.width.saturating_sub(2 * MARGIN)) / CELL_W,
        rows: (fb.height.saturating_sub(HEADER_H + 2 * MARGIN)) / CELL_H,
        col: 0,
        row: 0,
    };
    if c.cols == 0 || c.rows == 0 {
        kinfo!("console: resolucao pequena demais");
        return;
    }
    let bg = c.pixel(BG);
    c.fill(0, 0, c.width, c.height, bg);
    c.header("inicializando");
    let (cols, rows) = (c.cols, c.rows);
    *CONSOLE.lock() = Some(c);
    crate::klog::enable_console();
    kinfo!(
        "console: {}x{} px, {} colunas x {} linhas, {}",
        fb.width,
        fb.height,
        cols,
        rows,
        if bgr { "BGRX" } else { "RGBX" }
    );
}

/// Escreve texto formatado (chamado pelo logger, já sem interrupções).
pub fn write_fmt(args: fmt::Arguments) {
    if let Some(c) = CONSOLE.lock().as_mut() {
        let _ = c.write_fmt(args);
    }
}

/// Atualiza a barra de status.
pub fn status(text: &str) {
    nexo_arch_x86_64::cpu::without_interrupts(|| {
        if let Some(c) = CONSOLE.lock().as_mut() {
            c.header(text);
        }
    });
}

/// Libera o lock à força (caminho de panic).
///
/// # Safety
/// Somente quando o detentor não voltará a executar.
pub unsafe fn force_unlock() {
    // SAFETY: contrato da função.
    unsafe { CONSOLE.force_unlock() };
}

//! Motor de composição de janelas (Plano §Fase 5: "implementar compositor em espaço de usuário"
//! e "damage tracking"). Não é o serviço nem o transporte: é a **lógica** — dada uma lista de
//! janelas (retângulo + buffer de pixels + z) e uma região de dano, compõe as janelas visíveis
//! sobre um fundo na superfície de saída, redesenhando só o que sujou. Sem alocação; testável
//! no host. O serviço `wm` e o transporte por `MemoryObject` compartilhado virão depois.

#![no_std]
#![forbid(unsafe_code)]

use nexo_gfx::{Color, PixelFormat, Rect, Surface};

/// Uma janela na cena: um retângulo na tela e o conteúdo (buffer de pixels do cliente).
#[derive(Clone, Copy)]
pub struct Window<'a> {
    /// Posição e tamanho na tela.
    pub rect: Rect,
    /// Ordem de empilhamento (maior = mais à frente).
    pub z: i32,
    /// Pixels do conteúdo (largura = `rect.w`, altura = `rect.h`, stride em pixels).
    pub pixels: &'a [u8],
    /// Pixels por linha do buffer do cliente.
    pub stride: u32,
    /// Formato dos pixels do cliente.
    pub format: PixelFormat,
    /// Opacidade da janela inteira (255 = opaca, 0 = invisível): composta src-over sobre o que
    /// está abaixo. Buffers são `*x8888` (sem alfa por pixel), então a opacidade é por janela.
    pub alpha: u8,
}

impl Window<'_> {
    /// Lê a cor do pixel do conteúdo em `(cx, cy)` relativo ao canto da janela (com a opacidade
    /// da janela no canal alfa).
    fn sample(&self, cx: i32, cy: i32) -> Option<Color> {
        if cx < 0 || cy < 0 || cx >= self.rect.w || cy >= self.rect.h {
            return None;
        }
        let o = ((cy * self.stride as i32 + cx) * 4) as usize;
        if o + 4 > self.pixels.len() {
            return None;
        }
        let p = &self.pixels[o..o + 4];
        Some(match self.format {
            PixelFormat::Rgbx8888 => Color::rgba(p[0], p[1], p[2], self.alpha),
            PixelFormat::Bgrx8888 => Color::rgba(p[2], p[1], p[0], self.alpha),
            PixelFormat::Unknown => Color::TRANSPARENT,
        })
    }
}

/// Compõe as `windows` sobre `background` na região `damage` de `out`, do fundo para a frente
/// (ordem de z crescente entre as janelas na ordem dada — o chamador as fornece já ordenadas).
/// Redesenha apenas os pixels dentro de `damage` (∩ superfície).
pub fn composite(out: &mut Surface<'_>, windows: &[Window<'_>], damage: Rect, background: Color) {
    let area = damage.intersect(&Rect::new(0, 0, out.width(), out.height()));
    if area.is_empty() {
        return;
    }
    let saved = out.clip();
    out.set_clip(area);
    out.fill_rect(area, background);
    // z crescente: quem tem z menor é desenhado primeiro (fica atrás)
    // (estabilidade: mantém a ordem dada para z iguais)
    let mut order = [0usize; MAX_WINDOWS];
    let n = windows.len().min(MAX_WINDOWS);
    for (i, slot) in order.iter_mut().enumerate().take(n) {
        *slot = i;
    }
    // insertion sort por z (n pequeno)
    for i in 1..n {
        let mut j = i;
        while j > 0 && windows[order[j - 1]].z > windows[order[j]].z {
            order.swap(j - 1, j);
            j -= 1;
        }
    }
    for &wi in &order[..n] {
        let w = &windows[wi];
        let vis = w.rect.intersect(&area);
        if vis.is_empty() {
            continue;
        }
        for y in vis.y..vis.y + vis.h {
            for x in vis.x..vis.x + vis.w {
                if let Some(c) = w.sample(x - w.rect.x, y - w.rect.y) {
                    out.blend(x, y, c);
                }
            }
        }
    }
    out.set_clip(saved);
}

/// Máximo de janelas numa cena.
pub const MAX_WINDOWS: usize = 32;
/// Máximo de retângulos de dano acumulados antes de coalescer no envelope.
pub const MAX_DAMAGE: usize = 16;

/// Rastreamento de danos: acumula retângulos sujos; ao encher, coalesce no envelope (bounding box).
#[derive(Clone, Copy)]
pub struct Damage {
    rects: [Rect; MAX_DAMAGE],
    len: usize,
}

impl Default for Damage {
    fn default() -> Self {
        Self::new()
    }
}

impl Damage {
    /// Região vazia.
    pub const fn new() -> Self {
        Damage {
            rects: [Rect::new(0, 0, 0, 0); MAX_DAMAGE],
            len: 0,
        }
    }

    /// `true` se nada está sujo.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Marca `r` como sujo (ignora retângulos vazios; coalesce ao envelope quando cheio).
    pub fn add(&mut self, r: Rect) {
        if r.is_empty() {
            return;
        }
        if self.len < MAX_DAMAGE {
            self.rects[self.len] = r;
            self.len += 1;
        } else {
            // coalesce tudo num só envelope e acrescenta o novo
            let env = self.bounds().union(&r);
            self.rects[0] = env;
            self.len = 1;
        }
    }

    /// Envelope (bounding box) de todos os retângulos sujos.
    pub fn bounds(&self) -> Rect {
        if self.len == 0 {
            return Rect::new(0, 0, 0, 0);
        }
        let mut b = self.rects[0];
        for r in &self.rects[1..self.len] {
            b = b.union(r);
        }
        b
    }

    /// Retângulos sujos.
    pub fn rects(&self) -> &[Rect] {
        &self.rects[..self.len]
    }

    /// Limpa a região.
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn win_buf(w: i32, h: i32, c: Color, fmt: PixelFormat) -> std::vec::Vec<u8> {
        let mut b = std::vec![0u8; (w * h * 4) as usize];
        let mut s = Surface::new(&mut b, w as u32, h as u32, w as u32, fmt).unwrap();
        s.clear(c);
        b
    }

    #[test]
    fn z_order_front_wins_overlap() {
        let mut out_buf = std::vec![0u8; 8 * 8 * 4];
        let mut out = Surface::new(&mut out_buf, 8, 8, 8, PixelFormat::Rgbx8888).unwrap();
        let red = win_buf(4, 4, Color::rgb(255, 0, 0), PixelFormat::Rgbx8888);
        let green = win_buf(4, 4, Color::rgb(0, 255, 0), PixelFormat::Rgbx8888);
        let windows = [
            Window {
                rect: Rect::new(0, 0, 4, 4),
                z: 0,
                pixels: &red,
                stride: 4,
                format: PixelFormat::Rgbx8888,
                alpha: 255,
            },
            Window {
                rect: Rect::new(2, 2, 4, 4),
                z: 1,
                pixels: &green,
                stride: 4,
                format: PixelFormat::Rgbx8888,
                alpha: 255,
            },
        ];
        composite(
            &mut out,
            &windows,
            Rect::new(0, 0, 8, 8),
            Color::rgb(0, 0, 40),
        );
        assert_eq!(out.get(0, 0), Color::rgb(255, 0, 0)); // so vermelho
        assert_eq!(out.get(3, 3), Color::rgb(0, 255, 0)); // sobreposicao: verde (z maior) na frente
        assert_eq!(out.get(5, 5), Color::rgb(0, 255, 0)); // so verde
        assert_eq!(out.get(7, 7), Color::rgb(0, 0, 40)); // fundo
    }

    #[test]
    fn per_window_alpha_blends_over_below() {
        let mut out_buf = std::vec![0u8; 8 * 8 * 4];
        let mut out = Surface::new(&mut out_buf, 8, 8, 8, PixelFormat::Rgbx8888).unwrap();
        let red = win_buf(8, 8, Color::rgb(255, 0, 0), PixelFormat::Rgbx8888);
        let green = win_buf(4, 4, Color::rgb(0, 255, 0), PixelFormat::Rgbx8888);
        let windows = [
            Window {
                rect: Rect::new(0, 0, 8, 8),
                z: 0,
                pixels: &red,
                stride: 8,
                format: PixelFormat::Rgbx8888,
                alpha: 255, // fundo opaco
            },
            Window {
                rect: Rect::new(0, 0, 4, 4),
                z: 1,
                pixels: &green,
                stride: 4,
                format: PixelFormat::Rgbx8888,
                alpha: 128, // ~50%: verde translúcido sobre o vermelho
            },
        ];
        composite(&mut out, &windows, Rect::new(0, 0, 8, 8), Color::BLACK);
        // sobreposição: ~50% verde sobre vermelho -> (127,128,0) aprox.
        let c = out.get(1, 1);
        assert!((c.r as i32 - 127).abs() <= 2, "r={}", c.r);
        assert!((c.g as i32 - 128).abs() <= 2, "g={}", c.g);
        assert_eq!(c.b, 0);
        // fora do verde: vermelho opaco intacto
        assert_eq!(out.get(6, 6), Color::rgb(255, 0, 0));
    }

    #[test]
    fn z_order_independent_of_input_order() {
        let mut out_buf = std::vec![0u8; 4 * 4 * 4];
        let mut out = Surface::new(&mut out_buf, 4, 4, 4, PixelFormat::Rgbx8888).unwrap();
        let a = win_buf(4, 4, Color::rgb(10, 0, 0), PixelFormat::Rgbx8888);
        let b = win_buf(4, 4, Color::rgb(0, 20, 0), PixelFormat::Rgbx8888);
        // fornecidas fora de ordem de z: b (z=5) antes de a (z=1); a frente e b
        let windows = [
            Window {
                rect: Rect::new(0, 0, 4, 4),
                z: 5,
                pixels: &b,
                stride: 4,
                format: PixelFormat::Rgbx8888,
                alpha: 255,
            },
            Window {
                rect: Rect::new(0, 0, 4, 4),
                z: 1,
                pixels: &a,
                stride: 4,
                format: PixelFormat::Rgbx8888,
                alpha: 255,
            },
        ];
        composite(&mut out, &windows, Rect::new(0, 0, 4, 4), Color::BLACK);
        assert_eq!(out.get(1, 1), Color::rgb(0, 20, 0));
    }

    #[test]
    fn damage_limits_repaint() {
        let mut out_buf = std::vec![0u8; 8 * 8 * 4];
        let mut out = Surface::new(&mut out_buf, 8, 8, 8, PixelFormat::Rgbx8888).unwrap();
        out.clear(Color::rgb(1, 2, 3)); // conteudo previo
        let white = win_buf(8, 8, Color::WHITE, PixelFormat::Rgbx8888);
        let windows = [Window {
            rect: Rect::new(0, 0, 8, 8),
            z: 0,
            pixels: &white,
            stride: 8,
            format: PixelFormat::Rgbx8888,
            alpha: 255,
        }];
        composite(&mut out, &windows, Rect::new(2, 2, 3, 3), Color::BLACK);
        // fora do dano: conteudo previo intacto
        assert_eq!(out.get(0, 0), Color::rgb(1, 2, 3));
        assert_eq!(out.get(6, 6), Color::rgb(1, 2, 3));
        // dentro do dano: recomposto (branco da janela)
        assert_eq!(out.get(3, 3), Color::WHITE);
    }

    #[test]
    fn damage_accumulation_and_coalesce() {
        let mut d = Damage::new();
        assert!(d.is_empty());
        d.add(Rect::new(0, 0, 0, 0)); // vazio ignorado
        assert!(d.is_empty());
        d.add(Rect::new(1, 1, 2, 2));
        d.add(Rect::new(5, 5, 1, 1));
        assert_eq!(d.rects().len(), 2);
        assert_eq!(d.bounds(), Rect::new(1, 1, 5, 5));
        // enche e coalesce
        for i in 0..MAX_DAMAGE + 4 {
            d.add(Rect::new(i as i32, 0, 1, 1));
        }
        assert!(d.rects().len() <= MAX_DAMAGE);
        let b = d.bounds();
        assert!(b.contains(0, 0) && b.w >= MAX_DAMAGE as i32);
        d.clear();
        assert!(d.is_empty());
    }
}

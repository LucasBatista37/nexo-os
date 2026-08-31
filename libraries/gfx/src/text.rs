//! Rasterização de texto (Plano §Fase 5) sobre uma [`Surface`], usando a fonte bitmap 8×8 de
//! `nexo-font`. Glifos fora da faixo imprimível caem no glifo de **fallback** da fonte. Suporta
//! escala inteira, cor de frente e fundo opcional; `\n` quebra linha.

use crate::{Color, Rect, Surface};
use nexo_font::{GLYPH_HEIGHT, GLYPH_WIDTH, glyph};

/// Largura de uma célula de glifo na escala dada.
pub const fn cell_width(scale: i32) -> i32 {
    GLYPH_WIDTH as i32 * scale
}
/// Altura de uma célula de glifo na escala dada.
pub const fn cell_height(scale: i32) -> i32 {
    GLYPH_HEIGHT as i32 * scale
}

/// Largura em pixels que `text` ocuparia (linha mais longa × largura da célula).
pub fn text_width(text: &str, scale: i32) -> i32 {
    let mut max = 0i32;
    let mut cur = 0i32;
    for c in text.chars() {
        if c == '\n' {
            max = max.max(cur);
            cur = 0;
        } else {
            cur += 1;
        }
    }
    max.max(cur) * cell_width(scale)
}

/// Desenha um glifo em `(x, y)` com escala `scale`; `bg = None` não pinta o fundo (transparente).
pub fn draw_glyph(
    surf: &mut Surface<'_>,
    c: char,
    x: i32,
    y: i32,
    scale: i32,
    fg: Color,
    bg: Option<Color>,
) {
    let g = glyph(c);
    if let Some(bgc) = bg {
        surf.fill_rect(Rect::new(x, y, cell_width(scale), cell_height(scale)), bgc);
    }
    for (row, bits) in g.iter().enumerate() {
        for col in 0..GLYPH_WIDTH {
            if bits & (0x80 >> col) != 0 {
                let px = x + col as i32 * scale;
                let py = y + row as i32 * scale;
                surf.fill_rect(Rect::new(px, py, scale, scale), fg);
            }
        }
    }
}

/// Desenha `text` a partir de `(x, y)`; `\n` avança uma linha. Devolve a posição final `(x, y)`.
pub fn draw_text(
    surf: &mut Surface<'_>,
    text: &str,
    x: i32,
    y: i32,
    scale: i32,
    fg: Color,
    bg: Option<Color>,
) -> (i32, i32) {
    let (mut cx, mut cy) = (x, y);
    for c in text.chars() {
        if c == '\n' {
            cx = x;
            cy += cell_height(scale);
            continue;
        }
        draw_glyph(surf, c, cx, cy, scale, fg, bg);
        cx += cell_width(scale);
    }
    (cx, cy)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::PixelFormat;

    fn buf(w: u32, h: u32) -> std::vec::Vec<u8> {
        std::vec![0u8; (w * h * 4) as usize]
    }

    #[test]
    fn dimensions() {
        assert_eq!(cell_width(1), 8);
        assert_eq!(cell_height(2), 16);
        assert_eq!(text_width("ab", 1), 16);
        assert_eq!(text_width("a\nbcd", 2), 3 * 16);
    }

    #[test]
    fn draws_something_and_respects_bg() {
        let mut b = buf(16, 8);
        let mut s = Surface::new(&mut b, 16, 8, 16, PixelFormat::Rgbx8888).unwrap();
        s.clear(Color::BLACK);
        // 'H' tem pixels acesos; com fundo azul, os pixels apagados ficam azuis
        draw_glyph(
            &mut s,
            'H',
            0,
            0,
            1,
            Color::WHITE,
            Some(Color::rgb(0, 0, 40)),
        );
        let mut white = 0;
        let mut blue = 0;
        for y in 0..8 {
            for x in 0..8 {
                match s.get(x, y) {
                    Color {
                        r: 255,
                        g: 255,
                        b: 255,
                        ..
                    } => white += 1,
                    Color {
                        r: 0, g: 0, b: 40, ..
                    } => blue += 1,
                    _ => {}
                }
            }
        }
        assert!(white > 0, "glifo nao desenhou");
        assert!(blue > 0, "fundo nao pintado");
        assert_eq!(white + blue, 64, "cores inesperadas");
    }

    #[test]
    fn newline_advances_line() {
        let mut b = buf(16, 24);
        let mut s = Surface::new(&mut b, 16, 24, 16, PixelFormat::Rgbx8888).unwrap();
        s.clear(Color::BLACK);
        let (ex, ey) = draw_text(&mut s, "a\nb", 0, 0, 2, Color::WHITE, None);
        assert_eq!((ex, ey), (cell_width(2), cell_height(2)));
    }

    #[test]
    fn fallback_glyph_for_out_of_range() {
        // caractere fora da faixa imprimivel usa o glifo de fallback (nao entra em panico)
        let mut b = buf(8, 8);
        let mut s = Surface::new(&mut b, 8, 8, 8, PixelFormat::Rgbx8888).unwrap();
        s.clear(Color::BLACK);
        draw_glyph(&mut s, '\u{2764}', 0, 0, 1, Color::WHITE, None);
        let lit = (0..8)
            .flat_map(|y| (0..8).map(move |x| (x, y)))
            .filter(|&(x, y)| s.get(x, y) == Color::WHITE)
            .count();
        assert!(lit > 0, "fallback nao desenhou nada");
    }
}

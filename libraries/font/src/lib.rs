//! Fonte bitmap 8×8 do Nexo OS.
//!
//! Os glifos vivem em `font8x8.txt` como arte ASCII (`.` = apagado, qualquer
//! outro caractere = aceso) e são convertidos em bits por uma `const fn` no
//! momento da compilação. Cobre ASCII 0x20–0x7E; qualquer outro caractere usa
//! o glifo de substituição (0x7F, uma caixa).
#![no_std]
#![deny(unsafe_code)]

/// Largura de um glifo em pixels.
pub const GLYPH_WIDTH: usize = 8;
/// Altura de um glifo em pixels.
pub const GLYPH_HEIGHT: usize = 8;
/// Número de glifos (0x20..=0x7F).
pub const GLYPH_COUNT: usize = 96;

const SOURCE: &str = include_str!("font8x8.txt");

/// Tabela de glifos. `GLYPHS[c - 0x20][row]`, bit 7 = pixel mais à esquerda.
pub static GLYPHS: [[u8; GLYPH_HEIGHT]; GLYPH_COUNT] = parse(SOURCE.as_bytes());

const fn hex(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

const fn parse(src: &[u8]) -> [[u8; GLYPH_HEIGHT]; GLYPH_COUNT] {
    let mut glyphs = [[0u8; GLYPH_HEIGHT]; GLYPH_COUNT];
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'#' && i + 2 < src.len() {
            let code = (hex(src[i + 1]) as usize) * 16 + hex(src[i + 2]) as usize;
            while i < src.len() && src[i] != b'\n' {
                i += 1;
            }
            i += 1;
            let mut row = 0;
            while row < GLYPH_HEIGHT && i < src.len() {
                let mut bits = 0u8;
                let mut col = 0;
                while col < GLYPH_WIDTH && i < src.len() && src[i] != b'\n' {
                    if src[i] != b'.' {
                        bits |= 1 << (7 - col);
                    }
                    col += 1;
                    i += 1;
                }
                while i < src.len() && src[i] != b'\n' {
                    i += 1;
                }
                i += 1;
                if code >= 0x20 && code <= 0x7f {
                    glyphs[code - 0x20][row] = bits;
                }
                row += 1;
            }
        } else {
            while i < src.len() && src[i] != b'\n' {
                i += 1;
            }
            i += 1;
        }
    }
    glyphs
}

/// Glifo de `c` (substituição para caracteres fora de 0x20..=0x7E).
pub fn glyph(c: char) -> &'static [u8; GLYPH_HEIGHT] {
    let code = c as u32;
    if (0x20..0x7f).contains(&code) {
        &GLYPHS[(code - 0x20) as usize]
    } else {
        &GLYPHS[GLYPH_COUNT - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_glyphs() {
        assert_eq!(glyph(' '), &[0; 8]);
        assert_eq!(glyph('A')[0], 0b0011_1100);
        assert_eq!(glyph('A')[3], 0b0111_1110);
        assert_eq!(glyph('_')[7], 0b0111_1110);
        assert_eq!(glyph('|')[0], 0b0001_1000);
        assert_eq!(glyph('\u{e9}'), glyph('\u{7f}'));
        assert_eq!(glyph('\u{7f}')[0], 0b0111_1110);
    }

    #[test]
    fn every_printable_glyph_is_defined() {
        for c in 0x21u8..=0x7f {
            let g = &GLYPHS[(c - 0x20) as usize];
            assert!(g.iter().any(|&b| b != 0), "glifo {:#x} vazio", c);
        }
    }
}

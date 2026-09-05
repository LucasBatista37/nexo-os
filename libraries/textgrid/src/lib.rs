//! `nexo-textgrid` — a grade de texto compartilhada pelo terminal e pelo editor: quebra
//! automática em `C` colunas, `\r`/`\n`/backspace e rolagem (a linha nova nasce limpa). A
//! pintura é uma função pura das células — pixels determinísticos e testáveis. Também mora aqui
//! o mapa mínimo de scancodes evdev (pressão) para ASCII, usado por quem consome eventos `key`.
#![no_std]

/// Grade `C` colunas × `R` linhas.
pub struct Grid<const C: usize, const R: usize> {
    /// Células (espaço = vazio).
    pub cells: [[u8; C]; R],
    /// Coluna do cursor.
    pub cx: usize,
    /// Linha do cursor (na grade).
    pub cy: usize,
    /// Quantas vezes a grade rolou (linhas que saíram por cima): `scrolled + cy` é a linha
    /// ABSOLUTA do cursor no texto alimentado — o que um editor com rolagem precisa saber.
    pub scrolled: usize,
}

impl<const C: usize, const R: usize> Default for Grid<C, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const C: usize, const R: usize> Grid<C, R> {
    /// Grade vazia, cursor em (0, 0).
    pub const fn new() -> Self {
        Grid {
            cells: [[b' '; C]; R],
            cx: 0,
            cy: 0,
            scrolled: 0,
        }
    }

    fn newline(&mut self) {
        self.cy += 1;
        if self.cy == R {
            self.cells.copy_within(1.., 0);
            self.cells[R - 1] = [b' '; C];
            self.cy = R - 1;
            self.scrolled += 1;
        }
    }

    /// Alimenta um byte: imprimível ocupa a célula e avança (com quebra), `\r` volta ao início
    /// da linha, `\n` desce E volta ao início (modo *newline* — `\r\n` continua idempotente),
    /// `0x08` recua o cursor. O resto é ignorado.
    pub fn feed(&mut self, b: u8) {
        match b {
            b'\r' => self.cx = 0,
            b'\n' => {
                self.cx = 0;
                self.newline();
            }
            0x08 => self.cx = self.cx.saturating_sub(1),
            0x20..=0x7e => {
                self.cells[self.cy][self.cx] = b;
                self.cx += 1;
                if self.cx == C {
                    self.cx = 0;
                    self.newline();
                }
            }
            _ => {}
        }
    }

    /// Alimenta todos os bytes.
    pub fn feed_all(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.feed(b);
        }
    }
}

/// Tradução mínima de scancodes evdev (pressão) para ASCII; `None` para o que não mapear.
pub fn evdev_char(code: u16) -> Option<u8> {
    Some(match code {
        16..=25 => b"qwertyuiop"[code as usize - 16],
        30..=38 => b"asdfghjkl"[code as usize - 30],
        44..=50 => b"zxcvbnm"[code as usize - 44],
        2..=10 => b"123456789"[code as usize - 2],
        11 => b'0',
        57 => b' ',
        28 => b'\n',
        14 => 0x08,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_scrolls_and_backspaces() {
        let mut g: Grid<4, 2> = Grid::new();
        g.feed_all(b"abcdef"); // quebra em 4: "abcd" / "ef"
        assert_eq!(&g.cells[0], b"abcd");
        assert_eq!(&g.cells[1], b"ef  ");
        g.feed_all(b"\r\n"); // desce alem da ultima linha: rola, linha nova limpa
        assert_eq!(&g.cells[0], b"ef  ");
        assert_eq!(&g.cells[1], b"    ");
        g.feed_all(b"x\x08y"); // backspace recua; y sobrescreve x
        assert_eq!(&g.cells[1], b"y   ");
        assert_eq!((g.cx, g.cy), (1, 1));
    }

    #[test]
    fn shell_banner_layout_is_stable() {
        // o fluxo do shell de diagnostico, como o user_term espera
        let mut g: Grid<8, 6> = Grid::new();
        g.feed_all(b"\r\nNexo OS - shell de diagnostico (digite 'ajuda')\r\n> ");
        g.feed_all(b"eco ola\r\nola\r\n> ");
        assert!((0..6).rev().any(|r| &g.cells[r] == b"ola     "));
    }

    #[test]
    fn scrolled_counts_lines_that_left_the_top() {
        // 2 linhas: "a", "b", "c" + quebra final = 4 linhas absolutas; a grade rolou 2 vezes
        let mut g: Grid<4, 2> = Grid::new();
        g.feed_all(b"a\nb\nc\n");
        assert_eq!(g.scrolled, 2);
        assert_eq!((g.cx, g.cy), (0, 1));
        assert_eq!(g.scrolled + g.cy, 3); // linha absoluta do cursor
        assert_eq!(&g.cells[0], b"c   ");
        // a quebra automatica tambem conta como rolagem
        let mut g: Grid<2, 2> = Grid::new();
        g.feed_all(b"abcde"); // ab / cd / e -> rolou 1
        assert_eq!(g.scrolled, 1);
        assert_eq!(&g.cells[1], b"e ");
    }

    #[test]
    fn bare_newline_returns_the_column() {
        let mut g: Grid<8, 3> = Grid::new();
        g.feed_all(b"ola\nmundo");
        assert_eq!(&g.cells[0], b"ola     ");
        assert_eq!(&g.cells[1], b"mundo   ");
        assert_eq!((g.cx, g.cy), (5, 1));
    }

    #[test]
    fn evdev_map_covers_test_keys() {
        assert_eq!(evdev_char(30), Some(b'a'));
        assert_eq!(evdev_char(28), Some(b'\n'));
        assert_eq!(evdev_char(14), Some(0x08));
        assert_eq!(evdev_char(57), Some(b' '));
        assert_eq!(evdev_char(60), None); // F2 nao e texto
    }
}

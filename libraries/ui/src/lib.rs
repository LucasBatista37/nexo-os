//! `nexo-ui` — toolkit de UI nativo do Nexo OS (Plano §Fase 5: "criar toolkit UI nativo e tokens
//! de design" e "temas claro/escuro/alto contraste"). Widgets `no_std`/sem alocação que se
//! desenham sobre uma [`Surface`] do `nexo-gfx` a partir de um [`Theme`]. É a **lógica** de UI
//! (layout, estados, desenho), independente de compositor ou entrada: os widgets fazem hit-test
//! por conta própria e o app decide o que fazer. Testável no host.
#![no_std]
#![forbid(unsafe_code)]

mod theme;
pub use theme::Theme;

use nexo_gfx::text::{cell_height, cell_width, draw_text};
use nexo_gfx::{Color, Rect, Surface};

/// Espaçamento interno padrão dos widgets (px, na escala 1).
pub const PADDING: i32 = 4;

/// Um rótulo de texto.
#[derive(Clone, Copy, Debug)]
pub struct Label<'a> {
    /// Texto (uma linha; `\n` não é tratado aqui).
    pub text: &'a str,
    /// Escala inteira da fonte.
    pub scale: i32,
}

impl<'a> Label<'a> {
    /// Cria um rótulo na escala 1.
    pub fn new(text: &'a str) -> Label<'a> {
        Label { text, scale: 1 }
    }

    /// Tamanho `(largura, altura)` em pixels.
    pub fn size(&self) -> (i32, i32) {
        (
            self.text.chars().count() as i32 * cell_width(self.scale),
            cell_height(self.scale),
        )
    }

    /// Desenha o rótulo em `(x, y)` com a cor de texto do tema.
    pub fn draw(&self, surf: &mut Surface<'_>, x: i32, y: i32, theme: &Theme) {
        draw_text(surf, self.text, x, y, self.scale, theme.fg, None);
    }
}

/// Estado visual de um botão (o app o atualiza a partir da entrada).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    /// Em repouso.
    Normal,
    /// Sob o ponteiro.
    Hover,
    /// Sendo pressionado.
    Pressed,
}

/// Um botão retangular com rótulo centralizado.
#[derive(Clone, Copy, Debug)]
pub struct Button<'a> {
    /// Área do botão na superfície.
    pub rect: Rect,
    /// Texto do rótulo.
    pub label: &'a str,
    /// Escala da fonte do rótulo.
    pub scale: i32,
    /// Estado visual.
    pub state: ButtonState,
}

impl<'a> Button<'a> {
    /// Cria um botão em `rect` na escala 1, em repouso.
    pub fn new(rect: Rect, label: &'a str) -> Button<'a> {
        Button {
            rect,
            label,
            scale: 1,
            state: ButtonState::Normal,
        }
    }

    /// `true` se `(x, y)` está dentro do botão (hit-test).
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.rect.contains(x, y)
    }

    /// Recalcula o estado a partir do ponteiro: dentro + pressionado → `Pressed`, dentro →
    /// `Hover`, fora → `Normal`. Devolve o novo estado (e o guarda).
    pub fn update(&mut self, px: i32, py: i32, pressed: bool) -> ButtonState {
        self.state = if self.contains(px, py) {
            if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Hover
            }
        } else {
            ButtonState::Normal
        };
        self.state
    }

    /// Cor de fundo conforme o estado.
    fn bg(&self, theme: &Theme) -> Color {
        match self.state {
            ButtonState::Normal => theme.button_bg,
            ButtonState::Hover => theme.button_hover,
            ButtonState::Pressed => theme.button_pressed,
        }
    }

    /// Desenha o botão (fundo por estado, borda e rótulo centralizado).
    pub fn draw(&self, surf: &mut Surface<'_>, theme: &Theme) {
        surf.fill_rect(self.rect, self.bg(theme));
        surf.stroke_rect(self.rect, theme.border);
        let tw = self.label.chars().count() as i32 * cell_width(self.scale);
        let th = cell_height(self.scale);
        let tx = self.rect.x + (self.rect.w - tw) / 2;
        let ty = self.rect.y + (self.rect.h - th) / 2;
        draw_text(surf, self.label, tx, ty, self.scale, theme.button_fg, None);
    }
}

/// Pilha vertical: distribui filhos de cima para baixo, com espaçamento fixo, numa coluna de
/// largura `width` a partir de `(x, y)`. Cada `place(h)` devolve o retângulo do próximo filho.
#[derive(Clone, Copy, Debug)]
pub struct VStack {
    x: i32,
    y: i32,
    width: i32,
    spacing: i32,
    cursor: i32,
}

impl VStack {
    /// Nova pilha em `(x, y)`, largura `width`, com `spacing` entre os filhos.
    pub fn new(x: i32, y: i32, width: i32, spacing: i32) -> VStack {
        VStack {
            x,
            y,
            width,
            spacing,
            cursor: y,
        }
    }

    /// Reserva o próximo filho com altura `h`; devolve seu retângulo e avança o cursor.
    pub fn place(&mut self, h: i32) -> Rect {
        let r = Rect::new(self.x, self.cursor, self.width, h);
        self.cursor += h + self.spacing;
        r
    }

    /// Altura total consumida até agora (sem o espaçamento final).
    pub fn height(&self) -> i32 {
        (self.cursor - self.y - self.spacing).max(0)
    }
}

/// Navegação por teclado entre widgets focáveis: um índice de foco que o app avança com Tab
/// (`next`) e recua com Shift+Tab (`prev`), sempre ciclando. O widget focado desenha um anel
/// ([`draw_focus_ring`]) para a navegação ser visível.
#[derive(Clone, Copy, Debug)]
pub struct Nav {
    count: usize,
    /// Índice do widget focado (0-based).
    pub index: usize,
}

impl Nav {
    /// Navegação sobre `count` widgets (foco inicial no 0). `count` deve ser ≥ 1.
    pub fn new(count: usize) -> Nav {
        Nav {
            count: count.max(1),
            index: 0,
        }
    }

    /// Avança o foco (Tab), ciclando; devolve o novo índice.
    pub fn focus_next(&mut self) -> usize {
        self.index = (self.index + 1) % self.count;
        self.index
    }

    /// Recua o foco (Shift+Tab), ciclando; devolve o novo índice.
    pub fn focus_prev(&mut self) -> usize {
        self.index = (self.index + self.count - 1) % self.count;
        self.index
    }
}

/// Desenha o anel de foco (cor de acento) em volta de `rect` — 1 px por fora, para não cobrir a
/// borda do widget.
pub fn draw_focus_ring(surf: &mut Surface<'_>, rect: Rect, theme: &Theme) {
    surf.stroke_rect(
        Rect::new(rect.x - 1, rect.y - 1, rect.w + 2, rect.h + 2),
        theme.accent,
    );
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use nexo_gfx::PixelFormat;

    fn surface(buf: &mut [u8], w: u32, h: u32) -> Surface<'_> {
        Surface::new(buf, w, h, w, PixelFormat::Rgbx8888).unwrap()
    }

    #[test]
    fn themes_are_distinct() {
        let l = Theme::light();
        let d = Theme::dark();
        let hc = Theme::high_contrast();
        assert_ne!(l.bg, d.bg);
        assert_ne!(l.fg, d.fg);
        // alto contraste: fundo preto e texto branco puros
        assert_eq!(hc.bg, Color::rgb(0, 0, 0));
        assert_eq!(hc.fg, Color::rgb(255, 255, 255));
        // acento do alto contraste é amarelo saturado
        assert_eq!(hc.accent, Color::rgb(255, 255, 0));
    }

    #[test]
    fn label_size_matches_text() {
        let l = Label::new("ok");
        assert_eq!(l.size(), (2 * cell_width(1), cell_height(1)));
        let big = Label {
            text: "abc",
            scale: 2,
        };
        assert_eq!(big.size(), (3 * cell_width(2), cell_height(2)));
    }

    #[test]
    fn button_hit_test_and_state() {
        let mut b = Button::new(Rect::new(10, 10, 40, 16), "Go");
        assert!(b.contains(12, 12));
        assert!(!b.contains(0, 0));
        assert_eq!(b.update(12, 12, false), ButtonState::Hover);
        assert_eq!(b.update(12, 12, true), ButtonState::Pressed);
        assert_eq!(b.update(0, 0, true), ButtonState::Normal);
    }

    #[test]
    fn button_draws_bg_border_and_label() {
        let theme = Theme::light();
        let mut buf = std::vec![0u8; 40 * 20 * 4];
        let mut s = surface(&mut buf, 40, 20);
        s.clear(theme.bg);
        let b = Button::new(Rect::new(2, 2, 36, 16), "A");
        b.draw(&mut s, &theme);
        // fundo do botão preenchido no interior
        assert_eq!(s.get(6, 8), theme.button_bg);
        // borda pintada na aresta superior
        assert_eq!(s.get(2, 2), theme.border);
        // rótulo centralizado acende algum pixel na cor do texto do botão
        let lit = (b.rect.x..b.rect.x + b.rect.w)
            .flat_map(|x| (b.rect.y..b.rect.y + b.rect.h).map(move |y| (x, y)))
            .any(|(x, y)| s.get(x, y) == theme.button_fg);
        assert!(lit, "rotulo nao desenhou");
    }

    #[test]
    fn nav_cycles_focus_both_ways() {
        let mut n = Nav::new(3);
        assert_eq!(n.index, 0);
        assert_eq!(n.focus_next(), 1);
        assert_eq!(n.focus_next(), 2);
        assert_eq!(n.focus_next(), 0); // cicla
        assert_eq!(n.focus_prev(), 2); // cicla para tras
        assert_eq!(n.focus_prev(), 1);
    }

    #[test]
    fn focus_ring_draws_around_widget() {
        let theme = Theme::light();
        let mut buf = std::vec![0u8; 20 * 20 * 4];
        let mut s = surface(&mut buf, 20, 20);
        s.clear(theme.bg);
        let r = Rect::new(4, 4, 8, 8);
        draw_focus_ring(&mut s, r, &theme);
        // anel na moldura externa (3,3)..(12,12); interior intacto
        assert_eq!(s.get(3, 3), theme.accent);
        assert_eq!(s.get(12, 3), theme.accent);
        assert_eq!(s.get(3, 12), theme.accent);
        assert_eq!(s.get(6, 6), theme.bg);
    }

    #[test]
    fn vstack_places_children_with_spacing() {
        let mut st = VStack::new(5, 10, 100, 4);
        let a = st.place(16);
        let b = st.place(16);
        assert_eq!(a, Rect::new(5, 10, 100, 16));
        assert_eq!(b, Rect::new(5, 30, 100, 16)); // 10 + 16 + 4
        assert_eq!(st.height(), 36); // 16 + 4 + 16
    }
}

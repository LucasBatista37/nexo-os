//! Tokens de design: um [`Theme`] agrupa as cores da interface (fundo, texto, acento, bordas e
//! estados de botão). Há variantes claro, escuro e alto contraste — a base do gerenciamento de
//! temas do Nexo OS. Os widgets leem só o tema, então trocar de tema repinta a UI inteira.

use nexo_gfx::Color;

/// Paleta da interface. Todos os widgets pintam a partir destes tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// Fundo da janela/tela.
    pub bg: Color,
    /// Fundo de uma superfície de conteúdo (cartão, painel).
    pub surface: Color,
    /// Texto principal.
    pub fg: Color,
    /// Texto/detalhe secundário (menos ênfase).
    pub muted: Color,
    /// Cor de destaque (foco, seleção).
    pub accent: Color,
    /// Bordas e divisórias.
    pub border: Color,
    /// Fundo do botão em repouso.
    pub button_bg: Color,
    /// Fundo do botão sob o ponteiro.
    pub button_hover: Color,
    /// Fundo do botão pressionado.
    pub button_pressed: Color,
    /// Texto do botão.
    pub button_fg: Color,
}

impl Theme {
    /// Tema claro (padrão).
    pub const fn light() -> Theme {
        Theme {
            bg: Color::rgb(0xf4, 0xf4, 0xf6),
            surface: Color::rgb(0xff, 0xff, 0xff),
            fg: Color::rgb(0x1a, 0x1a, 0x1f),
            muted: Color::rgb(0x6b, 0x6b, 0x76),
            accent: Color::rgb(0x2f, 0x6f, 0xed),
            border: Color::rgb(0xd0, 0xd0, 0xd6),
            button_bg: Color::rgb(0x2f, 0x6f, 0xed),
            button_hover: Color::rgb(0x4a, 0x84, 0xf0),
            button_pressed: Color::rgb(0x24, 0x58, 0xc0),
            button_fg: Color::rgb(0xff, 0xff, 0xff),
        }
    }

    /// Tema escuro.
    pub const fn dark() -> Theme {
        Theme {
            bg: Color::rgb(0x14, 0x15, 0x18),
            surface: Color::rgb(0x1e, 0x1f, 0x24),
            fg: Color::rgb(0xec, 0xec, 0xf0),
            muted: Color::rgb(0x9a, 0x9a, 0xa4),
            accent: Color::rgb(0x6f, 0x9f, 0xff),
            border: Color::rgb(0x33, 0x34, 0x3a),
            button_bg: Color::rgb(0x2f, 0x6f, 0xed),
            button_hover: Color::rgb(0x4a, 0x84, 0xf0),
            button_pressed: Color::rgb(0x1f, 0x48, 0xa8),
            button_fg: Color::rgb(0xff, 0xff, 0xff),
        }
    }

    /// Tema de alto contraste (acessibilidade): preto/branco puros e acento saturado.
    pub const fn high_contrast() -> Theme {
        Theme {
            bg: Color::rgb(0, 0, 0),
            surface: Color::rgb(0, 0, 0),
            fg: Color::rgb(0xff, 0xff, 0xff),
            muted: Color::rgb(0xff, 0xff, 0xff),
            accent: Color::rgb(0xff, 0xff, 0x00),
            border: Color::rgb(0xff, 0xff, 0xff),
            button_bg: Color::rgb(0, 0, 0),
            button_hover: Color::rgb(0x33, 0x33, 0x00),
            button_pressed: Color::rgb(0xff, 0xff, 0x00),
            button_fg: Color::rgb(0xff, 0xff, 0xff),
        }
    }
}

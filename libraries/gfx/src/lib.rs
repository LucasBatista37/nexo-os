//! Renderizador 2D por software (Plano §Fase 5): opera sobre uma [`Surface`] — um buffer de
//! pixels do chamador com largura/altura/stride/formato — oferecendo preenchimento de
//! retângulos, `blit` (cópia com/sem recorte), **composição alfa** src-over e um retângulo de
//! **clipping**. Sem alocação; `forbid(unsafe_code)`; totalmente testável no host.

#![no_std]
#![forbid(unsafe_code)]

pub use nexo_boot_abi::PixelFormat;

/// Cor RGBA de 8 bits por canal (alfa direto, não pré-multiplicado).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    /// Vermelho.
    pub r: u8,
    /// Verde.
    pub g: u8,
    /// Azul.
    pub b: u8,
    /// Alfa (255 = opaco).
    pub a: u8,
}

impl Color {
    /// Cor opaca.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }
    /// Cor com alfa.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }
    /// Preto opaco.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Branco opaco.
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    /// Transparente.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
}

/// Retângulo em pixels (origem no canto superior esquerdo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// Coluna do canto esquerdo.
    pub x: i32,
    /// Linha do topo.
    pub y: i32,
    /// Largura.
    pub w: i32,
    /// Altura.
    pub h: i32,
}

impl Rect {
    /// Novo retângulo.
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }
    /// Interseção de dois retângulos (largura/altura ≥ 0).
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }
    /// `true` se vazio.
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    /// `true` se contém `(px, py)`.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Superfície de desenho: um buffer de pixels de 32 bits com stride e formato.
pub struct Surface<'a> {
    pixels: &'a mut [u8],
    width: i32,
    height: i32,
    stride: i32,
    format: PixelFormat,
    clip: Rect,
}

impl<'a> Surface<'a> {
    /// Cria uma superfície sobre `pixels` (deve ter ao menos `stride*height*4` bytes).
    /// Devolve `None` se o formato não é suportado ou o buffer é pequeno demais.
    pub fn new(
        pixels: &'a mut [u8],
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
    ) -> Option<Self> {
        if format == PixelFormat::Unknown || width == 0 || height == 0 || stride < width {
            return None;
        }
        if (stride as usize) * (height as usize) * 4 > pixels.len() {
            return None;
        }
        let (w, h) = (width as i32, height as i32);
        Some(Surface {
            pixels,
            width: w,
            height: h,
            stride: stride as i32,
            format,
            clip: Rect::new(0, 0, w, h),
        })
    }

    /// Largura em pixels.
    pub fn width(&self) -> i32 {
        self.width
    }
    /// Altura em pixels.
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Define o retângulo de recorte (interseccionado com os limites da superfície).
    pub fn set_clip(&mut self, clip: Rect) {
        self.clip = clip.intersect(&Rect::new(0, 0, self.width, self.height));
    }
    /// Remove o recorte (volta à superfície inteira).
    pub fn reset_clip(&mut self) {
        self.clip = Rect::new(0, 0, self.width, self.height);
    }
    /// Recorte atual.
    pub fn clip(&self) -> Rect {
        self.clip
    }

    fn encode(&self, c: Color) -> [u8; 4] {
        match self.format {
            PixelFormat::Rgbx8888 => [c.r, c.g, c.b, 0],
            PixelFormat::Bgrx8888 => [c.b, c.g, c.r, 0],
            PixelFormat::Unknown => [0; 4],
        }
    }

    fn decode(&self, px: [u8; 4]) -> Color {
        match self.format {
            PixelFormat::Rgbx8888 => Color::rgb(px[0], px[1], px[2]),
            PixelFormat::Bgrx8888 => Color::rgb(px[2], px[1], px[0]),
            PixelFormat::Unknown => Color::BLACK,
        }
    }

    fn offset(&self, x: i32, y: i32) -> usize {
        ((y * self.stride + x) * 4) as usize
    }

    /// Lê um pixel (fora dos limites → preto).
    pub fn get(&self, x: i32, y: i32) -> Color {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return Color::BLACK;
        }
        let o = self.offset(x, y);
        self.decode([
            self.pixels[o],
            self.pixels[o + 1],
            self.pixels[o + 2],
            self.pixels[o + 3],
        ])
    }

    /// Escreve um pixel opaco (respeita o recorte).
    pub fn put(&mut self, x: i32, y: i32, c: Color) {
        if !self.clip.contains(x, y) {
            return;
        }
        let px = self.encode(c);
        let o = self.offset(x, y);
        self.pixels[o..o + 4].copy_from_slice(&px);
    }

    /// Compõe um pixel com alfa direto (src-over) sobre o fundo (respeita o recorte).
    pub fn blend(&mut self, x: i32, y: i32, c: Color) {
        if !self.clip.contains(x, y) || c.a == 0 {
            return;
        }
        if c.a == 255 {
            return self.put(x, y, c);
        }
        let dst = self.get(x, y);
        let a = c.a as u32;
        let ia = 255 - a;
        let mix = |s: u8, d: u8| (((s as u32 * a) + (d as u32 * ia) + 127) / 255) as u8;
        let out = Color::rgb(mix(c.r, dst.r), mix(c.g, dst.g), mix(c.b, dst.b));
        self.put(x, y, out);
    }

    /// Preenche a superfície inteira (ignora o recorte).
    pub fn clear(&mut self, c: Color) {
        let saved = self.clip;
        self.clip = Rect::new(0, 0, self.width, self.height);
        self.fill_rect(Rect::new(0, 0, self.width, self.height), c);
        self.clip = saved;
    }

    /// Preenche `rect` com `c` (opaco = escrita direta; com alfa = composição). Recorta.
    pub fn fill_rect(&mut self, rect: Rect, c: Color) {
        let r = rect.intersect(&self.clip);
        if r.is_empty() || c.a == 0 {
            return;
        }
        if c.a == 255 {
            let px = self.encode(c);
            for y in r.y..r.y + r.h {
                let mut o = self.offset(r.x, y);
                for _ in 0..r.w {
                    self.pixels[o..o + 4].copy_from_slice(&px);
                    o += 4;
                }
            }
        } else {
            for y in r.y..r.y + r.h {
                for x in r.x..r.x + r.w {
                    self.blend(x, y, c);
                }
            }
        }
    }

    /// Contorno de 1 px de `rect`.
    pub fn stroke_rect(&mut self, rect: Rect, c: Color) {
        self.fill_rect(Rect::new(rect.x, rect.y, rect.w, 1), c);
        self.fill_rect(Rect::new(rect.x, rect.y + rect.h - 1, rect.w, 1), c);
        self.fill_rect(Rect::new(rect.x, rect.y, 1, rect.h), c);
        self.fill_rect(Rect::new(rect.x + rect.w - 1, rect.y, 1, rect.h), c);
    }

    /// Copia a região `src_rect` de `src` para `(dx, dy)` nesta superfície, compondo com alfa.
    /// Recorta pelo clip do destino. Origem e destino são superfícies distintas.
    pub fn blit(&mut self, src: &Surface<'_>, src_rect: Rect, dx: i32, dy: i32) {
        let sr = src_rect.intersect(&Rect::new(0, 0, src.width, src.height));
        for row in 0..sr.h {
            for col in 0..sr.w {
                let sc = src.get(sr.x + col, sr.y + row);
                self.blend(dx + col, dy + row, sc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn surface(w: u32, h: u32) -> std::vec::Vec<u8> {
        std::vec![0u8; (w * h * 4) as usize]
    }

    #[test]
    fn fill_and_read_back() {
        let mut buf = surface(4, 4);
        let mut s = Surface::new(&mut buf, 4, 4, 4, PixelFormat::Rgbx8888).unwrap();
        s.clear(Color::rgb(10, 20, 30));
        assert_eq!(s.get(0, 0), Color::rgb(10, 20, 30));
        assert_eq!(s.get(3, 3), Color::rgb(10, 20, 30));
        s.fill_rect(Rect::new(1, 1, 2, 2), Color::rgb(200, 100, 50));
        assert_eq!(s.get(0, 0), Color::rgb(10, 20, 30));
        assert_eq!(s.get(1, 1), Color::rgb(200, 100, 50));
        assert_eq!(s.get(2, 2), Color::rgb(200, 100, 50));
        assert_eq!(s.get(3, 3), Color::rgb(10, 20, 30));
    }

    #[test]
    fn pixel_format_byte_order() {
        let mut rgb = surface(1, 1);
        Surface::new(&mut rgb, 1, 1, 1, PixelFormat::Rgbx8888)
            .unwrap()
            .put(0, 0, Color::rgb(1, 2, 3));
        assert_eq!(&rgb[..3], &[1, 2, 3]);
        let mut bgr = surface(1, 1);
        Surface::new(&mut bgr, 1, 1, 1, PixelFormat::Bgrx8888)
            .unwrap()
            .put(0, 0, Color::rgb(1, 2, 3));
        assert_eq!(&bgr[..3], &[3, 2, 1]);
    }

    #[test]
    fn alpha_compositing_srcover() {
        let mut buf = surface(1, 1);
        let mut s = Surface::new(&mut buf, 1, 1, 1, PixelFormat::Rgbx8888).unwrap();
        s.clear(Color::rgb(0, 0, 0));
        // 50% branco sobre preto ~ 128
        s.blend(0, 0, Color::rgba(255, 255, 255, 128));
        let c = s.get(0, 0);
        assert!((c.r as i32 - 128).abs() <= 1, "r={}", c.r);
        // opaco sobrescreve
        s.blend(0, 0, Color::rgb(10, 20, 30));
        assert_eq!(s.get(0, 0), Color::rgb(10, 20, 30));
        // alfa 0 nao muda nada
        s.blend(0, 0, Color::rgba(9, 9, 9, 0));
        assert_eq!(s.get(0, 0), Color::rgb(10, 20, 30));
    }

    #[test]
    fn clipping_constrains_draw() {
        let mut buf = surface(8, 8);
        let mut s = Surface::new(&mut buf, 8, 8, 8, PixelFormat::Rgbx8888).unwrap();
        s.clear(Color::BLACK);
        s.set_clip(Rect::new(2, 2, 3, 3));
        s.fill_rect(Rect::new(0, 0, 8, 8), Color::WHITE);
        // fora do clip permanece preto
        assert_eq!(s.get(1, 1), Color::BLACK);
        assert_eq!(s.get(5, 5), Color::BLACK);
        // dentro do clip fica branco
        assert_eq!(s.get(2, 2), Color::WHITE);
        assert_eq!(s.get(4, 4), Color::WHITE);
        // put fora do clip e ignorado
        s.put(0, 0, Color::WHITE);
        assert_eq!(s.get(0, 0), Color::BLACK);
    }

    #[test]
    fn stride_larger_than_width() {
        // stride 6 numa superficie 4x2: colunas 4,5 sao padding e nao devem ser tocadas
        let mut buf = surface(6, 2);
        Surface::new(&mut buf, 4, 2, 6, PixelFormat::Rgbx8888)
            .unwrap()
            .clear(Color::rgb(9, 9, 9));
        // pixel (4,0) = offset (0*6+4)*4 = 16
        assert_eq!(&buf[16..19], &[0, 0, 0], "padding foi escrito");
    }

    #[test]
    fn blit_composites_between_surfaces() {
        let mut src = surface(2, 2);
        Surface::new(&mut src, 2, 2, 2, PixelFormat::Rgbx8888)
            .unwrap()
            .clear(Color::rgb(255, 0, 0));
        let src_surf = Surface::new(&mut src, 2, 2, 2, PixelFormat::Rgbx8888).unwrap();
        let mut dst = surface(4, 4);
        let mut ds = Surface::new(&mut dst, 4, 4, 4, PixelFormat::Bgrx8888).unwrap();
        ds.clear(Color::rgb(0, 0, 255));
        ds.blit(&src_surf, Rect::new(0, 0, 2, 2), 1, 1);
        // convertido corretamente entre formatos: vermelho no destino
        assert_eq!(ds.get(1, 1), Color::rgb(255, 0, 0));
        assert_eq!(ds.get(0, 0), Color::rgb(0, 0, 255));
        assert_eq!(ds.get(3, 3), Color::rgb(0, 0, 255));
    }

    #[test]
    fn rejects_bad_surfaces() {
        let mut buf = surface(4, 4);
        assert!(Surface::new(&mut buf, 4, 4, 4, PixelFormat::Unknown).is_none());
        let mut small = std::vec![0u8; 8];
        assert!(Surface::new(&mut small, 4, 4, 4, PixelFormat::Rgbx8888).is_none());
    }
}

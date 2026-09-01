//! `nexo-img` — decodificação de imagens (Plano §Fase 6: "criar visualizador de imagens e
//! documentos básicos"). Primeiro formato: **PPM P6** (binário), o formato de intercâmbio mais
//! simples que existe — cabeçalho em texto (`P6`, largura, altura, valor máximo) seguido de
//! trios RGB. `no_std` e sem alocação: o parser valida tudo e devolve uma *vista* sobre os
//! bytes; quem pinta lê pixel a pixel. Entrada é dado hostil (vem de arquivo): nenhum caminho
//! pode entrar em pânico — truncado, gigante ou malformado devolve erro.
#![no_std]

/// Limite de dimensão (largura ou altura): imagens maiores são rejeitadas.
pub const MAX_DIM: u32 = 4096;

/// Erros de decodificação.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImgError {
    /// Não começa com `P6`.
    Magic,
    /// Cabeçalho malformado (dimensões ou valor máximo ilegíveis).
    Header,
    /// Valor máximo não suportado (só 255).
    Maxval,
    /// Dimensão zero ou acima de [`MAX_DIM`].
    Dims,
    /// Faltam bytes de pixels.
    Truncated,
}

/// Imagem PPM P6 decodificada: uma vista validada sobre os bytes originais.
#[derive(Debug)]
pub struct Ppm<'a> {
    /// Largura em pixels.
    pub w: u32,
    /// Altura em pixels.
    pub h: u32,
    data: &'a [u8],
}

/// Consome espaços em branco e comentários (`#` até o fim da linha) a partir de `i`.
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else {
            return i;
        }
    }
}

/// Lê um inteiro decimal a partir de `i`; devolve (valor, próxima posição).
fn read_u32(bytes: &[u8], mut i: usize) -> Result<(u32, usize), ImgError> {
    let start = i;
    let mut v: u32 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        v = v
            .checked_mul(10)
            .and_then(|v| v.checked_add((bytes[i] - b'0') as u32))
            .ok_or(ImgError::Header)?;
        i += 1;
    }
    if i == start {
        return Err(ImgError::Header);
    }
    Ok((v, i))
}

impl<'a> Ppm<'a> {
    /// Valida `bytes` como PPM P6; nunca entra em pânico.
    pub fn parse(bytes: &'a [u8]) -> Result<Ppm<'a>, ImgError> {
        if bytes.len() < 2 || &bytes[..2] != b"P6" {
            return Err(ImgError::Magic);
        }
        let i = skip_ws(bytes, 2);
        let (w, i) = read_u32(bytes, i)?;
        let i = skip_ws(bytes, i);
        let (h, i) = read_u32(bytes, i)?;
        let i = skip_ws(bytes, i);
        let (maxval, i) = read_u32(bytes, i)?;
        if maxval != 255 {
            return Err(ImgError::Maxval);
        }
        if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
            return Err(ImgError::Dims);
        }
        // exatamente UM espaço em branco separa o cabeçalho dos pixels
        if i >= bytes.len() || !bytes[i].is_ascii_whitespace() {
            return Err(ImgError::Header);
        }
        let px = i + 1;
        let need = (w as usize) * (h as usize) * 3;
        if bytes.len() < px + need {
            return Err(ImgError::Truncated);
        }
        Ok(Ppm {
            w,
            h,
            data: &bytes[px..px + need],
        })
    }

    /// Pixel `(x, y)` como `(r, g, b)`; fora da imagem devolve preto.
    pub fn pixel(&self, x: u32, y: u32) -> (u8, u8, u8) {
        if x >= self.w || y >= self.h {
            return (0, 0, 0);
        }
        let o = ((y as usize * self.w as usize) + x as usize) * 3;
        (self.data[o], self.data[o + 1], self.data[o + 2])
    }

    /// Trios RGB crus, linha a linha.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::vec::Vec;

    fn sample(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(std::format!("P6\n# comentario\n{w} {h}\n255\n").as_bytes());
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[x as u8, y as u8, 7]);
            }
        }
        v
    }

    #[test]
    fn parses_and_reads_pixels() {
        let bytes = sample(16, 12);
        let p = Ppm::parse(&bytes).unwrap();
        assert_eq!((p.w, p.h), (16, 12));
        assert_eq!(p.pixel(0, 0), (0, 0, 7));
        assert_eq!(p.pixel(15, 11), (15, 11, 7));
        assert_eq!(p.pixel(16, 0), (0, 0, 0)); // fora: preto
        assert_eq!(p.data().len(), 16 * 12 * 3);
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(
            Ppm::parse(b"P5\n1 1\n255\nxxx").unwrap_err(),
            ImgError::Magic
        );
        assert_eq!(
            Ppm::parse(b"P6\n1 1\n127\nxxx").unwrap_err(),
            ImgError::Maxval
        );
        assert_eq!(Ppm::parse(b"P6\n0 1\n255\n").unwrap_err(), ImgError::Dims);
        assert_eq!(
            Ppm::parse(b"P6\n9999 9999\n255\n").unwrap_err(),
            ImgError::Dims
        );
        assert_eq!(
            Ppm::parse(b"P6\n2 2\n255\nabc").unwrap_err(),
            ImgError::Truncated
        );
    }

    #[test]
    fn fuzz_lite_truncations_never_panic() {
        let bytes = sample(8, 8);
        for n in 0..bytes.len() {
            let _ = Ppm::parse(&bytes[..n]); // qualquer prefixo: erro ou ok, nunca panico
        }
        let mut mutated = bytes.clone();
        for i in 0..mutated.len().min(64) {
            let orig = mutated[i];
            mutated[i] = 0xff;
            let _ = Ppm::parse(&mutated);
            mutated[i] = orig;
        }
    }
}

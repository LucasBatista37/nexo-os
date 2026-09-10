//! `nexo-prefs` — o formato das preferências do sistema.
//!
//! Quatro preferências viviam só na memória do compositor e morriam com ele: movimento
//! reduzido, escala, tema e idioma. Este crate define como elas viram bytes e voltam.
//!
//! O formato é `chave=valor`, uma por linha, e as suas regras são todas sobre **sobreviver a
//! um arquivo estranho**, porque um arquivo de preferências é exatamente o que fica corrompido
//! num corte de energia ou desatualizado depois de uma atualização:
//!
//! - chave desconhecida é **ignorada** (um sistema novo lê o arquivo de um antigo, e vice-versa);
//! - linha malformada é **ignorada** (não invalida as outras);
//! - chave ausente fica no **padrão** (o arquivo não precisa de estar completo);
//! - valor absurdo cai no padrão daquela chave (escala zero não existe).
//!
//! Nada aqui aloca e nada entra em pânico: ler preferências não pode ser uma forma de derrubar
//! o compositor.
#![no_std]

/// As preferências do sistema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// Movimento reduzido (acessibilidade): aplicativos desligam animações.
    pub reduce_motion: bool,
    /// Numerador da escala global.
    pub scale_num: u32,
    /// Denominador da escala global.
    pub scale_den: u32,
    /// Tema: 0 = escuro, 1 = claro.
    pub theme: u8,
    /// Idioma: 0 = pt-BR, 1 = en-US.
    pub idioma: u8,
}

impl Default for Prefs {
    /// O sistema como sai de fábrica: escala 1:1, tema escuro, português, sem movimento
    /// reduzido.
    fn default() -> Prefs {
        Prefs {
            reduce_motion: false,
            scale_num: 1,
            scale_den: 1,
            theme: 0,
            idioma: 0,
        }
    }
}

/// Escreve as preferências em `out` e devolve quantos bytes ocupou (0 se não coube).
///
/// O formato é legível de propósito: quem depurar um sistema que não arranca precisa de poder
/// ler e corrigir este arquivo com o editor mais simples que tiver à mão.
pub fn serializa(p: &Prefs, out: &mut [u8]) -> usize {
    let mut n = 0;
    let mut escreve = |chave: &str, valor: u32, out: &mut [u8]| {
        for b in chave.bytes().chain(core::iter::once(b'=')) {
            if n >= out.len() {
                return;
            }
            out[n] = b;
            n += 1;
        }
        // Número sem alocação: no máximo 10 dígitos.
        let mut d = [0u8; 10];
        let mut i = 0;
        let mut v = valor;
        loop {
            d[i] = b'0' + (v % 10) as u8;
            v /= 10;
            i += 1;
            if v == 0 {
                break;
            }
        }
        while i > 0 {
            i -= 1;
            if n >= out.len() {
                return;
            }
            out[n] = d[i];
            n += 1;
        }
        if n < out.len() {
            out[n] = b'\n';
            n += 1;
        }
    };
    escreve("reduce_motion", u32::from(p.reduce_motion), out);
    escreve("scale_num", p.scale_num, out);
    escreve("scale_den", p.scale_den, out);
    escreve("theme", u32::from(p.theme), out);
    escreve("idioma", u32::from(p.idioma), out);
    n
}

/// Lê preferências de `bytes`, mantendo o padrão para o que faltar ou não fizer sentido.
pub fn parse(bytes: &[u8]) -> Prefs {
    let mut p = Prefs::default();
    for linha in bytes.split(|b| *b == b'\n') {
        let Some(eq) = linha.iter().position(|b| *b == b'=') else {
            continue; // linha sem '=': ignorada
        };
        let (chave, valor) = (&linha[..eq], &linha[eq + 1..]);
        let Some(v) = numero(valor) else {
            continue; // valor não numérico: ignorado
        };
        match chave {
            b"reduce_motion" => p.reduce_motion = v != 0,
            b"scale_num" if v > 0 => p.scale_num = v,
            b"scale_den" if v > 0 => p.scale_den = v,
            b"theme" => p.theme = (v != 0) as u8,
            b"idioma" => p.idioma = (v != 0) as u8,
            _ => {} // chave desconhecida: ignorada, de propósito
        }
    }
    p
}

/// Número decimal sem sinal; `None` se estiver vazio, tiver lixo ou estourar.
fn numero(b: &[u8]) -> Option<u32> {
    let b = match b.split_last() {
        Some((b'\r', resto)) => resto, // arquivo escrito noutro sistema
        _ => b,
    };
    if b.is_empty() {
        return None;
    }
    let mut v: u32 = 0;
    for c in b {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v.checked_mul(10)?.checked_add(u32::from(c - b'0'))?;
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn ida_e_volta() {
        let p = Prefs {
            reduce_motion: true,
            scale_num: 3,
            scale_den: 2,
            theme: 1,
            idioma: 1,
        };
        let mut buf = [0u8; 128];
        let n = serializa(&p, &mut buf);
        assert!(n > 0);
        assert_eq!(parse(&buf[..n]), p);
    }

    #[test]
    fn padrao_para_o_que_falta() {
        let p = parse(b"idioma=1\n");
        assert_eq!(p.idioma, 1);
        assert_eq!(p.scale_num, 1, "escala devia ficar no padrao");
        assert_eq!(p.theme, 0);
    }

    #[test]
    fn arquivo_estranho_nao_derruba_nada() {
        // Lixo, linha sem '=', valor não numérico, chave desconhecida, escala zero: tudo
        // ignorado, e o que é válido continua a valer.
        let p = parse(b"\x00\x01lixo\nsem_igual\ntheme=abc\nfuturo=7\nscale_num=0\nidioma=1\n");
        assert_eq!(p.idioma, 1);
        assert_eq!(p.theme, 0);
        assert_eq!(p.scale_num, 1);
    }

    #[test]
    fn arquivo_vazio_da_o_padrao() {
        assert_eq!(parse(b""), Prefs::default());
        assert_eq!(parse(b"\n\n\n"), Prefs::default());
    }

    #[test]
    fn numero_gigante_e_recusado_sem_estourar() {
        let p = parse(b"scale_num=99999999999999\n");
        assert_eq!(p.scale_num, 1, "overflow devia cair no padrao");
    }

    #[test]
    fn fim_de_linha_de_outro_sistema() {
        let p = parse(b"idioma=1\r\ntheme=1\r\n");
        assert_eq!((p.idioma, p.theme), (1, 1));
    }

    #[test]
    fn buffer_pequeno_nao_estoura() {
        let p = Prefs::default();
        let mut buf = [0u8; 8];
        let n = serializa(&p, &mut buf);
        assert!(n <= 8, "escreveu {n} num buffer de 8");
    }
}

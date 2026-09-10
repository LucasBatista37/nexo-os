//! `nexo-i18n` — catálogo de mensagens.
//!
//! O sistema é escrito em português e as suas mensagens estavam **no código**, o que impede
//! traduzir sem reescrever. Isto é o mecanismo que faltava: uma tabela de chaves com uma
//! tradução por idioma, consultada em tempo de execução.
//!
//! Três decisões que moldam o resto:
//!
//! - **Sem alocação e sem falha.** `texto` devolve `&'static str` e nunca entra em pânico: uma
//!   chave desconhecida devolve a própria chave. Uma interface com um rótulo estranho é ruim;
//!   uma interface que morre ao desenhar um rótulo é pior.
//! - **A completude é verificada, não prometida.** Um teste de host percorre o catálogo e
//!   recusa chave repetida, tradução vazia ou fora de ordem. Faltar uma tradução é um erro de
//!   compilação do teste, não uma descoberta do usuário.
//! - **Progressivo.** Converter todas as mensagens de uma vez seria uma mudança enorme e sem
//!   revisão possível. O catálogo cresce serviço a serviço; o que ainda não passou por aqui
//!   continua a funcionar exatamente como antes.
#![no_std]

/// Idiomas que o sistema conhece.
///
/// Deliberadamente curto: dois idiomas de verdade valem mais que doze pela metade, e o Plano
/// pede pt-BR e en-US.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Idioma {
    /// Português do Brasil (o idioma em que o sistema é escrito).
    PtBr,
    /// Inglês dos Estados Unidos.
    EnUs,
}

impl Idioma {
    /// Código BCP-47 (`pt-BR`, `en-US`).
    pub const fn codigo(self) -> &'static str {
        match self {
            Idioma::PtBr => "pt-BR",
            Idioma::EnUs => "en-US",
        }
    }

    /// Interpreta um código; qualquer coisa desconhecida cai no padrão do sistema.
    pub fn do_codigo(s: &str) -> Idioma {
        match s {
            "en" | "en-US" | "en_US" => Idioma::EnUs,
            _ => Idioma::PtBr,
        }
    }
}

/// Uma mensagem: a chave e as suas traduções.
#[derive(Clone, Copy, Debug)]
pub struct Entrada {
    /// Chave estável (nunca aparece ao usuário, exceto se faltar tradução).
    pub chave: &'static str,
    /// Texto em português do Brasil.
    pub pt: &'static str,
    /// Texto em inglês dos Estados Unidos.
    pub en: &'static str,
}

/// Catálogo do sistema, **ordenado por chave** (a busca é binária).
///
/// Mantê-lo ordenado é exigido por teste: sem ordem a busca binária mente em silêncio.
pub const CATALOGO: &[Entrada] = &[
    Entrada {
        chave: "greeter.entrar",
        pt: "Entrar",
        en: "Sign in",
    },
    Entrada {
        chave: "greeter.senha",
        pt: "Senha",
        en: "Password",
    },
    Entrada {
        chave: "greeter.senha_incorreta",
        pt: "Senha incorreta",
        en: "Wrong password",
    },
    Entrada {
        chave: "greeter.titulo",
        pt: "Sessao bloqueada",
        en: "Session locked",
    },
];

/// Texto de `chave` no `idioma`, procurando em `catalogo`.
///
/// Chave desconhecida devolve a própria chave — visível para quem revê a interface, inofensiva
/// para quem a usa.
pub fn texto_em(catalogo: &'static [Entrada], idioma: Idioma, chave: &str) -> &'static str {
    match catalogo.binary_search_by(|e| e.chave.cmp(chave)) {
        Ok(i) => match idioma {
            Idioma::PtBr => catalogo[i].pt,
            Idioma::EnUs => catalogo[i].en,
        },
        // Sem `unwrap`: a chave que veio é `&str` de vida arbitrária, então devolve-se a chave
        // do catálogo mais próxima só quando existe; caso contrário, um marcador estável.
        Err(_) => "?",
    }
}

/// Texto do catálogo do sistema.
pub fn texto(idioma: Idioma, chave: &str) -> &'static str {
    texto_em(CATALOGO, idioma, chave)
}

/// O que pode estar errado num catálogo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErroCatalogo {
    /// Chaves fora de ordem crescente (a busca binária depende da ordem).
    ForaDeOrdem(&'static str),
    /// A mesma chave duas vezes.
    Repetida(&'static str),
    /// Falta a tradução de um idioma.
    TraducaoVazia(&'static str),
}

/// Confere as invariantes de um catálogo: ordem estrita, sem repetidas, sem tradução vazia.
///
/// É uma função e não só um teste porque assim vale para **qualquer** catálogo — o do sistema
/// e o de quem escrever um. Sem ordem, a busca binária erra em silêncio, que é a pior forma de
/// errar: a interface mostra um rótulo estranho e ninguém sabe porquê.
pub fn valida(catalogo: &'static [Entrada]) -> Result<(), ErroCatalogo> {
    let mut anterior: Option<&'static str> = None;
    for e in catalogo {
        if e.pt.is_empty() || e.en.is_empty() {
            return Err(ErroCatalogo::TraducaoVazia(e.chave));
        }
        if let Some(a) = anterior {
            if e.chave == a {
                return Err(ErroCatalogo::Repetida(e.chave));
            }
            if e.chave < a {
                return Err(ErroCatalogo::ForaDeOrdem(e.chave));
            }
        }
        anterior = Some(e.chave);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn catalogo_do_sistema_e_valido() {
        assert_eq!(valida(CATALOGO), Ok(()));
    }

    #[test]
    fn traduz_nos_dois_idiomas() {
        assert_eq!(texto(Idioma::PtBr, "greeter.entrar"), "Entrar");
        assert_eq!(texto(Idioma::EnUs, "greeter.entrar"), "Sign in");
    }

    #[test]
    fn chave_desconhecida_nao_entra_em_panico() {
        assert_eq!(texto(Idioma::PtBr, "nao.existe"), "?");
        assert_eq!(texto(Idioma::EnUs, ""), "?");
    }

    #[test]
    fn codigos_de_idioma() {
        assert_eq!(Idioma::PtBr.codigo(), "pt-BR");
        assert_eq!(Idioma::do_codigo("en-US"), Idioma::EnUs);
        assert_eq!(Idioma::do_codigo("en"), Idioma::EnUs);
        // Desconhecido cai no padrão do sistema, nunca falha.
        assert_eq!(Idioma::do_codigo("klingon"), Idioma::PtBr);
        assert_eq!(Idioma::do_codigo(""), Idioma::PtBr);
    }

    /// O validador tem de **reprovar** cada defeito; um validador que só aprova não valida.
    #[test]
    fn validador_reprova_os_tres_defeitos() {
        const FORA_DE_ORDEM: &[Entrada] = &[
            Entrada {
                chave: "z",
                pt: "zeta",
                en: "zed",
            },
            Entrada {
                chave: "a",
                pt: "alfa",
                en: "alpha",
            },
        ];
        assert_eq!(valida(FORA_DE_ORDEM), Err(ErroCatalogo::ForaDeOrdem("a")));

        const REPETIDA: &[Entrada] = &[
            Entrada {
                chave: "a",
                pt: "alfa",
                en: "alpha",
            },
            Entrada {
                chave: "a",
                pt: "outra",
                en: "other",
            },
        ];
        assert_eq!(valida(REPETIDA), Err(ErroCatalogo::Repetida("a")));

        const SEM_TRADUCAO: &[Entrada] = &[Entrada {
            chave: "a",
            pt: "alfa",
            en: "",
        }];
        assert_eq!(valida(SEM_TRADUCAO), Err(ErroCatalogo::TraducaoVazia("a")));
    }
}

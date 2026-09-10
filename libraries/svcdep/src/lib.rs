//! `nexo-svcdep` — ordem de partida de serviços a partir de dependências **declaradas**.
//!
//! O `svcmgr` já sabia reiniciar um serviço que cai; o que faltava era saber em que ordem
//! iniciá-los, e a partir de uma declaração — não de uma sequência escrita à mão no código,
//! que ninguém consegue revisar e que silenciosamente sai de ordem quando alguém acrescenta
//! um serviço no meio.
//!
//! A resolução mora aqui, fora do serviço, por um motivo prático: assim ela é uma função pura
//! sobre dados e pode ser testada no host — ciclos, dependência inexistente, nome repetido e
//! ordem estável são todos casos determinísticos, e nenhum deles precisa de um sistema
//! operacional a rodar para ser exercitado.
//!
//! `no_std`, sem alocação (o chamador fornece o vetor de saída) e sem pânico possível.
#![no_std]

/// Quantos serviços a resolução aceita numa tabela.
pub const MAX_SERVICOS: usize = 32;

/// Um serviço declarado: o nome e quem precisa estar **pronto** antes dele começar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec<'a> {
    /// Nome do serviço (é a chave: dois iguais são um erro de declaração).
    pub nome: &'a str,
    /// Nomes dos serviços dos quais este depende.
    pub depende: &'a [&'a str],
}

/// Por que uma tabela de serviços não pôde ser ordenada.
///
/// Todos são erros de **declaração**, não de execução: falham cedo e por inteiro, porque uma
/// tabela mal declarada iniciaria serviços numa ordem errada sem avisar ninguém.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepErro<'a> {
    /// `servico` depende de um nome que não está declarado na tabela.
    Desconhecida {
        /// Quem declarou a dependência.
        servico: &'a str,
        /// O nome que não existe.
        dependencia: &'a str,
    },
    /// Há um ciclo; o serviço nomeado está preso nele (inclui depender de si mesmo).
    Ciclo(&'a str),
    /// Dois serviços com o mesmo nome.
    Duplicado(&'a str),
    /// A tabela passa de [`MAX_SERVICOS`] ou o vetor de saída é menor que ela.
    Espaco,
}

/// Escreve em `saida` os índices de `specs` numa ordem em que toda dependência vem antes de
/// quem depende dela. Devolve quantos índices foram escritos (sempre `specs.len()`).
///
/// A ordem é **estável**: entre serviços igualmente prontos, vence quem foi declarado antes.
/// Isso importa para diagnóstico — a mesma tabela dá sempre o mesmo log de partida.
pub fn ordem<'a>(specs: &[Spec<'a>], saida: &mut [usize]) -> Result<usize, DepErro<'a>> {
    let n = specs.len();
    if n > MAX_SERVICOS || saida.len() < n {
        return Err(DepErro::Espaco);
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if specs[i].nome == specs[j].nome {
                return Err(DepErro::Duplicado(specs[i].nome));
            }
        }
    }
    for s in specs {
        for d in s.depende {
            if !specs.iter().any(|o| o.nome == *d) {
                return Err(DepErro::Desconhecida {
                    servico: s.nome,
                    dependencia: d,
                });
            }
        }
    }
    let mut pronto = [false; MAX_SERVICOS];
    let mut escritos = 0usize;
    while escritos < n {
        let antes = escritos;
        for i in 0..n {
            if pronto[i] {
                continue;
            }
            let liberado = specs[i].depende.iter().all(|d| {
                specs
                    .iter()
                    .position(|o| o.nome == *d)
                    .is_some_and(|j| pronto[j])
            });
            if liberado {
                pronto[i] = true;
                saida[escritos] = i;
                escritos += 1;
            }
        }
        if escritos == antes {
            // Ninguém avançou e ainda falta gente: quem sobrou está num ciclo ou depende de
            // quem está. O primeiro não-pronto identifica o problema para o log.
            let preso = (0..n).find(|&i| !pronto[i]).map_or("?", |i| specs[i].nome);
            return Err(DepErro::Ciclo(preso));
        }
    }
    Ok(escritos)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    fn nomes<'a>(specs: &[Spec<'a>], saida: &[usize], n: usize) -> std::vec::Vec<&'a str> {
        saida[..n].iter().map(|&i| specs[i].nome).collect()
    }

    #[test]
    fn dependencia_vem_antes_mesmo_declarada_depois() {
        let specs = [
            Spec {
                nome: "cliente",
                depende: &["eco"],
            },
            Spec {
                nome: "eco",
                depende: &[],
            },
        ];
        let mut saida = [0usize; 2];
        let n = ordem(&specs, &mut saida).unwrap();
        assert_eq!(nomes(&specs, &saida, n), ["eco", "cliente"]);
    }

    #[test]
    fn diamante_respeita_todas_as_arestas() {
        // d depende de b e c; ambos dependem de a.
        let specs = [
            Spec {
                nome: "d",
                depende: &["b", "c"],
            },
            Spec {
                nome: "b",
                depende: &["a"],
            },
            Spec {
                nome: "c",
                depende: &["a"],
            },
            Spec {
                nome: "a",
                depende: &[],
            },
        ];
        let mut saida = [0usize; 4];
        let n = ordem(&specs, &mut saida).unwrap();
        let ordem_nomes = nomes(&specs, &saida, n);
        assert_eq!(ordem_nomes[0], "a");
        assert_eq!(ordem_nomes[3], "d");
        // estável: entre b e c, vence quem foi declarado antes
        assert_eq!(ordem_nomes, ["a", "b", "c", "d"]);
    }

    #[test]
    fn sem_dependencias_preserva_a_ordem_de_declaracao() {
        let specs = [
            Spec {
                nome: "um",
                depende: &[],
            },
            Spec {
                nome: "dois",
                depende: &[],
            },
            Spec {
                nome: "tres",
                depende: &[],
            },
        ];
        let mut saida = [0usize; 3];
        let n = ordem(&specs, &mut saida).unwrap();
        assert_eq!(nomes(&specs, &saida, n), ["um", "dois", "tres"]);
    }

    #[test]
    fn ciclo_e_recusado_nomeando_um_participante() {
        let specs = [
            Spec {
                nome: "a",
                depende: &["b"],
            },
            Spec {
                nome: "b",
                depende: &["a"],
            },
        ];
        let mut saida = [0usize; 2];
        assert_eq!(ordem(&specs, &mut saida), Err(DepErro::Ciclo("a")));
    }

    #[test]
    fn depender_de_si_mesmo_e_um_ciclo() {
        let specs = [Spec {
            nome: "a",
            depende: &["a"],
        }];
        let mut saida = [0usize; 1];
        assert_eq!(ordem(&specs, &mut saida), Err(DepErro::Ciclo("a")));
    }

    #[test]
    fn dependencia_inexistente_e_recusada_com_os_dois_nomes() {
        let specs = [Spec {
            nome: "a",
            depende: &["fantasma"],
        }];
        let mut saida = [0usize; 1];
        assert_eq!(
            ordem(&specs, &mut saida),
            Err(DepErro::Desconhecida {
                servico: "a",
                dependencia: "fantasma"
            })
        );
    }

    #[test]
    fn nome_repetido_e_recusado() {
        let specs = [
            Spec {
                nome: "a",
                depende: &[],
            },
            Spec {
                nome: "a",
                depende: &[],
            },
        ];
        let mut saida = [0usize; 2];
        assert_eq!(ordem(&specs, &mut saida), Err(DepErro::Duplicado("a")));
    }

    #[test]
    fn saida_pequena_e_recusada_antes_de_qualquer_trabalho() {
        let specs = [
            Spec {
                nome: "a",
                depende: &[],
            },
            Spec {
                nome: "b",
                depende: &[],
            },
        ];
        let mut saida = [0usize; 1];
        assert_eq!(ordem(&specs, &mut saida), Err(DepErro::Espaco));
    }

    #[test]
    fn tabela_vazia_e_valida() {
        let specs: [Spec; 0] = [];
        let mut saida = [0usize; 0];
        assert_eq!(ordem(&specs, &mut saida), Ok(0));
    }
}

# Compromissos de longa duração

Este arquivo é a fonte de verdade dos compromissos que **não fecham por trabalho, e sim por tempo
de relógio**: uma execução de dias, um agendamento semanal do CI, um incidente à espera de
reincidência. Nenhum deles aparece nas checklists do Plano Mestre (que rastreiam *itens*, não
*prazos*) nem em qualquer bateria de validação — por isso, quando são esquecidos, o esquecimento
é silencioso e só se descobre tarde.

Ele nasceu de um esquecimento real: o veredito do stress de 7 dias estava anotado como
`2026-09-14` no plano e no `ROADMAP_STATUS`, dois dias antes do prazo verdadeiro, porque a data
veio de um relançamento planejado (07/09) que na prática só aconteceu em 09/09 e ninguém
recalculou. A data certa vivia apenas num relatório de progresso.

`make prazos` (isto é, `tools/nexo-prazos`) lê este arquivo, calcula quanto falta para cada
prazo, confere se o processo que sustenta o compromisso continua vivo, lê o contador de saúde no
log e **sai com código 1** quando algo venceu, morreu ou acusou erro. Rode-o no início de cada
bloco, junto de `make lint` — assim o prazo é lembrado por construção, e não pela memória de
quem estiver trabalhando.

## Formato

Um `## <id>` em minúsculas por compromisso, seguido de campos `- chave: valor` (um por linha).
Campos desconhecidos são ignorados; campos ausentes desligam a verificação correspondente.

| campo | obrigatório | significado |
| --- | --- | --- |
| `estado` | sim | `aberto` ou `fechado` (fechado é histórico: não é verificado) |
| `o-que` | sim | uma linha dizendo o que é o compromisso |
| `inicio` | não | `AAAA-MM-DD` ou `AAAA-MM-DD HH:MM:SS` (hora local) |
| `vence` | não | idem; ausente significa "sem prazo, apenas vigiar" |
| `maquina` | não | `hostname` onde o compromisso roda; noutra máquina ele só é exibido |
| `processo` | não | padrão para `pgrep -f`; se não houver processo vivo, é falha |
| `log` | não | arquivo consultado para saúde e veredito |
| `saude` | não | regex com **um** grupo que precisa valer `0` na última linha que casar |
| `progresso` | não | regex com um grupo exibido como progresso (ex.: o `t=` do stress) |
| `veredito` | não | texto que, achado no log, significa que o compromisso **cumpriu** e cobra a ação |
| `acao` | sim | o que fazer quando vencer ou cumprir |
| `cuidado` | não | armadilha conhecida (exibida sempre) |
| `ref` | não | onde o compromisso está registrado no plano/documentação |
| `verificar` | não | comando de shell; código de saída diferente de zero vira alerta |

## stress-7d

- estado: aberto
- o-que: stress de 7 dias (604800 s, SMP=4) com o kernel do bloco 146 — terceira rodada
- inicio: 2026-09-11 09:19:32
- vence: 2026-09-18 09:19:32
- maquina: MacBook-Air-de-Lucas.local
- processo: nexo-stress-long.img
- pid: 54007 (QEMU; make lançador 53341, grupo 53340, ppid 1)
- log: build/logs/stress-604800s-20260911-091909.out
- progresso: \[STRESS\] t=(\d+)s
- saude: erros=(\d+)
- veredito: [STRESS] PASS duracao=604800s
- acao: escrever docs/progress/<data>-stress-7d.md com o veredito e os contadores finais, marcar o item de stress do Plano §6.1 como [x], rodar make roadmap e registrar no CHANGELOG
- cuidado: NUNCA matar este processo. As duas rodadas anteriores morreram de SIGTERM ao fim da sessão que as lançou (5,84 e 1,95 dias, zero erros): `nohup` + `disown` NÃO protege. Esta foi lançada por `make stress-desacoplado` (sessão própria + adotado pelo init, auto-teste passou); se morrer de novo, o mecanismo da limpeza é outro e precisa ser identificado antes de relançar
- ref: PLANO_MESTRE_SISTEMA_OPERACIONAL.md §6.1 "stress de 24h e posteriormente 7 dias"; docs/progress/2026-09-11-stress-7d-parcial-2.md

## stress-7d-run2

- estado: fechado
- o-que: stress de 7 dias, segunda rodada (kernel dos blocos 105-119), lançada com nohup
- inicio: 2026-09-09 10:17:20
- vence: 2026-09-16 10:17:20
- acao: morta de fora em 2026-09-11 09:05:16 aos 168 542 s (1,95 dias) com zero erros; log preservado em build/logs/stress-7d-parcial-2026-09-11.log
- ref: docs/progress/2026-09-11-stress-7d-parcial-2.md

## fuzz-semanal

- estado: aberto
- o-que: fuzzing semanal de syscalls e parsers no CI (workflow fuzz.yml, agendado)
- acao: ao encontrar uma execução vermelha, abrir incidente com a semente registrada no log (as sementes vêm do TSC e ficam no log justamente para reproduzir)
- cuidado: falha silenciosa — um workflow agendado não avisa ninguém; só se descobre olhando
- ref: .github/workflows/fuzz.yml; PLANO_MESTRE_SISTEMA_OPERACIONAL.md §6.4 "fuzzing contínuo"
- verificar: gh run list --workflow=fuzz.yml --limit 1 --json conclusion --jq '.[0].conclusion' | grep -qx success

## incidente-fs-ponteiro-nulo

- estado: aberto
- o-que: duas quedas inexplicadas do serviço fs (um #UD e uma escrita em null+0x20 dentro de write_entry) sem causa raiz
- acao: na próxima reincidência, converter rip-base com llvm-objdump, anotar o registrador do ponteiro e tentar relocation-model=static só no fs para isolar o PIE; para provocar, `tools/nexo-repro storage <n>` (12 execuções em 2026-09-10 não reproduziram)
- cuidado: nexofs é forbid(unsafe_code), então a origem é externa ao serviço — não procurar o bug dentro dele
- ref: docs/incidents/2026-09-09-fs-ponteiro-nulo.md

## painel-mensal

- estado: aberto
- o-que: preencher o painel mensal do Plano §10 (docs/progress/painel-mensal.md), cadência de 4 semanas
- inicio: 2026-08-29
- vence: 2026-09-26
- acao: medir de novo tudo o que o painel pede (testes, tempo de boot, RAM ociosa, riscos, decisões pendentes) e acrescentar um bloco novo, sem apagar o anterior — a série é que mostra a tendência
- cuidado: o conteúdo envelhece rápido (a medição de agosto ainda diz "40 testes de kernel"; hoje são 145) e ninguém repara, porque nada falha quando um painel fica velho
- ref: PLANO_MESTRE_SISTEMA_OPERACIONAL.md §10; docs/progress/painel-mensal.md

## revisao-trimestral

- estado: aberto
- o-que: revisão do Plano Mestre e do ROADMAP_STATUS (cadência trimestral definida pelo próprio plano)
- inicio: 2026-08-29
- vence: 2026-11-29
- acao: reler o plano inteiro, corrigir estimativas por fase, confirmar as decisões provisórias (nome, licença, horas) e agendar a revisão seguinte
- ref: docs/ROADMAP_STATUS.md §8

## stress-24h

- estado: fechado
- o-que: stress de 24 h exigido pelo gate F1
- inicio: 2026-08-31
- vence: 2026-09-01
- acao: cumprido em 2026-09-01 com zero erros
- ref: docs/progress/2026-09-01-stress-24h.md

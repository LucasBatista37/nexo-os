# Stress de 7 dias — resultado parcial (5,84 dias, zero erros) — 2026-09-01 a 2026-09-07

**O que era**: a rodada de 7 dias do gate F1 (`make stress DURATION=604800`, QEMU q35, 4 vCPUs,
512 MiB, imagem `nexo-stress-long.img` com disco de dados), iniciada em 2026-09-01 às 19:29,
com veredito previsto para 2026-09-08 às 19:30.

**O que aconteceu**: em 2026-09-07 às 15:42 o processo do QEMU foi morto **de fora** — no mesmo
instante em que o diretório temporário da sessão de trabalho foi apagado (limpeza do ambiente;
não houve pânico, exceção nem `[STRESS] FAIL` no log; a última linha é um relatório normal).
O guest não falhou: foi o host que encerrou o emulador.

**O que o log mostra até o corte** (`build/logs/stress-7d-parcial-2026-09-07.log`, 125 MB,
fora do git):

| Métrica | Valor |
|---|---|
| Duração | 505 031 s = **5,84 dias** (140,3 h; 83,5 % da meta) |
| Relatórios `[STRESS] t=…` | 505 031, todos com `erros=0` |
| Trocas de contexto | 723 039 860 |
| Preempções | 75 328 690 |
| Processos criados | 53 657 412 |
| Alocações | 158 628 692 |
| Dormidas | 122 694 289 |
| Páginas mapeadas/desmapeadas | 630 916 616 |
| Quadros livres no fim | estável (sem vazamento) |
| Erros | **0** |

O kernel em teste era o de 2026-09-01 (antes dos blocos 94–106); a rodada de 24 h do gate já
tinha passado com zero erros. A rodada completa de 7 dias **continua pendente**: uma nova foi
lançada logo depois deste relatório, com o kernel atual, e o veredito fica para
2026-09-14.

**Lição de método**: o ambiente temporário da sessão não é lugar para processos de longa
duração dependerem de nada (o log fica em `build/logs/`, mas o QEMU era filho da sessão).
A próxima rodada é lançada desacoplada (`nohup`), e o log é copiado a cada relatório diário.

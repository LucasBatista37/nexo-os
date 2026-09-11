# Stress de 7 dias — segundo resultado parcial (1,95 dias, zero erros) — 2026-09-09 a 2026-09-11

**O que era**: a segunda rodada de 7 dias do gate F1 (`make stress DURATION=604800 SMP=4`,
QEMU q35, 4 vCPUs, imagem `nexo-stress-long.img` com disco de dados, kernel dos blocos
105–119), iniciada em 2026-09-09 às 10:17:20, veredito previsto para 2026-09-16 às 10:17.

**O que aconteceu**: em 2026-09-11 às 09:05:16 o processo morreu **de fora**, pela segunda
vez pelo mesmo caminho: `make: *** [stress] Terminated: 15` (SIGTERM), no instante em que a
sessão de trabalho que o tinha lançado terminou. A última linha do guest é um relatório
normal com `erros=0`; não houve pânico, exceção nem `[STRESS] FAIL`.

O livro de prazos (`make prazos`, bloco 139) acusou `MORTO` na primeira verificação da
sessão seguinte — foi para isso que ele foi feito, e funcionou: a primeira rodada só foi
descoberta morta horas depois.

**O que o log mostra até o corte** (`build/logs/stress-7d-parcial-2026-09-11.log`, 39 MB,
fora do git):

| Métrica | Valor |
|---|---|
| Duração | 168 542 s = **1,95 dias** (46,8 h; 27,9 % da meta) |
| Relatórios `[STRESS] t=…` | 168 542, todos com `erros=0` |
| Trocas de contexto | 198 088 137 |
| Preempções | 21 438 319 |
| Processos criados | 15 090 360 |
| Alocações | 44 754 108 |
| Dormidas | 31 714 566 |
| Páginas mapeadas/desmapeadas | 175 043 648 |
| Quadros livres | entre 127 474 e 127 988 durante toda a rodada (sem vazamento) |
| Heap do kernel | entre 94 KiB e 166 KiB (sem crescimento) |
| Erros | **0** |

Somando as duas rodadas interrompidas, o kernel acumula **7,79 dias** de stress SMP com zero
erros — mas em duas execuções de kernels diferentes, o que **não** equivale a uma rodada
contínua de 7 dias. O gate continua pendente.

## Por que morreu, desta vez com diagnóstico

A primeira rodada foi lançada como filha da sessão; a lição registrada foi "lançar
desacoplada", e a segunda foi lançada com `nohup make stress … &` seguido de `disown`. Isso
foi insuficiente, e o motivo é preciso:

- `nohup` só faz o processo ignorar **SIGHUP**; o que chegou foi **SIGTERM**;
- `disown` só o tira da tabela de jobs do shell; ele continua no **mesmo grupo de processos
  e na mesma sessão** do shell, e continua **descendente** dele na árvore de processos.

A limpeza do ambiente mata por um desses dois caminhos (grupo de processos ou árvore de
descendentes) — o log não diz qual, e não é preciso saber: o novo lançador
(`tools/nexo-desacoplar`) fecha os dois ao mesmo tempo, com `setsid()` (sessão e grupo
novos) e fork duplo (o pai sai e o neto é adotado pelo init, pid 1).

A promessa "sobrevive ao fim da sessão" desta vez foi **testada antes** de arriscar outra
semana: `tools/nexo-desacoplar --auto-teste` lança um `sleep` por um shell intermediário,
manda SIGTERM ao grupo inteiro desse shell e confere três coisas — o intermediário morreu, o
`sleep` continua vivo, e está noutro grupo com ppid 1. Resultado na máquina do stress:

```
[desacoplar] auto-teste OK: o grupo 52821 morreu (rc=-15) e o pid 52824 sobreviveu no grupo 52823 com ppid 1
```

Isso prova que o lançador resiste ao golpe que reproduzimos. Não prova que a limpeza do
ambiente usa exatamente esse golpe: se a terceira rodada morrer, o mecanismo é outro e o
livro de prazos dirá — em minutos, não em dias.

## Terceira rodada

Lançada logo após este relatório por `make stress-desacoplado DURATION=604800 SMP=4`, com o
kernel do bloco 146; pid, início e log registrados em `docs/COMPROMISSOS.md` (`stress-7d`).
O veredito fica para sete dias depois do lançamento.

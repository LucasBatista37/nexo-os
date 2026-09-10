# Linha de base de desempenho

O projeto não tinha número nenhum. Estes são os primeiros — e o que eles valem depende de
ler as ressalvas antes dos valores.

## O que estes números NÃO são

Foram medidos **sob TCG do QEMU**, que interpreta cada instrução x86 em software, num host
macOS/aarch64. Não se comparam com hardware real, nem entre máquinas diferentes, nem sequer
entre execuções sob carga diferente do host. Um número duas vezes maior aqui pode ser o
ventilador do portátil, não o kernel.

Servem para duas coisas honestas: **comparar releases na mesma máquina** e **ver regressões
grosseiras** (uma ordem de grandeza). É o que um benchmark em CI pode prometer sem mentir.

Por isso nenhum deles é asserido contra um alvo. As asserções dos auto-testes `bench_switch` e
`user_bench` são apenas de sanidade: as trocas aconteceram, o relógio andou, as voltas todas
completaram. O valor vai para o log com o marcador `[BENCH]`.

## Para comparar, só A/B intercalado

O mesmo binário mediu a syscall nula em **308 ns** e em **531 ns** — a diferença foi a carga do
host, não o kernel. A tentação seguinte é normalizar: dividir cada medida pela syscall da mesma
execução e comparar as razões. **Não funciona**, e isso foi medido: a syscall e o IPC não
escalam juntos com a carga, de modo que a razão sobe quando a máquina está rápida e desce
quando está lenta. A razão continua impressa no marcador `[BENCH]` porque dá uma segunda vista,
mas nem ela nem o valor absoluto comparam duas execuções feitas em momentos diferentes.

O único método que funciona aqui é o **A/B intercalado**: construir as duas versões, correr
`antes, depois, antes, depois, …` na mesma máquina e comparar as séries. Foi assim que se
descobriu que uma otimização anunciada como melhoria não era (ver abaixo). `git worktree add`
numa revisão anterior torna isso barato.

Comparações **dentro da mesma execução** são válidas, porque partilham as condições: é o caso
de `ipc_ida_volta` contra `ipc_ida_volta_1cpu`.

## Medidas (2026-09-09, QEMU TCG, q35, 4 vCPUs, host macOS aarch64)

| Medida | Valor | Razão | O que inclui |
| --- | --- | --- | --- |
| troca de contexto | ~790 ns | — | duas threads de kernel presas à **mesma** CPU, `yield` em cadeia (40 000 trocas) |
| syscall nula | ~390–420 ns | 1× | `get_pid` do espaço de usuário: entrada em ring 0, despacho, volta (100 000 amostras) |
| IPC sem escalonamento | ~4 400 ns | ~10–12× | `send` + `recv` na mesma thread, pelas duas pontas: fila, cópia e handles, sem acordar ninguém |
| IPC ida-e-volta (CPUs livres) | ~37 000 ns | ~110× | contra outra thread que precisa acordar: bloqueio, `unpark`, duas trocas e, quando a outra thread está noutra CPU, um IPI |
| IPC ida-e-volta (mesma CPU) | ~24 500 ns | ~74× | as duas threads presas à CPU 0 (`thread_set_affinity`): o mesmo caminho **sem** o IPI |

## O que os números já mostram

- **A syscall é barata perto do IPC.** Uma ida-e-volta de mensagem custa mais de dez vezes uma
  syscall nula, e mais de cinco vezes só no mecanismo (sem escalonamento). O custo não está na
  fronteira usuário/kernel.
- **Cada mensagem alocava e deixou de alocar (bloco 134) — mas isso NÃO se viu no relógio.**
  `Message` carregava os dados num `Vec`; agora até 128 bytes viajam dentro da própria mensagem
  e os handles idem. Anunciei uma melhoria de "12,8× para 9,3×" comparando execuções feitas em
  momentos diferentes. O A/B intercalado depois desmentiu:

  | | syscall | IPC | razão |
  | --- | --- | --- | --- |
  | antes | 436 / 463 / 531 ns | 4591 / 4374 / 5319 ns | 10,5 / 9,4 / 10,0× |
  | depois | 426 / 364 / 379 ns | 4471 / 4367 / 4432 ns | 10,5 / 12,0 / 11,7× |

  As séries sobrepõem-se; pela razão, a versão nova até parece pior. **Não há melhoria
  mensurável neste microbenchmark.** O que a mudança dá continua a valer por si — nenhuma
  alocação no caminho de mensagem, incluindo dentro do handler de interrupção, o que tira
  pressão do heap e torna a latência mais previsível — mas isso é um argumento estrutural, não
  um número. O custo do IPC está noutro lugar, e achá-lo exige medir por dentro (contadores por
  etapa), não cronometrar o todo.

- **Acordar noutra CPU custa um terço da ida-e-volta.** Com as duas threads presas à mesma CPU,
  o mesmo trabalho cai de ~37 µs para ~24,5 µs — e esta comparação vale porque as duas medidas
  saem da **mesma execução**. O que sobra depois de tirar o IPI ainda é ~74× uma syscall: o
  grosso está no bloqueio e no despertar, não no envio.
- **Acordar custa caro sob TCG.** A ida-e-volta com escalonamento é dez vezes o mecanismo puro,
  bem acima das duas trocas de contexto que ela contém. Antes de concluir qualquer coisa sobre
  o `park`/`unpark`, é preciso medir em hardware — sob TCG, entrar e sair de `hlt` e mexer no
  LAPIC são operações desproporcionalmente caras.

## Como reproduzir

```
make image && tools/run-qemu --test | grep BENCH
```

Os dois marcadores aparecem no cenário `boot` do `tools/test-qemu`, que exige a presença de
ambos — um benchmark que deixa de correr em silêncio não é linha de base nenhuma.

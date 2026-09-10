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

## Compare pelas razões, não pelos valores

O mesmo binário mediu a syscall nula em **322 ns** e em **430 ns** na mesma tarde — a diferença
foi a carga do host, não o kernel. Por isso o marcador `[BENCH]` traz, além dos nanossegundos,
a razão de cada medida contra a **syscall medida na mesma execução**. É a razão que se compara
entre execuções; o valor absoluto serve só para dar ordem de grandeza.

## Medidas (2026-09-09, QEMU TCG, q35, 4 vCPUs, host macOS aarch64)

| Medida | Valor | Razão | O que inclui |
| --- | --- | --- | --- |
| troca de contexto | ~790 ns | — | duas threads de kernel presas à **mesma** CPU, `yield` em cadeia (40 000 trocas) |
| syscall nula | ~390–420 ns | 1× | `get_pid` do espaço de usuário: entrada em ring 0, despacho, volta (100 000 amostras) |
| IPC sem escalonamento | ~3 800–4 100 ns | **9,3–10,6×** (era 12,7–12,9×) | `send` + `recv` na mesma thread, pelas duas pontas: fila, cópia e handles, sem acordar ninguém |
| IPC ida-e-volta | ~40 000 ns | ~95× | contra outra thread que precisa acordar: inclui bloqueio, `unpark` e duas trocas |

## O que os números já mostram

- **A syscall é barata perto do IPC.** Uma ida-e-volta de mensagem custa mais de dez vezes uma
  syscall nula, e mais de cinco vezes só no mecanismo (sem escalonamento). O custo não está na
  fronteira usuário/kernel.
- **Cada mensagem alocava — e deixou de alocar** (bloco 134). `Message` carregava os dados num
  `Vec`, então enviar era alocar no heap do kernel e receber era libertar. Mensagens do sistema
  são quase todas pequenas (um pedido de bloco, um evento de entrada, uma resposta de status),
  e agora até 128 bytes viajam **dentro** da própria mensagem; os handles idem (no máximo oito,
  cabem na pilha). O custo relativo do IPC sem escalonamento caiu de **12,7–12,9× a syscall
  para 9,3–10,6×** (cinco medições depois, duas antes) — a melhoria que a própria linha de base
  tornou possível verificar. A razão também tem ruído; um número único aqui seria escolher a
  medição mais simpática.
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

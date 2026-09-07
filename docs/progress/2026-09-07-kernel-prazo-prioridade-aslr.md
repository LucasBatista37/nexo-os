# Prazo, prioridade, ASLR completo e tique dinâmico — 2026-09-05 a 2026-09-07

Oito blocos em `main` (CI verde no HEAD de cada push, SHA conferido a cada watch). Depois do
bug de escalonador do bloco 94, o kernel ganhou quatro capacidades que estavam no plano
desde a Fase 1/2 e duas atualizações de documentação que estavam atrás do código.

| Bloco | Commit | O que entrou |
|---|---|---|
| 96 | `11148e7` | plano em dia (ABI C, memória, syscalls versionadas, dump) + relatório 89–95 |
| 97 | `800a8ce` | syscall 34 `channel_wait_any_timeout` + `Status::TimedOut`; o lançador deixa de sondar |
| 98 | `fbdbffa` | prioridades normal/baixa por thread; syscall 35 `set_priority`; preempção no tique seguinte |
| 99 | `1e6817a` | threat model v1 em `SECURITY.md` |
| 101 | `351f77d` | driver da calc espera a janela (flake do cenário `gdb`) |
| 100 | `ca5524d` | ASLR de pilha (1 GiB) e de mapeamentos (4 GiB); `kernel/src/aslr.rs` |
| 102 | `a46858f` | PIE: programas Rust `ET_DYN` numa base aleatória (1 TiB), relocações `R_X86_64_RELATIVE` no kernel |
| 103 | `cb7e79b` | tique dinâmico na BSP (LAPIC one-shot); `sleep_ns` real; dormidas e timers abaixo de 1 ms |

## Os fios

- **Espera com prazo (97)**: `channel_wait_any` ganhou um irmão com prazo em ns (aditivo,
  `TimedOut` = 12). É o timer de usuário simples que faltava: um laço de eventos dorme até o
  próximo evento OU o fim do prazo. O primeiro consumidor foi o lançador — a permissão
  temporária sondava a cada 20 ms — e a mesma mudança recolhe o proxy quando uma ponta
  morre (uma ponta fechada fica "pronta" para sempre e transformaria a espera em sondagem).
- **Prioridades (98)**: duas classes por thread, herdadas no spawn. A fila prefere as normais
  e uma normal pronta preempta a de baixa em execução já no tique seguinte, sem esperar o
  quantum de 10 ms (também na IPI de reescalonamento). O teste põe dois giradores de baixa
  prioridade presos à CPU 1 e mede 100 ciclos dormir/acordar de uma thread normal na mesma
  CPU: bem abaixo de 1 s (sem prioridades cada despertar esperaria até dois quanta).
- **ASLR completo (100 + 102)**: pilha num topo aleatório (18 bits), região de mapeamentos
  num deslocamento aleatório (20 bits) e, com o PIE, o **código** numa base aleatória (28
  bits). Entropia por `rdrand` quando há, senão xorshift64* semeado pelo TSC — para dispersar
  endereços, não para chaves. Nada no espaço de usuário dependia dos endereços fixos: a
  pilha chega em `RSP`, os mapeamentos devolvem o endereço e o PIE só tem relocações
  `RELATIVE` (o kernel recusa qualquer outro tipo e tabelas fora dos segmentos). Os programas
  C do toolchain continuam `ET_EXEC` em endereço fixo — o carregador aceita os dois; o loader
  de boot segue exigindo `ET_EXEC` para o kernel.
- **Tique dinâmico (103)**: o timer do LAPIC da BSP virou one-shot, re-armado a cada disparo
  para o próximo prazo (dormida, timer de kernel ou o tique de 1 ms) — **antes** de escalonar,
  porque o handler pode trocar de thread e só voltar muito depois. Um prazo mais cedo que o
  armado re-arma na hora ou, de outra CPU, manda uma IPI do vetor do timer à BSP. As APs
  mantêm o tique periódico para o quantum. `sleep` em ns de verdade.
- **Documentação (96, 99)**: quatro cláusulas do plano diziam "pendente" para coisas que já
  existiam (ABI C, SMP no item de memória, syscalls versionadas, dump em disco); o threat
  model v0 descrevia um kernel em ring 0 sem modo usuário — o v1 cobre o sistema de hoje,
  vetor a vetor, com o que mitiga e o que fica para depois.

## Lições de método

- Um bloco de *userland* (proxy de capacidade) revelou o único bug de escalonador até hoje
  (bloco 94); a sondagem de 20 ms que ele substituiu escondia a janela. Sondagens são
  dívida — e o 97 tirou a última do lançador.
- Flakes de tempo fixo (300 ms pela janela da calc) aparecem primeiro no cenário mais lento
  (`gdb`); a resposta é esperar o evento (`surface_info`), não aumentar o tempo.
- Scripts de cadeia: `grep -c` devolve 1 com zero ocorrências (mata `set -e`); `run-qemu
  --test` sem `exit` na linha de comando não encerra o QEMU — o veredito é o `[RESULT]`.

## Estado

- Suíte: 123 testes; varredura de 11 cenários verde; ABI com 36 syscalls (0–35).
- Stress de 7 dias em curso desde 2026-09-01 19:29 (zero erros aos 485 000 s); veredito e
  relatório em 2026-09-08.
- Próximos candidatos: jobs/domínios e quotas de CPU (Fase 2), binding por classe PCI no
  `devmgr`, dependências declarativas de serviços.

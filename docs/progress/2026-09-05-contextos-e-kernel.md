# Contextos fechando, um bug de escalonador e avisos com ação — 2026-09-05

Sete blocos em `main` (CI verde no HEAD de cada push, SHA conferido a cada watch). Três fios:

- **Fase 6/7 — POSIX em C até a rede**: a nexo-libc ganhou `sys/socket.h` sobre o protocolo
  `nexo.sock` gerado do IDL, e um `fetch` em C baixa uma página no cenário `net` (bloco 89).
  O toolchain empacotado tinha três buracos (sem `fd.c`/`socket.c` no `.a`, sem `sys/`, `crt0.o`
  apagado pela limpeza) — um segundo auto-teste do `nexo-cc` agora linka arquivos e sockets.
- **Fase 5/6 — Contextos**: painel de escala nas Configurações (bloco 90), documentos por
  Contexto no compositor (bloco 91), a convenção de processos como ADR-0017 (bloco 92) e a
  **revogação fina por capacidade** (bloco 93): o lançador entrega ao app um *proxy* da sessão
  de janelas e revoga fechando-o — o app perde as janelas ainda vivo.
- **Kernel**: o bloco 93 derrubou o kernel de forma reproduzível e o bloco 94 corrigiu um bug
  real do escalonador; o bloco 95 fechou o item de notificações com a ação no banner.

## Blocos

| Bloco | Commit | O que entrou |
|---|---|---|
| 89 | `7734fb4` | `sys/socket.h` na libc (TCP/UDP/DNS via `nexo.sock`), `fetch` em C na fase 4 do `net`, toolchain corrigido |
| 90 | `3e23a1e` | toggle **ES** nas Configurações pede "escala 1 2" ao shell; **bug**: `pointer` chegava em coordenadas de exibição — agora do buffer (`to_buf_coords`) |
| 91 | `935ca42` | `nexo.wm` v1.21: `set_document` + `surface_info.document`; editor e visor declaram o caminho aberto |
| 92 | `cb9e200` | ADR-0017 — convenção de processos (h0..h3) e a nexo-libc; índice de ADRs completo |
| 94 | `a2b2a5f` | escalonador: `join` sem duplicata, `finish_switch` só acorda bloqueados; `threads_join_spurious_wake` |
| 93 | `66f9c3f` | proxy da sessão no lançador; "por tempo" revoga (`revogado`) 200 ms antes de encerrar (`expirou`) |
| 95 | `9229856` | clique no banner ativa a janela de origem do aviso e o recolhe; item de notificações `[x]` |

## O bug do escalonador (bloco 94)

A suíte do bloco 93 caía com um *page fault de busca de instrução* num endereço de heap, na
thread do lançador, `rbp = 0`, sempre logo depois de o app sair. Os registradores da segunda
queda tinham cara de quadro de syscall de usuário (`rflags = 0x286`, `rsp` de usuário) escrito
na pilha de kernel do lançador — duas CPUs na mesma pilha.

A cadeia: o `wait_any` do lançador deixa waiters obsoletos nos canais do proxy; quando o app
sai, o `close` da ponta dele acorda esse waiter (`unpark`) **enquanto o lançador está bloqueado
em `join`** (`process_wait`). O `join` dá uma volta a mais e **re-insere** a própria thread na
lista de espera do alvo; quando o alvo morre, `finish_switch` punha **todos** os esperadores na
fila de prontos sem olhar o estado — a mesma thread duas vezes, duas CPUs, uma pilha.

Correção em duas linhas de defesa: `join` não duplica a própria entrada (`join_dedups`) e
`finish_switch` só re-enfileira quem está `Blocked` (`join_skips`). Os dois contadores saem na
linha `[SCHED]` da suíte e no veredito `[STRESS]`; a regressão `threads_join_spurious_wake`
dispara `unpark`s espúrios num joiner cujo alvo ainda dorme (na suíte: `join_dedups=5`,
`join_skips=1`). A ordem dos commits inverteu (94 antes de 93) para que cada commit fique
verde por si.

Lição para o método: um bloco de *userland* (proxy de capacidade) foi o primeiro a combinar
`wait_any` + `join` + morte do par — e a sonda de 20 ms que ele substituiu escondia a
janela. O bloco 97 (espera com prazo) tira a sondagem de vez.

## Estado

- Contextos: documentos, permissões temporárias e revogação fina existem; o que resta do
  item é experiência de uso com gente de fora (gate).
- Notificações: banner, dismiss, não-perturbe, Central, por Contexto e ação — `[x]`.
- Suíte: 119 testes; varredura de 11 cenários verde; stress de 7 dias em curso (veredito
  previsto para 2026-09-08).

# ADR-0017 — Convenção de processos (h0..h3) e a nexo-libc: a personalidade POSIX em C

- **Status:** aceita (convenção **estável** dentro da ABI v1 experimental)
- **Data:** 2026-09-05
- **Relacionados:** ADR-0014 (compatibilidade POSIX como personalidade), ADR-0011 (IPC por
  canais/handles), Plano §3.8 (ordem da compatibilidade) e §Fase 6 ("portar toolchain e
  utilitários POSIX"); `docs/sdk.md` (guia), `docs/spec/ipc-compat.md`

## Contexto

O Nexo não tem `fork`, descritores herdados nem ambiente: um processo nasce com um vetor de
**handles iniciais** (ADR-0011) e nada mais. Para portar ferramentas POSIX de linha de comando
(a etapa 3 da ordem do §3.8) era preciso decidir **como `argc/argv`, `stdin` e `stdout` chegam a
um programa C** sem inventar um mecanismo novo no kernel — e fazê-lo de um jeito que um shell
consiga compor programas em pipelines.

## Decisão

1. **Quatro handles, posicionais, válidos no arranque.**

   | Handle | Papel |
   |---|---|
   | 0 | serviço principal (ex.: o canal `nexo.fs`; a camada de arquivos da libc liga-se a ele com `nexo_libc_use_fs(0)`) |
   | 1 | **argv**: um canal com UMA mensagem já enviada — argumentos separados por `\0`, `argv[0]` incluso; sem canal ou sem mensagem, `argc = 0` |
   | 2 | **stdin** (opcional): cada mensagem é um pedaço; fila vazia com a outra ponta viva bloqueia; `PeerClosed` é o EOF |
   | 3 | **stdout** (opcional): bytes fiéis por canal (`\n` incluso) — um pipe; sem ele, a saída vai ao log do kernel em linhas |

   Nada disso exige kernel novo: argv é uma mensagem numa fila; EOF é o fecho da outra ponta;
   um pipe é um canal cujas pontas são o stdout de um processo e o stdin do seguinte.

2. **O `crt0` da libc materializa a convenção** — lê o argv, fixa a existência de stdin/stdout
   **antes do `main`** (as sondas por slot só valem no arranque: um programa que cria ou
   transfere handles reutiliza os slots 2/3) e dá o `flush` da última linha no `exit`.

3. **A nexo-libc é mínima e cresce por demanda**: `string.h`, `stdio.h` (`printf` próprio, saída
   em linhas ou por canal), `stdlib.h` (heap sobre objetos de memória do kernel, `atoi`),
   `fcntl.h`/`unistd.h` (fd sobre o `nexo.fs` gerado do IDL: open/read/write/lseek/close/unlink/
   rename/mkdir), `dirent.h`, `sys/socket.h` (BSD sobre o `nexo.sock` gerado do mesmo IDL).
   Os protocolos em C são **gerados do MESMO IDL** que os do Rust (`tools/idlgen`, backend C):
   nunca desatualizam.

4. **Composição é papel de quem lança, não do kernel**: o mini `sh` cria os canais, envia o
   argv de cada estágio, **duplica o h0** para cada um (a capacidade emprestada), lança
   `<cmd>-c` do initrd e espera; redireções são bombeadas antes do spawn (`<`) e drenadas depois
   dos waits (`>`) — o serviço do h0 nunca é usado por dois processos ao mesmo tempo.

5. **O que fica de fora, de propósito**: `fork`/`exec` (não há herança implícita de nada),
   variáveis de ambiente (não há convenção de ambiente; um `$VAR` sem fonte seria teatro),
   sinais, threads em C (o kernel tem threads; a libc ainda não as expõe) e TLS/assinatura
   (decisão de criptografia adiada pelo usuário).

## Consequências

- Um programa C portável precisa de `main(argc, argv)` + as chamadas POSIX cobertas — e de
  nada mais: onze utilitários (`wc cat ls cp rm mv echo head mkdir grep sort`), o `sh` e o
  `fetch` (HTTP em C) rodam sem uma linha específica do Nexo além de `nexo_libc_use_*`.
- Toda entrada/saída é um canal: um shell gráfico ou um serviço pode ser a outra ponta do
  stdout de qualquer utilitário (o shell de diagnóstico bombeia a saída para a console).
- Ferramentas que assumem `fork`, sinais ou ambiente não portam sem adaptação — assumido.

## Alternativas rejeitadas

- **Bloco de ambiente/argv em memória mapeada no spawn**: exigiria ABI nova no kernel; a
  mensagem numa fila faz o mesmo com o que já existe.
- **stdout sempre por canal**: programas sem consumidor precisariam de um dreno; o log em
  linhas como fallback mantém o utilitário útil sozinho.
- **Emular `fork`**: incompatível com o modelo de capacidades (nada é herdado implicitamente).

## Evidências

Auto-testes de boot `user_wc` … `user_grep_regex` (100º–118º), `user_sh*`, `user_pipe`; cenários
`boot` (marcadores de conteúdo), `shell` (utilitários pelo vfs duplicado) e `net` (fase 4:
`fetch` em C). Lições registradas no CHANGELOG (blocos 65–89) e em `docs/sdk.md`.

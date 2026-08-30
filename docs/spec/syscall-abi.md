# Especificação — ABI de syscalls (versão 0, instável)

**Crate de referência:** `abi/syscall` (`nexo-syscall-abi`). **SDK mínimo:** `sdk/nexo-sys`. **Implementação:** `kernel/src/x86/syscall.rs`, entrada em `arch/x86_64/src/syscall.rs`.

Antes de `0.9-beta` esta ABI muda sem aviso (ADR-0006). A versão é consultável por `SYS_ABI_VERSION`.

## 1. Convenção (x86_64)

| Item | Valor |
|---|---|
| Instrução | `syscall` (ring 3 → ring 0 via `LSTAR`); retorno por `sysretq` |
| Número | `RAX` |
| Argumentos | `RDI`, `RSI`, `RDX`, `R10`, `R8`, `R9` (a v0 usa até 3) |
| Retorno | `RAX` = `Status` (0 = OK); `RDX` = valor |
| Destruídos | `RCX`, `R11` (pela instrução); demais registradores preservados |
| Pilha | o kernel troca para a pilha de kernel da thread (`gs:[8]`); a pilha do usuário não é tocada |
| Interrupções | mascaradas na entrada (`SFMASK`), reabilitadas dentro do kernel; a syscall pode bloquear |

Seletores: código do usuário `0x2b`, dados `0x23` (`STAR[63:48] = 0x18`); código do kernel `0x08`, dados `0x10`.

## 2. Syscalls v0

| Nº | Nome | Args | Retorno (`RDX`) | Erros |
|---|---|---|---|---|
| 0 | `exit` | código (i64) | não retorna | — |
| 1 | `log` | ptr, len (≤ 1024, UTF-8) | bytes escritos | `BadAddress`, `InvalidArgs` |
| 2 | `time_now` | — | ns monotônicos | — |
| 3 | `yield` | — | 0 | — |
| 4 | `sleep` | ns | 0 | — |
| 5 | `get_pid` | — | pid | — |
| 6 | `abi_version` | — | 0 | — |
| 7 | `debug_info` | 0 CPUs online / 1 uptime ms / 2 syscalls do processo | valor | `InvalidArgs` |

| 8 | `handle_close` | h | 0 | `BadHandle` |
| 9 | `handle_duplicate` | h, rights | novo handle | `BadHandle`, `Denied` (sem `DUPLICATE` ou tentando ampliar direitos) |
| 10 | `channel_create` | — | `h0 \| (h1 << 32)` | `NoMemory` |
| 11 | `channel_send` | h, ptr, len, handles_ptr, n | bytes enviados | `BadHandle`, `Denied` (sem `WRITE`/`TRANSFER`), `TooBig` (> 4096 B ou > 8 handles), `BadAddress`, `PeerClosed`, `QueueFull` (> 64 pendentes), `InvalidArgs` (enviar o próprio canal) |
| 12 | `channel_recv` | h, buf, cap, handles_buf, hcap | `len \| (nhandles << 32)` | `BadHandle`, `Denied` (sem `READ`), `BadAddress` (buffers não graváveis), `PeerClosed` (par fechado e fila vazia), `TooBig` (mensagem descartada; `RDX` traz os tamanhos necessários) |
| 13 | `handle_info` | h | `rights \| (kind << 32)` | `BadHandle` |
| 14 | `process_spawn` | name_ptr, name_len (≤ 32), arg, handles_ptr, n | handle do processo filho | `NotFound` (membro ausente no initrd), `Denied` (handle sem `TRANSFER`), `TooBig`, `BadAddress` |
| 15 | `process_wait` | h | código de saída (i64) | `BadHandle`, `Denied` (sem `READ`), `InvalidArgs` (não é processo / é o próprio) — bloqueia |
| 16 | `process_info` | h | `pid \| (1 << 63 se terminou)` | `BadHandle`, `InvalidArgs` |

Números desconhecidos devolvem `NotSupported` (3) sem efeitos. `channel_recv` bloqueia a thread até haver mensagem ou o par fechar.

## 3. Status

`Ok`=0, `InvalidArgs`=1, `BadAddress`=2, `NotSupported`=3, `NoMemory`=4, `NotFound`=5, `Denied`=6, `PeerClosed`=7, `BadHandle`=8, `WouldBlock`=9 (reservado), `TooBig`=10, `QueueFull`=11.

## 3.1 Handles e direitos (ADR-0004)

- Handle = índice `u32` na tabela do processo (até 256); opaco e não forjável (o kernel valida índice, presença e direitos em toda syscall).
- Direitos: `READ`=1, `WRITE`=2, `TRANSFER`=4, `DUPLICATE`=8, `SIGNAL`=16, `MAP`=32, `ADMIN`=64. Só diminuem: `handle_duplicate` aceita apenas subconjuntos.
- Objetos v0: extremidade de canal (`kind` 1), criada com `READ|WRITE|TRANSFER|DUPLICATE`; processo (`kind` 2), criado por `process_spawn` com `READ|TRANSFER|DUPLICATE` (`READ` = esperar/consultar). Os handles iniciais passados no spawn ocupam os índices 0.. na tabela do filho.
- Handles enviados em uma mensagem saem da tabela do remetente e entram na do destinatário (índices novos) no `recv`; exigem `TRANSFER`.
- Ao terminar, o processo fecha todos os handles; a última extremidade fechada de um canal libera o objeto; o par vê `PeerClosed`.

## 3.2 Canais (ADR-0005)

Mensagem = até 4096 bytes + até 8 handles; fila de 64 por extremidade. Sem cabeçalho/protocolo tipado ainda (IDL e versionamento de protocolo vêm no próximo bloco). O kernel copia os bytes para memória própria no `send` e para o usuário no `recv`.

## 4. Validação de ponteiros

Todo ponteiro de usuário é validado antes do acesso: faixa `[ptr, ptr+len)` abaixo de `0x0000_8000_0000_0000` e cada página mapeada com o bit `USER` no espaço do processo; caso contrário `BadAddress`. O kernel copia os bytes para memória própria antes de usá-los.

## 5. Processos nesta versão

- Espaço de endereçamento por processo (PML4 própria; metade do kernel compartilhada), carregado de um ELF64 estático com segmentos W^X; pilha de 64 KiB em `0x0000_7fff_fff0_0000` (guard page abaixo).
- Uma thread por processo; `RDI` na entrada carrega um argumento inteiro.
- Falha em modo usuário (`#PF`, `#GP`, `#UD`…) encerra apenas o processo com código `-1` e motivo registrado no log; o kernel continua.
- Handles com direitos, canais com transferência de handles e processos como objetos (spawn por nome do initrd, wait, info) existem (§3.1–3.2, syscalls 14–16); memória compartilhada, jobs/domínios, eventos/espera múltipla e timers de usuário vêm nos próximos blocos.
- Programas: o initrd (`kernel/lib/initrd`, formato `NEXOIRD1`, gerado por `tools/mkinitrd.py`) contém `init`, `svcmgr`, `echo`, `echo-client` e `utest`. `init` inicia `svcmgr`; `svcmgr` supervisiona `echo` (reinício até 3 vezes) e atende pedidos de conexão de `echo-client` entregando um canal por pedido.

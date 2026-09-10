# Especificação — ABI de syscalls (versão 1, experimental)

**Crate de referência:** `abi/syscall` (`nexo-syscall-abi`). **SDK mínimo:** `sdk/nexo-sys`. **Implementação:** `kernel/src/x86/syscall.rs`, entrada em `arch/x86_64/src/syscall.rs`.

**Política da v1 (experimental):** mudanças são **aditivas** — syscalls novas recebem números
novos (o próximo é 33); structs de ABI só crescem por campos com padrão zero no fim; protocolos
IPC seguem o ipc-compat §3. Qualquer quebra exige subir `ABI_VERSION` (consultável por
`SYS_ABI_VERSION`; hoje = 1) e registro no CHANGELOG. A promoção a "estável" vem com o uso por
terceiros (gate F6) e o marco `0.9-beta` (ADR-0006). A v1 congela o conjunto atual: 47 syscalls
(0–46), handles com direitos que só diminuem, canais NXIP, memória compartilhada, os dois spawns,
jobs, threads de usuário e objetos de evento.

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

## 2. Syscalls v1

| Nº | Nome | Args | Retorno (`RDX`) | Erros |
|---|---|---|---|---|
| 0 | `exit` | código (i64) | não retorna | — |
| 1 | `log` | ptr, len (≤ 1024, UTF-8) | bytes escritos | `BadAddress`, `InvalidArgs` |
| 2 | `time_now` | — | ns monotônicos | — |
| 3 | `yield` | — | 0 | — |
| 4 | `sleep` | ns | 0 | — |
| 5 | `get_pid` | — | pid | — |
| 6 | `abi_version` | — | 0 | — |
| 7 | `debug_info` | 0 CPUs online / 1 uptime ms / 2 syscalls do processo / 3 handles do processo / 4 processos vivos / 5 quadros livres / 6 quadros utilizáveis / 7 segundos Unix UTC (0 = sem RTC), 8 = tempo de CPU do processo (ns, todas as threads) / 9 = faltas de página recuperadas em cópias usuário↔kernel (corridas de desmapeamento vencidas pelo fixup; 0 é o esperado) | valor | `InvalidArgs` |

| 8 | `handle_close` | h | 0 | `BadHandle` |
| 9 | `handle_duplicate` | h, rights | novo handle | `BadHandle`, `Denied` (sem `DUPLICATE` ou tentando ampliar direitos) |
| 10 | `channel_create` | — | `h0 \| (h1 << 32)` | `NoMemory` |
| 11 | `channel_send` | h, ptr, len, handles_ptr, n | bytes enviados | `BadHandle`, `Denied` (sem `WRITE`/`TRANSFER`), `TooBig` (> 4096 B ou > 8 handles), `BadAddress`, `PeerClosed`, `QueueFull` (> 64 pendentes), `InvalidArgs` (enviar qualquer ponta do próprio canal, ou o mesmo handle duas vezes na lista) |
| 12 | `channel_recv` | h, buf, cap, handles_buf, hcap | `len \| (nhandles << 32)` | `BadHandle`, `Denied` (sem `READ`), `BadAddress` (buffers não graváveis), `PeerClosed` (par fechado e fila vazia), `TooBig` (mensagem descartada; `RDX` traz os tamanhos necessários) |
| 13 | `handle_info` | h | `rights \| (kind << 32)` | `BadHandle` |
| 14 | `process_spawn` | name_ptr, name_len (≤ 32), arg, handles_ptr, n | handle do processo filho | `NotFound` (membro ausente no initrd), `Denied` (handle sem `TRANSFER`), `TooBig`, `BadAddress` |
| 15 | `process_wait` | h | código de saída (i64) | `BadHandle`, `Denied` (sem `READ`), `InvalidArgs` (não é processo / é o próprio) — bloqueia |
| 16 | `process_info` | h | `pid \| (1 << 63 se terminou)` | `BadHandle`, `InvalidArgs` |
| 17 | `pci_enum` | dev, buf, cap | nº total de funções PCI (copia até `cap` `PciInfo` em `buf`) | `BadHandle`, `Denied` (sem `READ`), `BadAddress` |
| 18 | `pci_cfg_read` | dev, bdf, offset (múltiplo de 4). **≤ 0xfc** pelo mecanismo legado; **0x100..0xffc** exige ECAM (tabela `MCFG`) e é a configuração estendida do PCIe | valor de 32 bits | `BadHandle`, `Denied` (sem `READ`), `InvalidArgs`, `NotSupported` (≥ 0x100 sem ECAM, ou ≥ 0x1000) |
| 19 | `pci_cfg_write` | dev, bdf, offset, valor | 0 | `BadHandle`, `Denied` (sem `WRITE`), `InvalidArgs` |
| 20 | `mmio_map` | dev, phys (alinhado a 4 KiB), len (≤ 16 MiB) | endereço virtual (região `0x6000_0000_0000`, sem cache) | `BadHandle`, `Denied` (sem `MAP`, ou faixa fora de um BAR MMIO enumerado), `InvalidArgs`, `NoMemory` |
| 21 | `dma_alloc` | dev, out (`DmaBuffer`) | endereço virtual da página (4 KiB zerada, contígua, mapeada `RW`) | `BadHandle`, `Denied` (sem `MAP`), `NoMemory`, `BadAddress` |
| 22 | `irq_alloc` | dev, out (`IrqInfo`) | vetor (0x50–0x6f) com endereço/dados MSI para a BSP | `BadHandle`, `Denied` (sem `SIGNAL`), `NoMemory` (pool esgotado), `BadAddress` |
| 23 | `irq_wait` | dev, vetor, visto | contagem atual de disparos (bloqueia até `> visto`) | `BadHandle`, `Denied` (sem `SIGNAL`), `InvalidArgs` |
| 25 | `channel_try_recv` | como `channel_recv`, mas devolve `WouldBlock` (9) em vez de bloquear quando não há mensagem e o par está aberto | idem `channel_recv` |
| 28 | `memory_create` | páginas (1..=256) → handle de memória (`kind` 4, `READ\|WRITE\|MAP\|TRANSFER\|DUPLICATE`); páginas zeradas | `InvalidArgs`, `NoMemory` |
| 29 | `memory_map` | mem (`MAP`) → endereço virtual (região de dispositivos, `USER\|RW` cacheável); o mesmo objeto pode ser mapeado por vários processos = memória compartilhada | `BadHandle`, `Denied`, `InvalidArgs`, `NoMemory` |
| 30 | `memory_unmap` | base, tamanho (múltiplos de página, na região de dispositivos) → desmapeia as páginas compartilhadas (limpa PTEs + invalida TLB) **sem** liberar os quadros; para realocar buffers (ex.: redimensionar superfícies) | `InvalidArgs` |
| 31 | `fb_info` | ponteiro → escreve o `FbInfo` (40 bytes: base física, tamanho, largura/altura/stride, formato, bpp) do framebuffer de boot. Só **informação**: o mapeamento continua gated pela concessão do dispositivo de vídeo (`mmio_map` — o framebuffer é um BAR) | `NotSupported` (sem framebuffer), `InvalidArgs` |
| 32 | `process_spawn_mem` | ELF na memória do chamador (ptr, tamanho ≤ 2 MiB), argumento e handles transferidos como no `process_spawn` → handle do processo. É como aplicativos **instalados** (fora do initrd) executam; mesmas validações (faixa, W^X) e isolamento | `TooBig`, `BadAddress`, `Denied`, `InvalidArgs` (ELF inválido) |
| 33 | `trace` | 0 desliga / 1 liga (`rsi` = handle de depuração, kind 5) / 2 lê (`rsi`/`rdx` = ptr/cap em entradas de 16 B; `r10` = handle de depuração) / 3 total (livre) | 2: copiadas; 3: total | `Denied` sem a capability; `InvalidArgs` |
| 34 | `channel_wait_any_timeout` | ptr (array de handles u32 — canais **e eventos**), n (1..=16), prazo em ns (0 = só sonda) → índice do primeiro pronto; `TimedOut` (12) ao esgotar o prazo (granularidade do tique de 10 ms) | `BadHandle`, `Denied`, `InvalidArgs`, `BadAddress`, `TimedOut` |
| 35 | `set_priority` | 0 (normal) / 1 (baixa, segundo plano): prioridade da thread chamadora — "nice", não fronteira de segurança; a fila prefere as normais e uma normal pronta preempta uma de baixa no tique seguinte; processos criados herdam | ok | `InvalidArgs` fora de 0/1 |
| 36 | `job_create` | — → handle de job (`KIND_JOB` = 6; direitos `READ\|WRITE\|TRANSFER\|DUPLICATE\|ADMIN`) | `NoMemory`/`TooBig` (tabela cheia) |
| 37 | `job_attach` | job (`ADMIN`), processo (`READ`) → anexa; os processos que o membro criar herdam o job | `BadHandle`, `Denied`, `InvalidArgs` (não é job/processo) |
| 38 | `job_kill` | job (`ADMIN`) → mata todos os membros vivos: tabelas de handles fechadas (pares veem `PeerClosed`), cada um sai com `EXIT_KILLED` na próxima syscall ou ao acordar de uma espera; idempotente; o chamador membro morre por último | `BadHandle`, `Denied`, `InvalidArgs` |
| 39 | `thread_create` | entrada (`extern "C" fn(u64) -> !`, termina com `thread_exit`), argumento (chega em `RDI`) → handle de thread (`KIND_THREAD` = 7; `READ\|TRANSFER\|DUPLICATE`); pilha própria de 256 KiB em endereço aleatório; handles e memória partilhados | `InvalidArgs` (entrada fora da faixa), `NoMemory` |
| 40 | `thread_exit` | — → termina a thread; a última viva termina o processo com 0 | — (nunca retorna) |
| 41 | `thread_join` | h (thread deste processo) → espera terminar | `BadHandle`, `Denied`, `InvalidArgs` (não é thread daqui / é a própria) — bloqueia |
| 42 | `job_set_cpu_limit` | job (`ADMIN`), ns por janela de 1 s (0 = sem limite) → define a quota de CPU do job; passando do orçamento as threads dos membros deixam de ser escalonadas até a janela seguinte (o excedente é cobrado dela) | `BadHandle`, `Denied`, `InvalidArgs` (não é job) |
| 44 | `event_create` | auto (1 = a espera consome o sinal, uma thread por sinalização; 0 = manual, fica sinalizado até `event_reset`) → handle de evento (`KIND_EVENT` = 8; `READ\|SIGNAL\|TRANSFER\|DUPLICATE`) | `NoMemory` |
| 45 | `event_signal` | handle de evento (`SIGNAL`) → acorda quem espera: todos no modo manual, um no automático | `BadHandle`, `Denied` (sem `SIGNAL`), `InvalidArgs` (não é evento) |
| 46 | `event_reset` | handle de evento (`SIGNAL`) → apaga o sinal (modo manual; no automático o consumo já apaga) | `BadHandle`, `Denied` (sem `SIGNAL`), `InvalidArgs` |
| 43 | `process_list` | ptr, capacidade (1..=256 em `ProcInfo` de 64 B), handle de depuração → quantos processos vivos couberam ({pid, cpu_ns, syscalls, handles, threads, nome}) | `Denied` (sem a capability), `InvalidArgs`, `BadAddress` |
| 27 | `irq_channel` | dev (`SIGNAL`), vetor (de um `irq_alloc` da mesma concessão) → handle de canal (`READ`): 1 byte por disparo, coalescido se já houver aviso na fila; combina com `channel_wait_any` | `BadHandle`, `Denied` (sem `SIGNAL` ou vetor de outra concessão), `InvalidArgs`, `NoMemory` |
| 26 | `channel_wait_any` | ptr (array de handles u32 — canais **e eventos**; o nome ficou por a ABI ser aditiva, mas desde o bloco 122 é a espera múltipla geral), n (1..=16) → índice do primeiro canal com mensagem ou par fechado, ou do primeiro evento sinalizado (num evento automático a espera **consome** o sinal) (bloqueia; acordado pelo `send`/fecho do par, com tique de cobertura de 10 ms) | `BadHandle`, `Denied` (sem `READ`), `InvalidArgs` (não-canal, n fora da faixa), `BadAddress` |
| 24 | `device_open` | dev (raiz, `ADMIN`), bdf | handle de concessão restrita à função `bdf` com `RIGHTS_DEVICE_DEFAULT` (sem `ADMIN`) | `BadHandle`, `Denied` (sem `ADMIN` ou `bdf` fora do escopo), `NotFound` (função não enumerada), `NoMemory` (tabela cheia) |

Números desconhecidos devolvem `NotSupported` (3) sem efeitos. `channel_recv` bloqueia a thread até haver mensagem ou o par fechar.

## 3. Status

`Ok`=0, `InvalidArgs`=1, `BadAddress`=2, `NotSupported`=3, `NoMemory`=4, `NotFound`=5, `Denied`=6, `PeerClosed`=7, `BadHandle`=8, `WouldBlock`=9 (`channel_try_recv`), `TooBig`=10, `QueueFull`=11.

## 3.1 Handles e direitos (ADR-0004)

- Handle = índice `u32` na tabela do processo (até 256); opaco e não forjável (o kernel valida índice, presença e direitos em toda syscall).
- Direitos: `READ`=1, `WRITE`=2, `TRANSFER`=4, `DUPLICATE`=8, `SIGNAL`=16, `MAP`=32, `ADMIN`=64. Só diminuem: `handle_duplicate` aceita apenas subconjuntos.
- Objetos v0: extremidade de canal (`kind` 1), criada com `READ|WRITE|TRANSFER|DUPLICATE`; processo (`kind` 2), criado por `process_spawn` com `READ|TRANSFER|DUPLICATE` (`READ` = esperar/consultar). Os handles iniciais passados no spawn ocupam os índices 0.. na tabela do filho.
- Handles enviados em uma mensagem saem da tabela do remetente e entram na do destinatário (índices novos) no `recv`; exigem `TRANSFER`.
- Ao terminar, o processo fecha todos os handles; a última extremidade fechada de um canal libera o objeto; o par vê `PeerClosed`.
- Mensagens pendentes podem carregar pontas de canal; se nenhum processo vivo alcança uma ponta (nem diretamente, nem por mensagens que ele poderia receber), o kernel a fecha ao término de um processo (coletor de ciclos). Fechar uma ponta descarta as mensagens que ela ainda não recebeu.

## 3.1.1 Concessões de dispositivo (`kind` 3, ADR-0015)

Um handle de dispositivo autoriza syscalls 17–24. Há dois escopos: a concessão **raiz** (todas as funções PCI; direitos `RIGHTS_DEVICE_ALL` = padrão + `ADMIN`), criada pelo kernel para o gerenciador de dispositivos (`devmgr`), e concessões **por função** (`device_open`, escopo = um BDF; direitos `RIGHTS_DEVICE_DEFAULT` = `READ|WRITE|MAP|SIGNAL|TRANSFER|DUPLICATE`), que o `devmgr` entrega a cada driver. Com escopo restrito, `pci_enum` devolve só a função, `pci_cfg_*` recusa outros BDFs (`Denied`) e `mmio_map` só aceita faixas dentro dos BARs dessa função. Restrições: `mmio_map` só aceita faixas dentro de BARs MMIO enumerados; DMA é uma página física por chamada, zerada, pertencente ao processo (liberada com ele) — **sem IOMMU**, o dispositivo pode escrever em qualquer endereço físico que o driver lhe indicar (caminho inseguro documentado no ADR-0015); vetores de IRQ são devolvidos ao pool quando a concessão é destruída. Estruturas `repr(C)` em `abi/syscall`: `PciInfo` (168 B: BDF, IDs, classe, IRQ legada, 6 `PciBar` com base/tamanho/flags), `DmaBuffer` (virt, phys, len), `IrqInfo` (vetor, endereço e dados MSI).

## 3.2 Canais (ADR-0005)

Mensagem = até 4096 bytes + até 8 handles; fila de 64 por extremidade. Sem cabeçalho/protocolo tipado ainda (IDL e versionamento de protocolo vêm no próximo bloco). O kernel copia os bytes para memória própria no `send` e para o usuário no `recv`.

## 3.3 Limites de recursos (todos por processo, salvo indicação)

Cada limite existe porque o recurso por trás dele é **global**: quadros físicos, memória de
kernel, vetores de interrupção. Sem teto, um processo sem privilégio não gasta a sua cota — gasta
a da máquina. Exceder um limite é sempre uma **recusa limpa** (um `Status`), nunca uma falha do
sistema.

| Constante | Valor | Escopo | Ao exceder | Exigido por |
| --- | --- | --- | --- | --- |
| `HANDLES_MAX` | 256 | tabela do processo | `NoMemory` | `user_handle_limit` |
| `THREADS_MAX_PER_PROCESS` | 64 | threads **vivas** do processo (a principal conta) | `NoMemory` | `user_thread_limit` |
| `SHM_PAGES_MAX_PER_PROCESS` | 4096 (16 MiB) | páginas de memória partilhável **criadas** pelo processo | `NoMemory` | `user_shmem` |
| `MEMORY_MAX_PAGES` | 256 (1 MiB) | páginas por objeto de memória | `InvalidArgs` | `user_shmem` |
| `SPAWN_MEM_MAX` | 2 MiB | ELF passado a `process_spawn_mem` | `TooBig` | `user_spawn_mem` |
| `MSG_MAX` | 4096 | bytes por mensagem | `TooBig` | `user_ipc` |
| `MSG_HANDLES_MAX` | 8 | handles por mensagem | `TooBig` | `user_ipc` |
| `CHANNEL_QUEUE_MAX` | 64 | mensagens em fila por ponta | `NoMemory` | `user_ipc` |
| `WAIT_ANY_MAX` | 16 | handles numa espera múltipla | `InvalidArgs` | `user_events` |
| `LOG_MAX` | 1024 | bytes por `log` | `TooBig` | `user_syscall_error` |
| quota de CPU do job | definida por `job_set_cpu_limit` | job | throttle no escalonador | `user_cpu_quota` |

A vaga volta quando o recurso é devolvido: fechar um handle abre uma vaga na tabela, uma thread
que termina devolve a sua (e os 256 KiB de pilha), e o objeto de memória devolve a cota ao morrer.

**Sem teto ainda**: número de processos vivos e número de canais — ambos limitados hoje só pela
memória disponível.

## 4. Validação de ponteiros

Todo ponteiro de usuário é validado antes do acesso: faixa `[ptr, ptr+len)` abaixo de `0x0000_8000_0000_0000` e cada página mapeada com o bit `USER` no espaço do processo; caso contrário `BadAddress`. O kernel copia os bytes para memória própria antes de usá-los.

## 5. Processos nesta versão

- Espaço de endereçamento por processo (PML4 própria; metade do kernel compartilhada), carregado de um ELF64 estático (`ET_EXEC`, endereços do arquivo) **ou PIE** (`ET_DYN`: base aleatória numa janela de 1 TiB acima de 256 MiB, relocações `R_X86_64_RELATIVE` da tabela `DT_RELA` aplicadas pelo kernel; outros tipos são recusados) com segmentos W^X; pilha de 256 KiB com topo **aleatório** (ASLR: alinhado a página, numa janela de 1 GiB abaixo de `0x0000_7fff_fff0_0000`; chega em `RSP`) e região de mapeamentos (`memory_map`/MMIO/DMA) começando num deslocamento aleatório de até 4 GiB acima de `USER_DEVICE_REGION`; o código do ELF é fixo (sem PIE).
- Uma thread por processo; `RDI` na entrada carrega um argumento inteiro.
- Falha em modo usuário (`#PF`, `#GP`, `#UD`…) encerra apenas o processo com código `-1` e motivo registrado no log; o kernel continua.
- Handles com direitos, canais com transferência de handles e processos como objetos (spawn por nome do initrd, wait, info) existem (§3.1–3.2, syscalls 14–16); espera múltipla de canais (`channel_wait_any`, 26; com prazo, `channel_wait_any_timeout`, 34 — o timer de usuário simples) e memória compartilhada (`memory_create`/`memory_map`/`memory_unmap`, 28/29/30) existem; **objetos de evento** genéricos existem (`event_create`/`event_signal`/`event_reset`, 44–46; manuais e automáticos, esperados na mesma `channel_wait_any` dos canais) e são o primeiro objeto a exercer o direito SINALIZAR; jobs existem (36–38). Falta o isolamento por domínios.
- Programas: o initrd (`kernel/lib/initrd`, formato `NEXOIRD1`, gerado por `tools/mkinitrd.py`) contém `init`, `svcmgr`, `echo`, `echo-client`, `utest`, `blockdev` (driver VirtIO-block em modo usuário) e `fs` (servidor NexoFS v0, ADR-0016).
- Pilha de usuário: 256 KiB (era 64 KiB; serviços com buffers de bloco na pilha estouravam). `init` inicia `svcmgr`; `svcmgr` supervisiona `echo` (reinício até 3 vezes) e atende pedidos de conexão de `echo-client` entregando um canal por pedido.

> Endurecimento de 2026-09-01 (mesmo dia da introdução da 33, ABI experimental): ligar e ler o trace passaram a exigir a capability de **depuração** (`KIND_DEBUG` = 5) — o anel é global e, sem o gate, qualquer app veria o padrão de syscalls dos outros (threat model §9).

> Quota (2026-09-01): `memory_create` respeita `SHM_PAGES_MAX_PER_PROCESS` (4096 páginas = 16 MiB por processo criador; devolvida quando o objeto morre) e devolve `NoMemory` ao excedê-la.

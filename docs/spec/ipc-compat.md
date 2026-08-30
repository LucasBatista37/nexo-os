# Regras de compatibilidade do IPC (ADR-0005)

Valem para todo protocolo entre processos do Nexo OS, desde os canais crus de hoje até os protocolos tipados gerados por IDL (próximo bloco da Fase 2).

## 1. Camada de transporte (existe hoje)

- Mensagem = até 4096 bytes + até 8 handles; fila de 64 por extremidade; `recv` bloqueante; `PeerClosed` quando o par fecha (`docs/spec/syscall-abi.md` §3.2).
- O transporte é **agnóstico de conteúdo**: nunca interpreta bytes. Compatibilidade de conteúdo é responsabilidade do protocolo.
- Handles transferidos chegam com índices novos; um protocolo nunca embute índices de handle no payload.

## 2. Cabeçalho obrigatório dos protocolos tipados (a partir do gerador de código)

Toda mensagem de um protocolo tipado começa com 24 bytes:

| Offset | Campo | Tipo | Regra |
|---|---|---|---|
| 0 | `magic` | u32 | `0x4E58_4950` (`"NXIP"`) |
| 4 | `protocol_id` | u32 | hash estável do nome do protocolo (FNV-1a de `"nexo.<nome>"`), atribuído pela IDL |
| 8 | `version_major` | u16 | mudanças incompatíveis |
| 10 | `version_minor` | u16 | adições compatíveis |
| 12 | `method_id` | u32 | número do método (nunca reutilizado) |
| 16 | `flags` | u32 | bit 0 = resposta; bit 1 = erro; bit 2 = evento (sem resposta); demais reservados (= 0) |
| 20 | `payload_len` | u32 | bytes após o cabeçalho |

Decodificadores rejeitam (sem efeitos) magic, `protocol_id`, `version_major` ou `method_id` desconhecidos, `payload_len` inconsistente com o tamanho recebido e flags reservadas ≠ 0.

## 3. Evolução

1. Campos de payload só são **acrescentados ao final**; nunca removidos, reordenados ou reinterpretados. Campos novos têm valor padrão explícito; leitores antigos ignoram bytes extras e leitores novos assumem o padrão quando o payload é curto.
2. Adição de campo ou método → `version_minor += 1`. Mudança de significado, remoção ou troca de tipo → `version_major += 1` e **novo `protocol_id`** (o protocolo antigo continua a existir até ser descontinuado).
3. Um serviço atende a versão maior atual e a anterior por pelo menos um ciclo de release (`RELEASE.md`); a descontinuação é anunciada nas notas de release com a versão em que deixa de ser aceita.
4. Números de método são estáveis para sempre; um método removido tem seu número reservado.
5. Enumerações reservam valor `0 = desconhecido`; leitores tratam valores fora da lista como desconhecido, nunca como erro fatal.
6. Dados grandes (> 4096 bytes) viajam por `MemoryObject` compartilhado + descritor no payload (objeto ainda não implementado); nenhum protocolo fragmenta mensagens manualmente.

## 4. Testes exigidos por protocolo

- Testes de layout (offsets e tamanhos de cada versão) no crate gerado.
- Decodificador submetido ao fuzz-lite (mutação determinística) e, quando houver, ao fuzzing contínuo.
- Teste de compatibilidade cruzada: mensagem da versão N-1 lida pela N e vice-versa (campos extras ignorados, padrões aplicados).

## 5. Protocolos tipados e crus

A IDL existe: `idl/*.idl` → `tools/idlgen` (`make idl`) → `abi/proto` (`nexo-proto`, cabeçalho NXIP do §2, structs com encode/decode, enum de pedidos, erros remotos tipados; testes de layout, ida-e-volta, compatibilidade N±1 e fuzz-lite no crate). O CI falha se os módulos gerados estiverem defasados (`make idl-check`). **Migrados:** `nexo.rng` v1.0 (`rngdev` + clientes) e `nexo.block` v1.0 (`blockdev` + `fs`, `espfs`, `devmgr`, `utest`; `idl/block.idl`: `read`/`write`/`capacity`/`identity`). Os demais protocolos abaixo continuam crus e serão migrados um a um:

| Canal | Mensagens | Observação |
|---|---|---|
| `echo-client` → `svcmgr` | `connect` · resposta `ok`+handle ou `retry` | substituído por protocolo tipado `nexo.svcmgr` |
| `svcmgr` → `echo` | `serve`+handle | idem `nexo.service` |
| cliente → `echo` | texto livre · resposta `echo: <texto>` | exemplo; não faz parte da ABI |
| cliente → `blockdev` | **tipado**: `nexo.block` v1.0 (`idl/block.idl`; métodos `read{sector,count}`→`{data}`, `write{sector,count,data}`, `capacity{}`→`{sectors}`, `identity{}`→`{read_only,serial}`); erros remotos 1 pedido inválido, 2 fora da capacidade, 3 dados insuficientes, 4 somente leitura, `0x10\|st` erro VirtIO | um canal por cliente (handle 1 do driver) |
| cliente → `rngdev` | **tipado**: `nexo.rng` v1.0 (`idl/rng.idl`, método 1 `fill{len}` → `{data}`); erros remotos 1 = pedido inválido, 2 = dispositivo não respondeu, 3 = malformado | um canal por cliente (handle 1 do driver) |
| cliente → `espfs` (`nexo.esp` v0) | pedido `[op u8][pad 3][offset u64][len u32][caminho]`; ops 0 list → `[attr u8][size u32][len u8][nome]…` (valor = nº de entradas), 1 stat → `[attr u8][size u32]` (valor = tamanho), 2 read(offset, len ≤ 4084) → dados · resposta `[status u8][pad 3][valor u64][dados]`; status 1 E/S, 2 corrompido, 3 não encontrado, 5 não é diretório, 6 é diretório, 12 sem ESP | somente leitura; um cliente por servidor |
| cliente → `vfs` (`nexo.fs` v0 roteado) | mesmo protocolo `nexo.fs`; caminhos roteados por prefixo do namespace da instância: `/disk` → `fs`, `/boot` → `espfs` (só leitura; escrita → status 13), `/tmp` → ramfs interno (16 arquivos × 16 KiB, volátil, por instância); inodes carregam a montagem nos bits 28..30 | um `vfs` por cliente = namespace por processo; handles: 0 `fs`, 1 `espfs`, 2 cliente; argumento = máscara de montagens |
| cliente → `consoledev` (`nexo.console` v0) | pedido `[op u8][dados…]`; op 0 = ler o que houver (resposta `[0][bytes…]`, possivelmente vazia, sem bloquear), 1 = escrever (resposta `[0]`) | porta 0 da VirtIO-console, sem `MULTIPORT`; um cliente por driver |
| cliente → `inputdev` (`nexo.input` v0) | pedido `[0]` = ler eventos disponíveis · resposta `[0][evento 8 B]…` (vazia se não houver, sem bloquear); evento = `[type u16][code u16][value u32]` (formato `virtio_input_event`/evdev) | um cliente por driver |
| `devmgr` → cliente | `fs`+handle (canal do servidor de arquivos), `rng`+handle (canal do `rngdev`), `esp`+handle (canal do `espfs`), `done` | canal do handle 1 do `devmgr`; drivers recebem `[concessão, canal]` como handles iniciais |
| cliente → `fs` (`nexo.fs` v0) | pedido `[op u8][pad 3][ino u32][offset u64][len u32][caminho ou dados]`; ops 0 stat(caminho) → `[kind u8][size u64]`, 1 create, 2 mkdir, 3 unlink, 4 read(ino, offset, len ≤ 4084), 5 write(ino, offset, dados), 6 list(caminho) → `[ino u32][kind u8][len u8][nome]…`, 7 sync, 8 info → `[total u64][livres u64][reparos u64][geração u64]`, 9 truncate(ino, offset) · resposta `[status u8][pad 3][valor u64][dados]`; status 0 ok, 1–11 = `nexofs::FsError::code()`, 255 pedido malformado | um cliente por servidor (handle 1); ADR-0016 |

Esses formatos são explicitamente **não estáveis** e existem só para o bring-up.

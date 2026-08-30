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

## 5. Protocolos crus provisórios (até a IDL existir)

| Canal | Mensagens | Observação |
|---|---|---|
| `echo-client` → `svcmgr` | `connect` · resposta `ok`+handle ou `retry` | substituído por protocolo tipado `nexo.svcmgr` |
| `svcmgr` → `echo` | `serve`+handle | idem `nexo.service` |
| cliente → `echo` | texto livre · resposta `echo: <texto>` | exemplo; não faz parte da ABI |
| cliente → `blockdev` (`nexo.block` v0) | pedido `[op u8][pad 3][setor u64][n u32][dados n×512 se escrita]`, `op` 0 = ler, 1 = escrever, `n` ≤ 7 · resposta `[status u8][dados n×512 se leitura]`; status 0 ok, 1 pedido curto, 2 fora da capacidade/`n` inválido, 3 dados insuficientes, `0x1x` erro do dispositivo | um canal por cliente (handle 1 do driver); substituído por `nexo.block` tipado |

Esses formatos são explicitamente **não estáveis** e existem só para o bring-up.

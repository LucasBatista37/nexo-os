# ADR-0005 — Formato e versionamento de IPC

- **Status:** aceita (direção); esquema binário final via RFC na Fase 2
- **Data:** 2026-08-29
- **Relacionados:** ADR-0004, ADR-0006

## Contexto

Serviços (VFS, rede, compositor…) conversam por `Channel`. Sem um formato tipado e versionado desde o início, cada serviço inventaria seu protocolo e a compatibilidade (§2.2 item 3) seria impossível de garantir.

## Decisão

1. Mensagens IPC = **cabeçalho fixo** (`magic`, `protocol_id`, `protocol_version`, `method_id`, `flags`, `payload_len`, `handle_count`) + **payload** com layout definido em uma IDL própria (`.nexo-idl`) e gerada para Rust (e C via ABI).
2. Regras de compatibilidade: campos só são adicionados ao final; `protocol_version` menor incrementa em adições, maior em quebras; um serviço deve atender pelo menos a versão maior anterior durante um ciclo de release.
3. Handles viajam fora do payload (tabela do cabeçalho), nunca como inteiros dentro dos dados.
4. **Todo** decodificador valida tamanhos, alinhamento, contagem de handles e versões antes de tocar o payload; decodificadores são alvo obrigatório de fuzzing.
5. Dados grandes vão por `MemoryObject` compartilhado + descritor no payload, nunca por cópia em mensagem (limite de mensagem: 64 KiB).
6. Nesta release não há IPC; o `BootInfo` (`abi/boot`) segue a mesma filosofia (magic, versão, tamanho, validação) e serve de modelo.

## Consequências

- Gerador de código e IDL entram no roadmap da Fase 2 (§5 Fase 2: "formato de protocolo tipado e gerador de código").
- Serviços não podem usar `serde`/formatos dinâmicos no caminho principal.

## Alternativas consideradas

- **Protobuf/Cap'n Proto/FIDL** existentes: rejeitados como base — dependências grandes e licença/manutenção fora do controle; a FIDL (Fuchsia) é referência de desenho.
- **Structs C cruas sem cabeçalho**: rejeitado — impossível versionar.

## Evidência / verificação

Testes de compatibilidade entre versões no CI (Fase 2); `abi/boot` já testa layout e validação.

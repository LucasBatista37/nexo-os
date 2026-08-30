# ADR-0006 — ABI nativa e política de estabilidade

- **Status:** aceita
- **Data:** 2026-08-29
- **Relacionados:** ADR-0004, ADR-0005, ADR-0014, `COMPATIBILITY.md`

## Decisão

1. A ABI pública é **C estável e versionada** (`abi/`): syscalls, tipos de handle, códigos de erro, cabeçalhos IPC e formatos de arquivo públicos. O SDK Rust é uma camada segura sobre ela.
2. Convenção de chamada: System V AMD64 em x86_64; `#[repr(C)]` em tudo que cruza fronteira.
3. Antes de `1.0` **nenhuma** promessa de estabilidade: cada release pode quebrar a ABI com nota de mudança. A partir de `0.9-beta` a ABI é candidata e só muda com RFC. A `1.x` é estável dentro de um contrato publicado.
4. Toda estrutura pública tem teste de layout (`size_of`/`align_of`, como em `abi/boot`) e número de versão.
5. Executáveis: ELF64 (`x86_64-unknown-none`-compatível) até que um formato próprio seja justificado por ADR.

## Consequências

Testes de layout obrigatórios; documentação por versão em `COMPATIBILITY.md`; o loader e o kernel já seguem a regra para a ABI de boot.

## Alternativas

ABI Rust direta (rejeitada: não estável entre versões do compilador); ABI POSIX como nativa (rejeitada: ADR-0014).

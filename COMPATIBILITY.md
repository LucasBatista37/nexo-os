# Compatibilidade — ABI, API e formatos

**Regra geral (ADR-0006):** antes de `1.0` nada é estável; mudanças são documentadas por release. Abaixo, o que existe hoje e seu status.

| Contrato | Versão | Estabilidade | Onde |
|---|---|---|---|
| ABI de boot (`BootInfo`) | 1 | interna ao repositório (loader e kernel são construídos juntos); incompatibilidades detectadas por `validate()` | `docs/spec/boot-abi.md`, `abi/boot` |
| Formato do kernel | ELF64 `ET_EXEC` x86_64 | interna | ADR-0003 |
| Imagem de disco | GPT + ESP FAT32 (`tools/mkgpt.py`) | interna | `tools/build-image` |
| Linha de comando do kernel | chaves `loglevel`, `selftest`, `test`, `exit` | pode mudar a qualquer momento | `docs/spec/boot-abi.md` §7 |
| Protocolo serial de testes | `[TEST]/[RESULT]/[MEMORY]/[HEAP]/[TIME]` | usado pelo CI; mudar exige atualizar `tools/test-qemu` | `docs/testing.md` |
| Syscalls / IPC / ABI C / SDK / pacotes | — | **não existem ainda** (Fases 2 e 6) | ADR-0004/0005/0006/0009 |

Registro de mudanças incompatíveis: seção "Compatibilidade" em cada `docs/releases/<versão>.md`.

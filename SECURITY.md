# Segurança — Nexo OS

## Contato e divulgação

- Relate vulnerabilidades por uma *security advisory* privada em https://github.com/LucasBatista37/nexo-os/security/advisories/new (não abra issue pública). Um endereço `security@` será publicado quando houver domínio.
- Prazo de resposta inicial: 7 dias. Divulgação coordenada: até 90 dias após a confirmação, ou antes se houver correção publicada.
- Enquanto o projeto está antes de `0.9-beta`, não há canal de atualização; correções entram na próxima release.

## Política

- Toda correção de segurança gera nota em `docs/releases/` e, quando aplicável, teste de regressão.
- Alterações em `unsafe`, syscalls, decodificadores e loader exigem revisão com foco em segurança (ver `CONTRIBUTING.md`).
- Nenhum crash é "não reproduzível" sem log serial e símbolos anexados.

## Threat model v0 (release `0.0.1-boot`)

Escopo atual: loader UEFI + kernel em ring 0 sem modo usuário, executando em QEMU. Não há rede, armazenamento gravável nem código de terceiros em execução; portanto o modelo cobre **o que já existe** e **o que está sendo preparado**.

### Ativos

- Integridade do kernel e das tabelas de página.
- Integridade da pilha do kernel e do heap.
- Saída de diagnóstico (serial) confiável — é a base do CI.

### Adversários e vetores considerados

| Vetor | Estado | Mitigação existente | Próximos passos |
|---|---|---|---|
| Kernel/loader malformado (ELF, segmentos sobrepostos, W+X) | coberto | loader valida cabeçalho, arquitetura, `ET_EXEC`, rejeita W+X e sobreposição; `BootInfo` validado por magic/versão/tamanho | assinatura do kernel (ADR-0010) |
| Corrupção de memória em `unsafe` | parcial | `unsafe_op_in_unsafe_fn=deny`, `SAFETY` obrigatório, crates puros `deny(unsafe_code)`, testes de host | inventário auditado por release, fuzzing (Fase 2) |
| Execução de dados (heap/pilha/physmap) | coberto | NX em todas as páginas não-código; teste `nx` prova a falha | — |
| Escrita em código/rodata | coberto | `.text`/`.rodata` sem WRITABLE + CR0.WP; teste `write_protect` | — |
| Estouro de pilha | coberto | guard page abaixo da pilha; `#DF` em IST1 com diagnóstico; cenário `overflow` | pilhas por thread com guard (Fase 1) |
| Acesso a endereço inválido | coberto | `#PF` fatal com CR2, RIP simbolizado e backtrace; sondas testadas | — |
| Falha silenciosa | coberto | panic com contexto; CI reconhece `[RESULT]`, `KERNEL PANIC`, `EXCEPTION` | crash dumps (Fase 8) |
| Memória do firmware reutilizada indevidamente | coberto | runtime services/ACPI NVS/MMIO reservados; primeiro MiB reservado; frame 0 nunca alocado | IOMMU (Fase 3) |
| Reprodutibilidade/adulteração de build | parcial | toolchain fixada, imagem determinística (`make reproducible`) | vendorização, SBOM, assinatura |
| Driver comprometido, rede/mídia malformada, rollback, acesso físico, perda de energia | fora do escopo desta release | — | threat models por subsistema a partir da Fase 2 |

### Suposições

- O firmware UEFI e o QEMU são confiáveis nesta fase.
- Não há usuários nem código não confiável em execução.
- O canal serial é controlado pelo operador.

### Superfície `unsafe`

Ver [docs/unsafe-inventory.md](docs/unsafe-inventory.md).

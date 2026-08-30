# ADR-0010 — Atualizações A/B e trust root

- **Status:** aceita (direção)
- **Data:** 2026-08-29
- **Relacionados:** ADR-0009, `RECOVERY.md`, `RELEASE.md`

## Decisão

1. O sistema base é distribuído como **imagens A/B** atômicas e assinadas; o bootloader escolhe o slot ativo e faz *fallback* automático após falha de health check pós-boot.
2. Metadados de atualização seguem o modelo **TUF** (papéis root/targets/snapshot/timestamp, thresholds, expiração) — resistentes a rollback e a comprometimento de chave online.
3. A **raiz de confiança é offline** (cerimônia documentada); chaves online assinam apenas snapshot/timestamp.
4. Ambiente de recuperação independente do sistema instalado.
5. Nesta release não há atualizador; o loader já valida o kernel estruturalmente (ELF, W^X) e a imagem é reproduzível — pré-requisitos para assinatura.

## Alternativas

Atualização por pacotes in-place (rejeitada como base: sem atomicidade); chave única online (rejeitada: §11).

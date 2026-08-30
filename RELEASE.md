# Releases — versionamento e canais

## Versionamento

- Marcos do Plano Mestre §7: `0.0.1-boot`, `0.1-kernel`, `0.2-userspace`, `0.3-storage`, `0.4-network`, `0.5-desktop`, `0.6-sdk`, `0.7-hardware-alpha`, `0.8-distributable-alpha`, `0.9-beta`, `1.0`.
- Tags git `v<versão>` assinadas (GPG/SSH) a partir de `0.2`; antes disso, tags anotadas.
- Crates internas seguem `0.0.x` até `0.6-sdk`, quando o SDK passa a ter versão própria.

## Regra de avanço

Uma versão só é publicada quando o gate técnico da anterior está atendido (§7.1). Evidência obrigatória em `docs/releases/<versão>.md`: comandos, logs do CI, hashes dos artefatos, itens da checklist atendidos e limitações.

## Canais

| Canal | Quando | Conteúdo |
|---|---|---|
| `main` | contínuo | CI verde obrigatório; imagens como artefatos do CI |
| tags `v*` | por marco | imagem `nexo.img`, `kernel.elf`, `kernel.sym`, `BOOTX64.EFI`, `SHA256SUMS`, notas |
| nightly/alpha/beta/stable | a partir de `0.9-beta` (Fase 9) | com assinatura TUF (ADR-0010) |

## Procedimento de release

1. `make ci` limpo em clone novo (macOS e Linux/CI).
2. Atualizar `CHANGELOG.md`, `docs/releases/<versão>.md`, `docs/CHECKLIST_STATUS.md` e §17 do Plano Mestre.
3. `git tag -a v<versão>`; anexar artefatos e `SHA256SUMS` (gerados por `tools/build-image`).
4. Publicar notas com limitações conhecidas e regressões registradas.

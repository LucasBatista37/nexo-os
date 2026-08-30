# ADR-0008 — VFS e namespace

- **Status:** aceita (direção); API assíncrona via RFC na Fase 3
- **Data:** 2026-08-29
- **Relacionados:** ADR-0004, ADR-0009

## Decisão

1. O VFS é um **serviço de usuário** com namespace **por processo/sessão**: um processo só vê o que lhe foi montado ou concedido por handle.
2. Evolução dos formatos: FAT só para a partição EFI; `ramfs/initramfs` no bring-up; leitura de um formato simples para testes; VFS assíncrono próprio; escrita com testes de queda de energia; formato copy-on-write próprio apenas após VFS e testes maduros (§3.9).
3. Arquivos abertos são handles com direitos (ler/escrever/mapear); não existem caminhos ambientais nem "root" implícito.
4. Todo formato persistente terá especificação pública versionada, checksums e ferramenta offline de verificação.

## Alternativas

VFS no kernel (rejeitado: ADR-0002); adotar ext4/btrfs (rejeitado: licença/complexidade; formatos externos só como leitura de compatibilidade).

# ADR-0012 — Licença do projeto e política de dependências

- **Status:** aceita (licença provisória até revisão jurídica)
- **Data:** 2026-08-29
- **Relacionados:** ADR-0001, `LICENSES.md`

## Decisão

1. Licença do código próprio: **MIT OR Apache-2.0** (dupla, padrão do ecossistema Rust: permite adoção por fabricantes e SDK sem atrito; Apache-2.0 traz cláusula de patentes).
2. Dependências permitidas: MIT, Apache-2.0, BSD-2/3, ISC, MPL-2.0 (isolada por arquivo), CC0/Unlicense, Zlib, OFL (fontes). **Proibidas** em kernel/serviços/SDK: GPL/LGPL/AGPL, SSPL, licenças sem redistribuição clara.
3. Toda dependência passa por revisão de licença, segurança e manutenção antes do merge e é registrada em `LICENSES.md` (SBOM manual até haver ferramenta).
4. Kernel: **zero** dependências externas. Loader: `uefi` (MPL-2.0), `uefi-raw`, `uefi-macros`, `log` (MIT/Apache-2.0), `ptr_meta`, `bitflags`, `ucs2` — revisadas nesta release.
5. Vendorização (`third_party/`) obrigatória a partir de `0.2` para builds sem rede.

## Alternativas

GPLv2 (rejeitada: bloqueia SDK/fabricantes proprietários); licença própria (rejeitada: insegurança jurídica).

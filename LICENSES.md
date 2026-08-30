# Licenças

## Código próprio

`MIT OR Apache-2.0` (ADR-0012). Textos: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE). Inclui a fonte bitmap `libraries/font/src/font8x8.txt` (autoral).

## Dependências (inventário da release `0.0.1-boot`)

| Componente | Crate | Versão | Licença | Uso | Revisão |
|---|---|---|---|---|---|
| kernel | — | — | — | **zero dependências externas** | — |
| loader | `uefi` | ver `boot/loader/Cargo.lock` | MPL-2.0 | protocolos UEFI (arquivos, GOP, memory map, exit) | 2026-08-29: mantido ativamente (rust-osdev); candidato a substituição por bindings próprios se necessário |
| loader | `uefi-raw`, `uefi-macros` | idem | MPL-2.0 | tipos crus/macros do `uefi` | idem |
| loader | `log` | idem | MIT OR Apache-2.0 | macros de log no console UEFI | ok |
| loader | `bitflags`, `ptr_meta`, `ucs2`, `cfg-if` (transitivas) | idem | MIT/Apache-2.0 | transitivas de `uefi` | ok |
| ferramentas | Python 3 stdlib | 3.9+ | PSF | build/QEMU/GPT | ok |
| firmware de teste | edk2 (OVMF) | do pacote QEMU | BSD-2-Clause-Patent | apenas em execução no QEMU; não redistribuído | ok |
| toolchain | Rust 1.98.0 | fixada | MIT/Apache-2.0 | compilação | ok |

Atualize esta tabela em todo PR que altere `Cargo.lock`. A partir de `0.2`, o `third_party/` vendoriza as dependências com seus textos de licença.

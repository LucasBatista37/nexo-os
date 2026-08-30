# Toolchain fixada — release `0.0.1-boot`

Verifique com `tools/check-toolchain`. Atualizações de versão só por PR dedicado com CI verde e nota em `docs/releases/`.

| Ferramenta | Versão fixada | Origem | Papel |
|---|---|---|---|
| Rust | `1.98.0` (2026-08-18), estável | `rust-toolchain.toml` (rustup) | loader, kernel, libs, testes |
| targets | `x86_64-unknown-none`, `x86_64-unknown-uefi` | rustup | kernel / loader |
| componentes | `rustfmt`, `clippy`, `llvm-tools` (sem `rust-src`: ver nota) | rustup | lint, `llvm-nm`, `llvm-objcopy`, `llvm-symbolizer` |
| linker | `rust-lld` (do toolchain) | rustup | ELF do kernel (script `kernel/linker.ld`) e PE do loader |
| QEMU | 11.1.1 (macOS/brew); ≥ 8.2 no CI (Ubuntu 24.04) | brew / apt | `q35`, `-cpu qemu64`, 512 MiB |
| firmware UEFI | edk2 `edk2-x86_64-code.fd` do pacote QEMU; `OVMF` no CI | brew / apt `ovmf` | boot UEFI |
| mtools | 4.0.49 | brew / apt | `mformat/mmd/mcopy` (ESP FAT32) |
| Python | 3.9+ (sem dependências) | sistema | `tools/` |
| depurador | `lldb` (Xcode) ou `gdb` | sistema | `tools/run-qemu --gdb` |
| git | 2.50 | sistema | — |

## Setup em uma máquina limpa

```sh
# macOS (Apple Silicon ou Intel)
brew install qemu mtools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
git clone https://github.com/LucasBatista37/nexo-os.git nexo-os && cd nexo-os
rustup show            # instala 1.98.0 + targets + componentes de rust-toolchain.toml
tools/check-toolchain
make test              # testes de host + boot em QEMU
```

```sh
# Debian/Ubuntu
sudo apt install qemu-system-x86 ovmf mtools build-essential curl git python3
# … idem acima
```

## Reprodutibilidade e caminhos

- `tools/build-image` compila o loader com `--remap-path-prefix` para `~/.cargo/registry` e para o repositório, e coloca na ESP uma cópia do kernel sem `.debug_*` (`llvm-objcopy --strip-debug`); o ELF completo fica em `build/kernel.elf` para `tools/symbolize`.
- `rust-src` não faz parte da toolchain fixada: quando instalado, o `rustc` reescreve `/rustc/<hash>/library/...` para o caminho local em strings de `panic` de código genérico de `core`/`alloc`, e a imagem passaria a depender do usuário/máquina. Instale-o só para o IDE (`rustup component add rust-src`) e não confie no hash da imagem nesse caso.
- `make reproducible` compara duas builds no mesmo checkout; o hash oficial de cada release é registrado em `docs/releases/`.
- Limite conhecido: o `cargo` deriva o hash de metadados dos símbolos (`_RNvCs<hash>_…`) do caminho dos pacotes; como a cópia do kernel na ESP mantém `.symtab`, checkouts em diretórios diferentes geram imagens diferentes (cada uma determinística). Compare hashes apenas entre builds do mesmo caminho (o CI usa caminho fixo).

## Política de atualização controlada

1. Abrir PR alterando `rust-toolchain.toml`/`Cargo.lock`/versões desta tabela.
2. CI precisa passar em `make ci` (inclui `make reproducible`).
3. Registrar em `docs/releases/<versão>.md` a mudança e qualquer diferença de comportamento.
4. Nunca usar `nightly` (ADR-0001).

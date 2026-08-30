# ci/

Pipeline: `.github/workflows/ci.yml` (Ubuntu 24.04: rustup fixado, QEMU + OVMF + mtools, `make lint`, `cargo test`, `tools/build-image`, `tools/test-qemu`, `make reproducible`, artefatos).

Execução local equivalente: `make ci`. Ambiente reproduzível: `docs/toolchain.md`.

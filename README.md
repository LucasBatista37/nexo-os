# Nexo OS

Sistema operacional próprio, construído do zero em Rust estável — kernel, loader UEFI, bibliotecas e ferramentas — seguindo o [Plano Mestre](PLANO_MESTRE_SISTEMA_OPERACIONAL.md).

**Release atual:** `0.0.1-boot` (Fase 0 + primeiros 90 dias concluídos); `main` já contém o grosso da Fase 1 (`0.1-kernel`): ACPI/APIC, SMP com 4 CPUs, escalonador preemptivo e modo de stress. Estado detalhado: [docs/CHECKLIST_STATUS.md](docs/CHECKLIST_STATUS.md). CI: ![ci](https://github.com/LucasBatista37/nexo-os/actions/workflows/ci.yml/badge.svg)

## O que já funciona

- Loader UEFI próprio (`boot/loader`): carrega o kernel ELF, constrói tabelas de página (physmap NX, kernel W^X, pilha com guard page), entrega `BootInfo` versionado ([spec](docs/spec/boot-abi.md)).
- Kernel x86_64 (`kernel/`): logger serial, GDT/TSS/IDT com 256 stubs, panic com backtrace simbolizado (demangler v0 próprio), alocador de quadros por bitmap, paginação (map/unmap/flags), heap com crescimento sob demanda, PIT a 1000 Hz, tarefas cooperativas, console de framebuffer com fonte própria.
- ACPI (MADT/HPET), LAPIC + I/O APIC, TSC como relógio monotônico, SMP (INIT/SIPI, dados por CPU), threads de kernel preemptivas (`spawn/sleep/join`, afinidade), timers de kernel, stress multi-CPU.
- **Modo usuário**: ABI de syscalls v0 (`syscall`/`sysret`), processos com espaço de endereçamento próprio carregados de ELF, `services/init` rodando em ring 3, falhas de usuário isoladas do kernel, handles com direitos e canais IPC com transferência de handles entre processos.
- 31 auto-testes executados no boot e verificados por serial no CI; cenários de falha (panic, page fault, estouro de pilha) com diagnóstico útil; cenário de stress.
- Imagem GPT+ESP reproduzível gerada por um comando; toolchain fixada.

## Começando

```sh
brew install qemu mtools            # macOS  (Debian: apt install qemu-system-x86 ovmf mtools)
curl https://sh.rustup.rs -sSf | sh # rustup; a toolchain vem de rust-toolchain.toml
git clone https://github.com/LucasBatista37/nexo-os.git && cd nexo-os
rustup toolchain install && rustup show
tools/check-toolchain
make image   # build/nexo.img
make run     # QEMU com display; serial no terminal
make test    # cargo test + 4 cenários em QEMU headless
```

Mais: [CONTRIBUTING.md](CONTRIBUTING.md), [docs/toolchain.md](docs/toolchain.md), [docs/testing.md](docs/testing.md).

## Estrutura

```
abi/boot        contrato loader→kernel        boot/loader   aplicação UEFI
arch/x86_64     paginação, GDT/IDT, traps     kernel/       kernel + lib/{mm,heap,sync,symbols}
libraries/font  fonte bitmap própria          tools/        build-image, run-qemu, test-qemu, symbolize
docs/           ADRs, RFCs, specs, progresso  ci/ .github/  pipeline
```

Diretórios `services/`, `drivers/`, `compositor/`, `shell/`, `apps/`, `sdk/`, `third_party/` estão reservados para as fases seguintes (ver `ARCHITECTURE.md`).

## Documentos

[PROJECT_CHARTER](PROJECT_CHARTER.md) · [ARCHITECTURE](ARCHITECTURE.md) · [SECURITY](SECURITY.md) · [ADRs](docs/adr/README.md) · [COMPATIBILITY](COMPATIBILITY.md) · [RELEASE](RELEASE.md) · [RECOVERY](RECOVERY.md) · [SUPPORTED_HARDWARE](SUPPORTED_HARDWARE.md) · [LICENSES](LICENSES.md)

Licença: MIT OR Apache-2.0.

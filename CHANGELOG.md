# Changelog

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/). Versões seguem os marcos do Plano Mestre §7.

## [0.0.1-boot] — 2026-08-29

Primeira release: fundação reproduzível, observável e testada (Fase 0 + Plano dos 90 dias).

### Adicionado
- Loader UEFI `nexo-loader` (x86_64-unknown-uefi): leitura de `kernel.elf`/`boot.cfg`, GOP, RSDP, physmap de 2 MiB (NX), carga ELF com W^X, pilha com guard page, cópia do ELF para símbolos, `ExitBootServices`, conversão do mapa de memória, `BootInfo` v1.
- Kernel `nexo-kernel` (x86_64-unknown-none, Rust estável): logger serial estruturado, GDT/TSS (IST1 para #DF), IDT com 256 stubs em assembly, handlers de #BP/#PF/#DF/IRQ, sondas de falta esperada, panic com backtrace por frame pointers e symbolication (demangler legado e v0), normalização do mapa de memória, alocador de quadros por bitmap, paginação (map/unmap/update/translate), heap com crescimento sob demanda e guard pages, PIT 1000 Hz, tarefas cooperativas (spawn/yield/exit/reap), console de framebuffer com fonte 8×8 própria.
- 15 auto-testes no boot com protocolo serial `[TEST]/[RESULT]`; cenários `test=panic|fault|overflow`.
- Crates de host com testes: `nexo-boot-abi`, `nexo-mm`, `nexo-heap`, `nexo-sync`, `nexo-symbols`, `nexo-font`, `nexo-arch-x86_64`.
- Ferramentas: `tools/build-image` (GPT + ESP FAT32 determinística), `tools/run-qemu`, `tools/test-qemu`, `tools/symbolize`, `tools/check-toolchain`; `Makefile`; CI GitHub Actions.
- Documentação: charter, arquitetura, segurança/threat model v0, ADR-0001..0014, spec da ABI de boot, toolchain, testes, inventário de `unsafe`, compatibilidade, release, recuperação, hardware suportado.

### Limitações conhecidas
- Apenas QEMU q35/UEFI; sem SMP, APIC, modo usuário, armazenamento ou rede.
- PIC/PIT legados; timer com resolução de 1 ms; sem relógio TSC calibrado.
- Console de framebuffer sem rolagem (volta ao topo).
- Regiões de memória "loader-reclaim" incluem código/dados de boot services que o kernel já pode reutilizar; runtime services não são chamados.

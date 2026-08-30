# Changelog

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/). Versões seguem os marcos do Plano Mestre §7.

## [Unreleased] — rumo a `0.1-kernel` e início da Fase 2

### Adicionado (Fase 2, bloco 1 — modo usuário)
- ABI de syscalls v0 (`abi/syscall`, `docs/spec/syscall-abi.md`): `exit`, `log`, `time_now`, `yield`, `sleep`, `get_pid`, `abi_version`, `debug_info`; status em `RAX`, valor em `RDX`.
- Entrada `syscall`/`sysret` com troca para a pilha de kernel da thread (`gs:[8]`), `swapgs` nas traps vindas de ring 3, GDT com segmentos de usuário (`0x23`/`0x2b`) e TSS.RSP0 por thread.
- Processos (`process.rs`): PML4 própria com a metade do kernel compartilhada, carga de ELF64 estático com W^X e bit `USER` propagado nas tabelas, pilha de usuário com guard page, thread principal em ring 3, `wait`, liberação do espaço ao terminar; troca de CR3 no escalonador.
- Faltas em modo usuário (`#PF`, `#GP`, `#UD`…) encerram só o processo (`EXIT_KILLED`), com log do motivo.
- `sdk/nexo-sys` (invocação de syscalls) e `services/init` (primeiro programa de usuário: exercita a ABI e os cenários de isolamento).
- Loader/ABI de boot v2: `\nexo\init.elf` é carregado como initrd (`initrd_addr/len`, tipo `Initrd`); leitor ELF compartilhado (`kernel/lib/elf`).
- Auto-testes `user_process`, `user_isolation` (leitura do kernel, `cli`, escrita em `.rodata` → processo morto, kernel íntegro, sem vazamento de quadros) e `user_syscall_error`. 30 testes no boot.

### Adicionado (Fase 1)
- `nexo-acpi`: parser de RSDP/XSDT/RSDT/MADT/HPET sem alocação (testes de host).
- LAPIC (xAPIC) com timer calibrado pelo PIT; I/O APIC com overrides ISA (teste roteia o PIT pelo GSI 2); PIC remapeado e mascarado; vetores de IPI (resched, halt, TLB flush) e espúria.
- TSC calibrado como relógio monotônico (`monotonic_ns`, `uptime`, `sleep`, `delay_us`); ticks de 1000 Hz por CPU só para o escalonador.
- SMP: trampolim INIT/SIPI (modo real → longo), dados por CPU via `gs:[0]` (GDT/TSS/#DF próprias), pilhas das APs com guard page, 4/4 CPUs online no QEMU; panic/exceção fatal param as outras CPUs; shootdown de TLB por IPI.
- Escalonador preemptivo de threads de kernel: fila global, quantum de 10 ms, idle por CPU, `spawn/yield/sleep/join/exit/reap`, pilhas em slots com guard page, preempção dentro do handler do timer, IPI para CPU ociosa.
- `IrqLock` (spinlock com interrupções desabilitadas) e regra de locks do kernel.
- Modo de stress `stress=<s>` (lock, atomics, heap, sleep, spawn/join, map/unmap, todas as CPUs) com invariantes; cenário `stress` no CI (15 s) e `make stress DURATION=86400` para o gate F1.
- Timers de kernel (`timer.rs`): callbacks únicos e periódicos por prazo em ns, despachados pela thread `ktimer` fora de contexto de interrupção; cancelamento.
- Afinidade de CPU por thread (`spawn_on`, `spawn_with_affinity`, `set_affinity`); a thread `main` fica presa à BSP.
- 27 auto-testes no boot (novos: acpi, apic_timer, tsc_clock, ioapic, ipi_self, smp, threads_*, timers, threads_affinity); `run-qemu --smp` (padrão 4).

### Alterado
- Tarefas cooperativas substituídas por threads preemptivas (`sched.rs`); `time::sleep_ms` usa o TSC.
- Lint inclui `clippy --target x86_64-unknown-none` para o código `cfg(x86_64)`.

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

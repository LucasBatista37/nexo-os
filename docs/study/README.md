# Currículo paralelo — guia de estudo

O Plano Mestre §9 lista o que precisa ser aprendido. Este guia mapeia cada tópico ao **código deste repositório que o exercita** e a uma referência primária. Marque o progresso pessoal em `docs/progress/`.

## Nível A — antes e durante o primeiro boot

| Tópico | Onde está aplicado aqui | Referência |
|---|---|---|
| binário, hexadecimal, endianness, complemento de dois | `abi/boot` (magic `NEXOBOOT` em little-endian), `nexo-symbols` (leitura ELF) | Patterson & Hennessy, cap. 2 |
| CPU, registradores, pilha, chamadas e ABI | `arch/x86_64/src/{trap,context}.rs`, `docs/spec/boot-abi.md` §2 | System V AMD64 ABI |
| memória virtual, páginas e TLB | `arch/x86_64/src/paging.rs`, `kernel/src/mm/virt.rs` | Intel SDM vol. 3, cap. 4 |
| Rust ownership, lifetimes, atomics, `no_std`, `unsafe` | `nexo-sync`, `nexo-heap`, `docs/unsafe-inventory.md` | Rust Book; Rustonomicon; Embedded Book |
| Assembly x86_64 básico | `trap.rs` (stubs), `context.rs`, `cpu.rs` | Intel SDM vol. 2 |
| linker, seções, símbolos, relocation, ELF | `kernel/linker.ld`, `boot/loader/src/elf.rs`, `nexo-symbols` | ELF-64 spec; LLD docs |
| UEFI e mapa de memória | `boot/loader/src/main.rs`, `ADR-0003` | UEFI 2.11 §7 (Boot Services) |
| GDB/LLDB e disassembly | `tools/run-qemu --gdb`, `tools/symbolize`, `llvm-objdump -d build/kernel.elf` | LLDB tutorial |
| Git, CI, builds reproduzíveis | `.github/workflows/ci.yml`, `make reproducible`, `docs/toolchain.md` | reproducible-builds.org |

## Nível B — kernel e concorrência (Fases 1–2)

| Tópico | Ponto de partida no repositório |
|---|---|
| interrupções, exceções, APIC, temporizadores | `x86/traps.rs`, `time.rs` (PIC/PIT → APIC/HPET) |
| processos, threads, troca de contexto | `task.rs` (cooperativo → preemptivo) |
| schedulers | `task::pick_next` (round-robin → prioridades) |
| locks, atomics, memory ordering | `nexo-sync` (spinlock → IRQ-safe, RwLock, testes SMP) |
| IPC e capabilities | ADR-0004/0005 (RFCs na Fase 2) |
| DMA, MMIO, IOMMU | ADR-0007 |
| property testing e fuzzing | `nexo-heap::tests::randomized_no_overlap` como semente |

## Nível C — sistema completo

Mapeado nas ADRs 0007–0014; sem código ainda.

## Projetos de estudo auxiliares

| Projeto | Estado |
|---|---|
| alocador em user space | ✔ `nexo-heap` é testado no host como biblioteca comum |
| executor de threads simples | ✔ `task.rs` cooperativo |
| filesystem em arquivo de imagem | parcial: `tools/mkgpt.py` (GPT) + mtools (FAT) |
| protocolo RPC tipado entre processos | pendente (Fase 2) |
| renderer 2D por software | parcial: `console.rs` + fonte própria |
| cliente TCP/HTTP educacional | pendente (Fase 4) |
| fuzzar um parser binário próprio | pendente — candidatos: `nexo-symbols`, `boot/loader/src/elf.rs` |
| analisar Redox, seL4, Fuchsia e Linux | referências em ADR-0002/0004; leitura contínua |

# Board — Fase 0 e primeiros 90 dias

Substitui um board externo até o repositório ter issues. Uma linha por item do Plano Mestre; `evidência` aponta para o artefato verificável. Legenda: `[x]` concluído · `[-]` em andamento · `[ ]` não iniciado · `[!]` bloqueado.

## Marco: `0.0.1-boot` — concluído em 2026-08-29

### Semanas 1–2 — Contrato do projeto
| | Item | Evidência |
|---|---|---|
| [x] | nome provisório, público e promessa de 1.0 | `PROJECT_CHARTER.md` |
| [x] | licença | ADR-0012, `LICENSE-*`, `LICENSES.md` |
| [x] | x86_64 + UEFI como plataforma 1 | ADR-0003 |
| [x] | ADR-0001 a ADR-0004 | `docs/adr/` |
| [x] | repositório e estrutura mínima | árvore §4.5 do plano; `git log` |
| [x] | board e milestones | este arquivo |
| [x] | instalar Rust, LLVM/binutils, QEMU, GDB/LLDB, firmware UEFI | `tools/check-toolchain` |
| [x] | versões exatas e comando de setup | `docs/toolchain.md`, `rust-toolchain.toml` |

### Semanas 3–4 — Imagem inicializável
| | Item | Evidência |
|---|---|---|
| [x] | binário UEFI | `boot/loader` → `build/BOOTX64.EFI` |
| [x] | imagem GPT com partição EFI | `tools/mkgpt.py`, `build/nexo.img` |
| [x] | inicializar no QEMU | `tools/test-qemu` cenário `boot` |
| [x] | escrever no console/framebuffer | loader: console UEFI + barra no framebuffer; kernel: `console.rs` |
| [x] | escrever no serial | loader e kernel (`klog.rs`) |
| [x] | comando único `build-image` | `tools/build-image` / `make image` |
| [x] | comando único `run-qemu` | `tools/run-qemu` / `make run` |

### Semanas 5–6 — Kernel e erros
| | Item | Evidência |
|---|---|---|
| [x] | separar loader e kernel | dois workspaces, `docs/spec/boot-abi.md` |
| [x] | transferir mapa de memória e framebuffer | `BootInfo` |
| [x] | entry point de 64 bits | `kernel/src/main.rs::_start` |
| [x] | logger estruturado mínimo | `kernel/src/klog.rs` |
| [x] | panic | `kernel/src/panic.rs`; cenário `panic` |
| [x] | causar e tratar exceção de teste | testes `breakpoint`, `page_fault_recovery`; cenários `fault`/`overflow` |
| [x] | gerar símbolos e localizar endereço de falha | `build/kernel.sym`, symbolication no kernel, `tools/symbolize` |

### Semanas 7–8 — Memória física
| | Item | Evidência |
|---|---|---|
| [x] | normalizar mapa de memória | `nexo_mm::normalize` + testes de host |
| [x] | marcar regiões reservadas | `mm::phys::init` (1 MiB, framebuffer, kernel, ACPI, runtime) |
| [x] | frame allocator | `BitmapFrameAllocator` |
| [x] | testar alocação, liberação e exaustão | host `bitmap::tests`; kernel `frame_alloc` |
| [x] | invariantes e testes no host | `cargo test -p nexo-mm` |
| [x] | mapear framebuffer e regiões necessárias | physmap cobre RAM + framebuffer |

### Semanas 9–10 — Memória virtual e heap
| | Item | Evidência |
|---|---|---|
| [x] | abstração de page tables | `nexo_arch_x86_64::paging::Mapper` (testes em arena) |
| [x] | mapear/desmapear páginas | teste `paging_map_unmap` |
| [x] | permissões RW/NX | testes `section_permissions`, `write_protect`, `nx` |
| [x] | guard page | teste `guard_page`; cenário `overflow` |
| [x] | heap do kernel | `nexo-heap` + `mm/heap.rs`; testes `heap`, `heap_grow` |
| [x] | page fault intencional | teste `page_fault_recovery` |
| [x] | medir e registrar alocações | marcadores `[MEMORY]`/`[HEAP]` no log |

### Semanas 11–12 — Interrupções e release inicial
| | Item | Evidência |
|---|---|---|
| [x] | IDT completa | `x86/traps.rs` (256 vetores) |
| [x] | timer | PIT 1000 Hz |
| [x] | ticks e tempo monotônico inicial | `time.rs`; teste `timer` |
| [x] | duas tarefas cooperativas | `task.rs`; teste `coop_tasks` (`ABABAB`) |
| [x] | boot automatizado no CI | `.github/workflows/ci.yml` (arquivo pronto; execução depende de remoto) |
| [x] | timeout e resultado via serial | `tools/run-qemu --test`, `tools/test-qemu` |
| [x] | documentação revisada e release `0.0.1-boot` | `docs/releases/0.0.1-boot.md`, tag `v0.0.1-boot` |

## Marco: `0.1-kernel` (Fase 1) — em andamento (código em `main`)

| | Item | Evidência |
|---|---|---|
| [x] | mapa de memória do firmware; sair dos boot services; GDT/TSS; IDT; panic com backtrace; alocador físico; heap; RO/NX/guard | herdados de `0.0.1-boot` |
| [-] | tabelas de página e espaços de endereçamento | mapper pronto; só o espaço do kernel (usuário na Fase 2) |
| [x] | APIC, timer e interrupções externas | `x86/apic.rs`, `time.rs`; testes `apic_timer`, `ioapic`, `ipi_self` |
| [x] | descobrir CPUs e SMP | `acpi.rs`, `x86/smp.rs`, `x86/percpu.rs`; teste `smp` (4/4) |
| [x] | threads do kernel e troca de contexto | `sched.rs`; `threads_yield` |
| [x] | escalonador preemptivo | `sched::on_tick`; `threads_preempt` |
| [x] | relógio monotônico e timers | TSC (`monotonic_ns`), `sleep` por thread, `timer::{after_ns, periodic_ns, cancel}` com thread `ktimer`; teste `timers` |
| [x] | afinidade de CPU | `sched::spawn_on/set_affinity`; teste `threads_affinity` |
| [x] | locks, atomics e sincronização | `nexo-sync`, `kernel/src/sync.rs` (`IrqLock`) |
| [x] | testes de concorrência e stress | `stress.rs`; cenário `stress`; `make stress` |
| [x] | limitar e registrar `unsafe` | `docs/unsafe-inventory.md` |
| [x] | symbolication e dump mínimo | herdado; registradores + backtrace por CPU |
| [-] | publicar `0.1-kernel` | **Gate F1** exige 24 h de stress sem falha: `make stress DURATION=86400 SMP=4` |

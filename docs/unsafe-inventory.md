# Inventário de `unsafe` — `main` (Fases 1–3)

Regra (ADR-0001): todo bloco tem `// SAFETY:`; crates puros são `deny(unsafe_code)`. Este inventário lista as **classes** de uso por crate para revisão a cada release.

Contagem em 2026-08-30 (`grep -rhoE 'unsafe( \{| fn| impl| extern)' --include='*.rs'`; `unsafe fn`/`unsafe impl` têm seção `# Safety`/comentário em vez de `SAFETY:` por bloco):

| Crate | ocorrências `unsafe` | comentários `SAFETY:` |
|---|---|---|
| `nexo-boot-abi` | 1 (tipo `unsafe extern "sysv64" fn`) | — |
| `nexo-mm`, `nexo-symbols`, `nexo-font`, `nexo-acpi`, `nexo-elf`, `nexo-initrd`, `nexo-syscall-abi`, `nexofs`, `nexo-fat` | 0 | — |
| `nexo-sync` | 9 | 8 |
| `nexo-heap` | 30 | 27 |
| `nexo-arch-x86_64` | 120 | 78 |
| `nexo-loader` | 15 | 14 |
| `nexo-kernel` | 120 | 111 |
| `nexo-sys + nexo-rt` | 31 | 27 |
| `nexo-virtio` | 10 | 10 (acessos voláteis a MMIO e às páginas da fila; um `SAFETY` por método/bloco) |
| `services/*` | 19 | 19 (drivers: cópias de/para páginas de DMA e a queda deliberada do `blockdev`) |
| **total** | **355** | **294** |

Atualize junto com o código.

| Crate | Classes de `unsafe` | Invariantes que sustentam |
|---|---|---|
| `nexo-boot-abi` | nenhum (`deny(unsafe_code)`) | — |
| `nexo-mm` | nenhum (`deny(unsafe_code)`) | — |
| `nexo-symbols` | nenhum (`deny(unsafe_code)`) | — |
| `nexo-font` | nenhum (`deny(unsafe_code)`) | — |
| `nexo-sync` | `UnsafeCell` atrás de flag atômico; `force_unlock` | acesso só via guard; `force_unlock` só em panic |
| `nexo-heap` | ponteiros para nós/cabeçalhos dentro de regiões entregues a `extend` | regiões exclusivas e mapeadas; cabeçalho com magic; testes aleatórios verificam ausência de sobreposição |
| `nexo-arch-x86_64` | `asm!` (CRs, MSRs, GS, portas, hlt/cli/sti, lgdt/lidt/ltr, invlpg, rdtsc); `global_asm!` (stubs de trap, troca de contexto, trampolim de AP); MMIO volátil do LAPIC/I-O APIC; leituras/escritas voláteis em tabelas de página via `PhysToVirt`; `transmute` do handler registrado | tabelas válidas por construção do `Mapper`; MMIO mapeado sem cache pelo kernel; trampolim copiado para página reservada; handler armazenado a partir de `fn` tipada; layout do `TrapFrame` casado com o assembly |
| `nexo-loader` | cópia para páginas alocadas, escrita de `BootInfo`/regiões, `write_volatile` no framebuffer, `open_protocol` GetProtocol, `exit_boot_services`, salto em `asm!` | páginas alocadas com tamanho verificado; identidade UEFI antes de `ExitBootServices`; nenhum serviço de boot após a saída |
| `nexo-kernel` | `StaticCell` (IDT/regiões) e `PerCpu` vazado em init; slices sobre memória do `BootInfo`/ACPI pelo physmap; `GlobalAlloc`; `prepare_stack`/`switch_context` com lock do escalonador atravessando a troca (`forget` + `force_unlock`); ponteiros crus de `Arc<Thread>` em `gs`; `inner()` de thread só sob o lock; sondas em `asm!`; `force_unlock` em panic | inicialização por CPU antes de `sti` naquela CPU; regiões do loader nunca reutilizadas; locks do kernel sempre com IRQs off; thread moribunda só vira `Dead` depois de a CPU trocar de pilha (`finish_switch`); pilhas liberadas apenas por `reap` de threads `Dead`; handler de `#PF` só redireciona quando a página sondada casa com CR2 |

| `nexo-sys`/`nexo-rt` (usuário) | `asm!` da instrução `syscall`; `raw`/`raw5` são `unsafe fn` | o kernel valida todos os ponteiros; o único risco é o próprio processo |
| `services/*` (usuário) | acessos deliberadamente inválidos nos testes (`utest`, `echo`) e chamadas `raw` | isolados pelo kernel: só o processo morre |
| `nexo-kernel` (adições) | `copy_to_user` após validação `USER|WRITABLE`; `park_with` (solta o lock após marcar bloqueada); coletor de pontas (só `Arc`/locks, sem `unsafe`) | ver `docs/spec/syscall-abi.md` §4 |

## Orçamento e ações

- Nenhum `unsafe` fora de `arch/`, `mm/`, `heap`, `sync`, loader e `kernel/src/{x86,task,console,panic,klog}`.
- Próxima release: adicionar lint de contagem por arquivo no CI e revisar `transmute` do handler (substituir por `Once<TrapHandler>`).

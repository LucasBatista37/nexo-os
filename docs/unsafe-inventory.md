# Inventário de `unsafe` — `main` (Fase 1)

Regra (ADR-0001): todo bloco tem `// SAFETY:`; crates puros são `deny(unsafe_code)`. Este inventário lista as **classes** de uso por crate para revisão a cada release.

Contagem em 2026-08-30 (`grep -rhoE 'unsafe( \{| fn| impl| extern)' --include='*.rs'`; `unsafe fn`/`unsafe impl` têm seção `# Safety`/comentário em vez de `SAFETY:` por bloco):

| Crate | ocorrências `unsafe` | comentários `SAFETY:` |
|---|---|---|
| `nexo-boot-abi` | 1 (tipo `unsafe extern "sysv64" fn`) | — |
| `nexo-mm`, `nexo-symbols`, `nexo-font`, `nexo-acpi` | 0 | — |
| `nexo-sync` | 9 | 8 |
| `nexo-heap` | 30 | 27 |
| `nexo-arch-x86_64` | 107 | 71 |
| `nexo-loader` | 15 | 14 |
| `nexo-kernel` | 96 | 85 |
| **total** | **258** | **205** |

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

## Orçamento e ações

- Nenhum `unsafe` fora de `arch/`, `mm/`, `heap`, `sync`, loader e `kernel/src/{x86,task,console,panic,klog}`.
- Próxima release: adicionar lint de contagem por arquivo no CI e revisar `transmute` do handler (substituir por `Once<TrapHandler>`).

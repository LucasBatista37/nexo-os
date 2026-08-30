# Inventário de `unsafe` — release `0.0.1-boot`

Regra (ADR-0001): todo bloco tem `// SAFETY:`; crates puros são `deny(unsafe_code)`. Este inventário lista as **classes** de uso por crate para revisão a cada release.

Contagem em 2026-08-29 (`grep -rhoE 'unsafe( \{| fn| impl| extern)' --include='*.rs'`; `unsafe fn`/`unsafe impl` têm seção `# Safety`/comentário em vez de `SAFETY:` por bloco):

| Crate | ocorrências `unsafe` | comentários `SAFETY:` |
|---|---|---|
| `nexo-boot-abi` | 1 (tipo `unsafe extern "sysv64" fn`) | — |
| `nexo-mm`, `nexo-symbols`, `nexo-font` | 0 | — |
| `nexo-sync` | 9 | 8 |
| `nexo-heap` | 30 | 27 |
| `nexo-arch-x86_64` | 79 | 52 (o restante são `unsafe fn` com `# Safety`) |
| `nexo-loader` | 15 | 14 |
| `nexo-kernel` | 47 | 41 |
| **total** | **181** | **142** |

Atualize junto com o código.

| Crate | Classes de `unsafe` | Invariantes que sustentam |
|---|---|---|
| `nexo-boot-abi` | nenhum (`deny(unsafe_code)`) | — |
| `nexo-mm` | nenhum (`deny(unsafe_code)`) | — |
| `nexo-symbols` | nenhum (`deny(unsafe_code)`) | — |
| `nexo-font` | nenhum (`deny(unsafe_code)`) | — |
| `nexo-sync` | `UnsafeCell` atrás de flag atômico; `force_unlock` | acesso só via guard; `force_unlock` só em panic |
| `nexo-heap` | ponteiros para nós/cabeçalhos dentro de regiões entregues a `extend` | regiões exclusivas e mapeadas; cabeçalho com magic; testes aleatórios verificam ausência de sobreposição |
| `nexo-arch-x86_64` | `asm!` (CRs, MSRs, portas, hlt/cli/sti, lgdt/lidt/ltr, invlpg, rdtsc); `global_asm!` (stubs de trap, troca de contexto); leituras/escritas voláteis em tabelas de página via `PhysToVirt`; `transmute` do handler registrado | tabelas válidas por construção do `Mapper`; handler armazenado a partir de `fn` tipada; layout do `TrapFrame` casado com o assembly (documentado no arquivo) |
| `nexo-loader` | cópia para páginas alocadas, escrita de `BootInfo`/regiões, `write_volatile` no framebuffer, `open_protocol` GetProtocol, `exit_boot_services`, salto em `asm!` | páginas alocadas com tamanho verificado; identidade UEFI antes de `ExitBootServices`; nenhum serviço de boot após a saída |
| `nexo-kernel` | `StaticCell` (GDT/IDT/TSS/pilha #DF/regiões) em init single-core; slices sobre memória do `BootInfo`; escrita/leitura pelo physmap; `GlobalAlloc`; `prepare_stack`/`switch_context`; sondas em `asm!` (`int3`, acessos propositais); `force_unlock` em panic | inicialização única antes de `sti`; regiões do loader nunca reutilizadas; heap serializado com IRQs off; pilhas de tarefa vivas até `reap`; handler de `#PF` só redireciona quando a página sondada casa com CR2 |

## Orçamento e ações

- Nenhum `unsafe` fora de `arch/`, `mm/`, `heap`, `sync`, loader e `kernel/src/{x86,task,console,panic,klog}`.
- Próxima release: adicionar lint de contagem por arquivo no CI e revisar `transmute` do handler (substituir por `Once<TrapHandler>`).

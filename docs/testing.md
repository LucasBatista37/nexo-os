# Testes

## Camadas

| Camada | Comando | O que cobre |
|---|---|---|
| Host (`cargo test`) | `cargo test --workspace` | ABI de boot (layout/validação/cmdline), endereços, normalização do mapa, bitmap de quadros, paginação (arena), heap (inclui aleatório com verificação de corrupção), spinlock/once, símbolos/demangler, fonte |
| Kernel em QEMU (`selftest`) | `tools/test-qemu --scenario boot` | 15 testes no kernel real: boot info, segmentos, `#BP`, quadros, map/unmap, permissões de seção, recuperação de `#PF`, guard pages, CR0.WP, NX, heap, crescimento do heap, timer, tarefas cooperativas, símbolos |
| Cenários de falha | `tools/test-qemu --scenario panic|fault|overflow` | panic com backtrace simbolizado; `#PF` fatal com CR2/RIP/backtrace; estouro de pilha → `#DF` em IST1 |
| Reprodutibilidade | `make reproducible` | duas builds → mesma imagem |
| Lint | `make lint` | fmt + clippy `-D warnings` nos três workspaces |

## Protocolo serial (o que o CI verifica)

- Sucesso: `[RESULT] PASS n/n` e `NEXO: boot completo`, QEMU sai com **33** (`isa-debug-exit`, valor `0x10`).
- Falha: qualquer `FAIL`, `KERNEL PANIC` ou `EXCEPTION` no cenário `boot`; código **35** nos cenários de falha esperada.
- Timeout (`--timeout`, padrão 120 s) → código 124.
- Marcadores estruturados: `[TEST] nome ... ok|FAIL: motivo`, `[MEMORY] …`, `[HEAP] …`, `[TIME] …`.

## Escrevendo um teste de kernel

1. Adicione `("nome", fn)` em `kernel/src/selftest.rs::TESTS`; use `check!(cond, "motivo {x}")`.
2. Para acessos que devem falhar, use `x86::traps::probe(ProbeKind::Read|Write|Exec, addr)` — a falta é capturada e o teste continua.
3. Acrescente o marcador `\[TEST\] nome \.\.\. ok` em `tools/test-qemu`.
4. Logs ficam em `build/logs/<cenário>.log`; `tools/symbolize` resolve endereços.

## Depuração

`tools/run-qemu --gdb` inicia parado com gdbstub em `:1234`. No `lldb`: `target create build/kernel.elf`, `gdb-remote 1234`, `b nexo_kernel::kmain`, `c`. Os símbolos do kernel estão em `build/kernel.sym` (`llvm-nm -n`).

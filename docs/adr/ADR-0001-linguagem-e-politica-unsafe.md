# ADR-0001 — Linguagem principal e política de `unsafe`

- **Status:** aceita
- **Data:** 2026-08-29
- **Relacionados:** ADR-0002, ADR-0003, `docs/unsafe-inventory.md`

## Contexto

O Plano Mestre exige memória segura por construção (§2.2) com `unsafe` e Assembly concentrados, documentados e auditáveis. É preciso escolher linguagem, canal do compilador e regras verificáveis desde o primeiro commit.

## Decisão

1. **Rust estável** (`rust-toolchain.toml` fixa `1.98.0`; edição 2024) é a linguagem de kernel, loader, serviços e ferramentas de build em Rust. Nenhum recurso `nightly`/`-Z` é permitido; se um recurso for indispensável, abre-se uma ADR para justificar.
2. **Assembly** apenas onde Rust não alcança: stubs de trap, troca de contexto, salto do loader, `lgdt/lidt/ltr`, acesso a CRs/MSRs/portas. Fica concentrado em `arch/x86_64` (`global_asm!`/`asm!`), nunca espalhado por serviços.
3. **Ferramentas de build** podem ser Python 3 (sem dependências) quando forem colas de processo (imagem, QEMU, símbolos).
4. Política de `unsafe`:
   - `unsafe_op_in_unsafe_fn = deny` em todo o repositório; `static_mut_refs = deny` no kernel;
   - todo bloco `unsafe` tem comentário `// SAFETY:` (clippy `undocumented_unsafe_blocks`);
   - crates de lógica pura (`nexo-boot-abi`, `nexo-mm`, `nexo-symbols`, `nexo-font`) usam `#![deny(unsafe_code)]`;
   - `docs/unsafe-inventory.md` lista as classes de `unsafe` por crate e é revisado a cada release;
   - orçamento: `unsafe` novo fora de `arch/`, `mm/`, `heap` e drivers exige revisão explícita no PR.
5. Dependências externas: zero no kernel; o loader usa `uefi` (MPL-2.0) e `log` — inventário em `LICENSES.md`, com plano de substituição por bindings próprios se a manutenção do crate falhar (ADR-0012).

## Consequências

- Código compila em ambiente limpo com um `rustup show`; sem `build-std`.
- Interrupt handlers usam stubs em assembly em vez de `abi_x86_interrupt` (nightly): mais código, porém explícito e auditável.
- `panic = "abort"`; sem unwinding no kernel.

## Alternativas consideradas

- **C/C++**: rejeitada — sem segurança de memória por padrão, contraria §2.2.
- **Rust nightly**: rejeitada — reprodutibilidade frágil e pressão de atualização contínua.
- **Zig**: rejeitada — ecossistema e ferramentas de verificação menos maduros para o prazo do projeto.

## Evidência / verificação

`make lint` (clippy `-D warnings` + lint de `unsafe` sem `SAFETY`), `rust-toolchain.toml`, `docs/unsafe-inventory.md`.

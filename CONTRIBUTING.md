# Contribuindo

## Pré-requisitos

- `rustup` (a toolchain exata vem de `rust-toolchain.toml`; `rustup show` instala tudo).
- QEMU ≥ 8 com firmware edk2/OVMF; `mtools`; Python 3.9+.
- macOS: `brew install qemu mtools`. Debian/Ubuntu: `apt install qemu-system-x86 ovmf mtools`.
- Verifique com `tools/check-toolchain`.

## Comandos

| Comando | O que faz |
|---|---|
| `make image` | compila loader + kernel e gera `build/nexo.img` |
| `make run` | inicia no QEMU com display; serial no terminal |
| `make test` | `cargo test --workspace` + `tools/test-qemu` (4 cenários) |
| `make lint` | `cargo fmt --check` e `clippy -D warnings` nos três workspaces |
| `make reproducible` | gera a imagem duas vezes e compara |
| `make ci` | tudo que o CI executa |
| `tools/symbolize build/logs/fault.log` | resolve endereços de um log para função/linha |
| `tools/run-qemu --gdb` | gdbstub em `:1234` (`lldb`: `gdb-remote 1234`) |

## Regras de código

1. Rust estável, edição 2024, sem `nightly` (ADR-0001).
2. Todo `unsafe` com `// SAFETY:`; crates de lógica pura mantêm `#![deny(unsafe_code)]`.
3. Toda funcionalidade vem com teste: de host quando possível, de QEMU quando for de kernel (adicione um marcador em `tools/test-qemu`).
4. Nada de dependências novas sem entrada em `LICENSES.md` e revisão (ADR-0012).
5. Documentação faz parte do código: ADR/spec/`docs/` atualizados no mesmo PR.
6. Mensagens de commit: `area: resumo no imperativo` (ex.: `kernel/mm: adiciona sonda de falta`). Uma mudança lógica por commit.

## Definição de pronto

Ver Plano Mestre §4.2 — espec/ADR, build limpo, testes positivos/negativos/de falha, CI verde, logs diagnosticáveis, limites de segurança analisados, docs atualizadas, `unsafe` justificado, artefato verificável, regressões registradas.

## Revisão

Toda mudança em `arch/`, `mm/`, `heap`, loader ou decodificadores exige revisão explícita de segurança (checklist em `SECURITY.md`). PRs devem anexar o log serial do cenário relevante quando alterarem o comportamento de boot.

# Estado da checklist do Plano Mestre — 2026-08-29

Legenda: `[x]` concluído com evidência · `[-]` em andamento/parcial · `[ ]` não iniciado · `[!]` bloqueado.
Marcações também aplicadas diretamente em `PLANO_MESTRE_SISTEMA_OPERACIONAL.md` (94 itens `[x]`, 14 `[-]`).

## Gates

| Gate | Critério | Estado | Evidência |
|---|---|---|---|
| **F0** | em um clone limpo, um único comando gera uma imagem que inicia em QEMU e o CI comprova a mensagem do kernel | **atendido** | clone novo em diretório temporário + `make ci` → exit 0 em 2026-08-29; `tools/test-qemu` verifica `Nexo OS kernel 0.0.1-boot (x86_64) iniciando`, `[RESULT] PASS 15/15`, `NEXO: boot completo`, código de saída 33 (`docs/releases/0.0.1-boot.md`) |
| 90 dias | fundação reproduzível, observável e testada; sem passos manuais; falhas diagnosticáveis | **atendido** | `make reproducible` OK; panic/exceções com backtrace simbolizado (`build/logs/{panic,fault,overflow}.log`) |
| **F1** | 24 h de stress em QEMU, múltiplas CPUs, memória virtual isolada, exceções tratadas, zero falha não explicada | **em andamento** | código da Fase 1 em `main` (APIC, SMP 4 CPUs, threads preemptivas, stress); cenário `stress` de 15 s verde no CI; a execução de 24 h (`make stress DURATION=86400`) ainda não foi feita |
| **F2** | três processos isolados simultâneos, servidor reiniciável, acessos sem capability falham de forma testada | **iniciado** | bloco 1: ring 3, syscalls v0, processos com espaço próprio, `init` em modo usuário, isolamento testado (`user_isolation`); handles/IPC/capabilities pendentes |
| F3..F10 | — | não iniciados | Plano Mestre §5 |

## §4.3 Documentos obrigatórios — 12/12

`PROJECT_CHARTER.md`, `ARCHITECTURE.md`, `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `LICENSES.md`, `SUPPORTED_HARDWARE.md`, `COMPATIBILITY.md`, `RELEASE.md`, `RECOVERY.md`, `docs/adr/`, `docs/rfc/`.

## §4.4 ADRs — 14/14 escritas

ADR-0001..0004 aceitas em detalhe; ADR-0005, 0007, 0008, 0009, 0010, 0013 aceitas como direção (detalhes via RFC nas fases indicadas); ADR-0012 com licença provisória até revisão jurídica. Índice: `docs/adr/README.md`.

## §5 Fase 0 — 11 `[x]`, 4 `[-]`

| Item | Estado | Nota |
|---|---|---|
| nome/visão/público | [x] | "Nexo OS" é **provisório**, escolhido sem confirmação do dono do projeto |
| computador de desenvolvimento / referência | [-] | host definido; PC de referência a escolher antes da Fase 7 |
| licença | [x] | MIT OR Apache-2.0 (provisória) |
| repositório, branches protegidas, convenções | [-] | git local com convenções; remoto e proteção de branch exigem decisão de hospedagem |
| Rust bare-metal, linker, asm, QEMU | [x] | |
| toolchain fixada | [x] | Rust 1.98.0 |
| UEFI no QEMU | [x] | edk2 do pacote QEMU |
| imagem reproduzível | [x] | mesma imagem em duas builds do mesmo checkout (`make reproducible`, também no clone limpo); hash varia entre diretórios de checkout por causa dos hashes de metadados do cargo nos símbolos — documentado em `docs/toolchain.md` |
| logs por serial | [x] | |
| CI que compila e inicia em QEMU | [x] | workflow escrito; `make ci` executa o mesmo localmente; a execução hospedada começa quando houver remoto |
| teste de sucesso/falha via serial | [x] | |
| templates de ADR/RFC | [x] | |
| threat model v0 | [x] | |
| currículo básico | [-] | guia mapeado ao código; estudo é atividade pessoal contínua |
| publicar `0.0.1-boot` | [-] | tag local `v0.0.1-boot` + notas; "publicar" exige repositório remoto |

## §8 Plano dos 90 dias — 41 `[x]`, 1 `[-]`

Todos os itens das semanas 1–12 implementados e verificados (`docs/board.md` lista a evidência de cada um). Único `[-]`: "publicar `0.0.1-boot`" (motivo acima).

## §15 Checklist de início imediato — 13 `[x]`, 4 `[-]`, 1 `[ ]`

`[-]`: dedicação semanal (meta provisória), PC de referência, criar repositório (remoto), publicar. `[ ]`: "revisar este plano no final de cada trimestre" (recorrente; próxima em 2026-11-29).

## §5 Fase 1 — 14 `[x]`, 5 `[-]` (ver `docs/board.md`)

Pendentes: espaços de endereçamento de usuário (Fase 2), execução do stress de 24 h e a release `0.1-kernel`. Evidência parcial do gate: stress de 30 min com 4 CPUs sem erros (ver `docs/progress/`).

## §5 Fase 2 — 3 `[x]`, 5 `[-]` (ver `docs/board.md`)

## §6 Frentes permanentes — o que já existe

`[x]` especificação da ABI de boot; testes unitários no host; testes kernel/QEMU; build reproduzível. `[-]` memória física e virtual; panic/dump/symbolication; threat model; W^X/NX/ASLR/guard pages (falta ASLR); image builder. Demais itens `[ ]`.

## §9 Currículo, §12 Rotina, §14 Critérios

Não são itens implementáveis pelo repositório; ficam `[ ]` e são conduzidos pela pessoa. Suporte: `docs/study/README.md`, `docs/progress/painel-mensal.md`.

## Decisões provisórias que precisam de confirmação do dono do projeto

1. Nome "Nexo OS".
2. Licença MIT OR Apache-2.0.
3. Dedicação semanal (10–15 h).
4. Hospedagem do repositório (GitHub?) — habilita CI hospedado, branches protegidas e publicação da release.
5. Computador de referência (só antes da Fase 7).

# ADR-0014 — Estratégia de compatibilidade POSIX/Linux/Web

- **Status:** aceita
- **Data:** 2026-08-29
- **Relacionados:** ADR-0006, ADR-0008

## Decisão

Ordem (§3.8): (1) ABI nativa C + SDK Rust; (2) biblioteca padrão e runtime próprios; (3) subconjunto POSIX (POSIX.1-2024, Issue 8) como **personalidade de compatibilidade** em espaço de usuário, mapeando descritores para handles; (4) APIs suficientes para portar softwares portáveis; (5) motor web existente portado com sandbox; (6) VM/contêiner Linux opcional; (7) Win32/Wine só como pesquisa tardia.

POSIX nunca dita a arquitetura: sem `fork` no kernel (spawn nativo), sem uid/gid como mecanismo de segurança primário, sem sinais assíncronos como base do IPC.

**Primeiro passo implementado:** `sdk/nexo-net` — sockets BSD (`socket`/`connect`/`send`/`recv`/`close`/`getaddrinfo`, `sockaddr_in`, `errno`) sobre `nexo.sock`, com descritores inteiros mapeados para conexões/portas do `netd` (§3, passo 3). Exercitado no cenário `net`.

## Alternativas

Ser uma distribuição Linux (rejeitado pelo escopo do projeto, §1.1); ignorar POSIX (rejeitado: portar ferramentas é o caminho mais curto para utilidade).

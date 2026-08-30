# ADR-0004 — Modelo de objetos, handles e capabilities

- **Status:** aceita (direção); detalhes finais via RFC na Fase 2
- **Data:** 2026-08-29
- **Relacionados:** ADR-0002, ADR-0005, ADR-0006

## Contexto

§3.4–3.5 definem os objetos mínimos do kernel e uma superfície de syscalls pequena e versionada. A decisão precisa existir antes de escrever qualquer código de modo usuário para evitar que APIs nasçam sem capabilities (risco "segurança tardia", §11).

## Decisão

1. **Tudo é objeto do kernel** referenciado por **handles** locais ao processo; nenhum recurso é acessível por nome global ou por PID sem um handle.
2. Objetos iniciais: `Process`, `Thread`, `AddressSpace`, `MemoryObject`, `Channel`, `Port/Event`, `Timer`, `Interrupt`, `DeviceMemory`, `Job/Domain`, `Capability/Handle`.
3. **Direitos** por handle: `READ`, `WRITE`, `SIGNAL`, `MAP`, `TRANSFER`, `DUPLICATE`, `ADMIN`. Direitos só diminuem (`reduce_rights`); nunca aumentam.
4. Handles são **não forjáveis**: índices em uma tabela por processo, validados pelo kernel; valores numéricos não vazam significado entre processos.
5. **Transferência** de handles apenas por `Channel` (IPC); o kernel reescreve os índices no destino.
6. **Syscalls** cobrem apenas §3.5 (tarefas, memória, canais/espera/sinal, capabilities, tempo/timers, interrupções/MMIO autorizados, depuração mínima, administração via capabilities especiais). Arquivos, sockets, janelas e áudio são protocolos IPC de serviços.
7. A ABI de syscall é **versionada** (número de versão consultável) com convenção de erro única (`Result<u64, Status>`; códigos estáveis).
8. O `Job/Domain` raiz recebe todas as capabilities no boot; `init` distribui capacidades mínimas aos serviços.

## Consequências

- Cada syscall e cada mensagem IPC validam handle + direitos; testes negativos são obrigatórios (gate F2).
- Ferramentas de fuzzing de syscalls/IPC entram na Fase 2.
- A ABI nativa (ADR-0006) expõe handles como inteiros opacos.

## Alternativas consideradas

- **Modelo POSIX (uid/gid + descritores)**: rejeitado como base — ambiente ambiental, difícil de restringir; será uma personalidade de compatibilidade (ADR-0014).
- **Capabilities em espaço de usuário apenas**: rejeitado — sem aplicação pelo kernel não há isolamento real.

## Evidência / verificação

RFC-0001 (Fase 2) com a tabela final de objetos/direitos; testes de negação de capability em `tests/qemu`.

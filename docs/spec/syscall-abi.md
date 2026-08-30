# Especificação — ABI de syscalls (versão 0, instável)

**Crate de referência:** `abi/syscall` (`nexo-syscall-abi`). **SDK mínimo:** `sdk/nexo-sys`. **Implementação:** `kernel/src/x86/syscall.rs`, entrada em `arch/x86_64/src/syscall.rs`.

Antes de `0.9-beta` esta ABI muda sem aviso (ADR-0006). A versão é consultável por `SYS_ABI_VERSION`.

## 1. Convenção (x86_64)

| Item | Valor |
|---|---|
| Instrução | `syscall` (ring 3 → ring 0 via `LSTAR`); retorno por `sysretq` |
| Número | `RAX` |
| Argumentos | `RDI`, `RSI`, `RDX`, `R10`, `R8`, `R9` (a v0 usa até 3) |
| Retorno | `RAX` = `Status` (0 = OK); `RDX` = valor |
| Destruídos | `RCX`, `R11` (pela instrução); demais registradores preservados |
| Pilha | o kernel troca para a pilha de kernel da thread (`gs:[8]`); a pilha do usuário não é tocada |
| Interrupções | mascaradas na entrada (`SFMASK`), reabilitadas dentro do kernel; a syscall pode bloquear |

Seletores: código do usuário `0x2b`, dados `0x23` (`STAR[63:48] = 0x18`); código do kernel `0x08`, dados `0x10`.

## 2. Syscalls v0

| Nº | Nome | Args | Retorno (`RDX`) | Erros |
|---|---|---|---|---|
| 0 | `exit` | código (i64) | não retorna | — |
| 1 | `log` | ptr, len (≤ 1024, UTF-8) | bytes escritos | `BadAddress`, `InvalidArgs` |
| 2 | `time_now` | — | ns monotônicos | — |
| 3 | `yield` | — | 0 | — |
| 4 | `sleep` | ns | 0 | — |
| 5 | `get_pid` | — | pid | — |
| 6 | `abi_version` | — | 0 | — |
| 7 | `debug_info` | 0 CPUs online / 1 uptime ms / 2 syscalls do processo | valor | `InvalidArgs` |

Números desconhecidos devolvem `NotSupported` (3) sem efeitos.

## 3. Status

`Ok`=0, `InvalidArgs`=1, `BadAddress`=2, `NotSupported`=3, `NoMemory`=4, `NotFound`=5, `Denied`=6.

## 4. Validação de ponteiros

Todo ponteiro de usuário é validado antes do acesso: faixa `[ptr, ptr+len)` abaixo de `0x0000_8000_0000_0000` e cada página mapeada com o bit `USER` no espaço do processo; caso contrário `BadAddress`. O kernel copia os bytes para memória própria antes de usá-los.

## 5. Processos nesta versão

- Espaço de endereçamento por processo (PML4 própria; metade do kernel compartilhada), carregado de um ELF64 estático com segmentos W^X; pilha de 64 KiB em `0x0000_7fff_fff0_0000` (guard page abaixo).
- Uma thread por processo; `RDI` na entrada carrega um argumento inteiro.
- Falha em modo usuário (`#PF`, `#GP`, `#UD`…) encerra apenas o processo com código `-1` e motivo registrado no log; o kernel continua.
- Sem handles, capabilities, IPC ou memória compartilhada ainda — próximos blocos da Fase 2 (ADR-0004/0005).

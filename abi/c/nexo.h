/* nexo.h — cabecalhos C minimos do Nexo OS (Plano §Fase 6: "publicar headers e toolchain
 * C/C++"). Freestanding: so <stdint.h>. Convencao de syscall (ABI v1 experimental,
 * docs/spec/syscall-abi.md): rax = numero -> status; rdi/rsi/rdx = argumentos; rdx tambem
 * devolve o valor; rcx/r11 sao destruidos pela instrucao `syscall`. */
#ifndef NEXO_H
#define NEXO_H

#include <stdint.h>
#include "nexo_syscalls.h"

typedef struct {
    uint64_t status; /* NEXO_STATUS_* */
    uint64_t value;
} nexo_ret;

static inline nexo_ret nexo_syscall3(uint64_t n, uint64_t a0, uint64_t a1, uint64_t a2) {
    uint64_t status = n, value = a2;
    __asm__ volatile("syscall"
                     : "+a"(status), "+d"(value)
                     : "D"(a0), "S"(a1)
                     : "rcx", "r11", "memory");
    nexo_ret r = {status, value};
    return r;
}

/* Encerra o processo com `code`. */
static inline void nexo_exit(int64_t code) {
    nexo_syscall3(NEXO_SYS_EXIT, (uint64_t)code, 0, 0);
    __builtin_unreachable();
}

/* Escreve `len` bytes de `msg` no log do kernel (aparece na serial, prefixado pelo pid). */
static inline uint64_t nexo_log(const char *msg, uint64_t len) {
    return nexo_syscall3(NEXO_SYS_LOG, (uint64_t)msg, len, 0).status;
}

/* Cede a CPU. */
static inline void nexo_yield(void) { nexo_syscall3(NEXO_SYS_YIELD, 0, 0, 0); }

/* Segundos Unix (UTC) do relogio de parede; 0 = sem RTC. */
static inline uint64_t nexo_wall_epoch(void) {
    return nexo_syscall3(NEXO_SYS_DEBUG_INFO, 7, 0, 0).value;
}

#endif /* NEXO_H */

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

/* Syscall de 5 argumentos (a3 em r10, a4 em r8 — ABI v0). */
static inline nexo_ret nexo_syscall5(uint64_t n, uint64_t a0, uint64_t a1, uint64_t a2,
                                     uint64_t a3, uint64_t a4) {
    uint64_t status = n, value = a2;
    register uint64_t r10 __asm__("r10") = a3;
    register uint64_t r8 __asm__("r8") = a4;
    __asm__ volatile("syscall"
                     : "+a"(status), "+d"(value)
                     : "D"(a0), "S"(a1), "r"(r10), "r"(r8)
                     : "rcx", "r11", "memory");
    nexo_ret r = {status, value};
    return r;
}

/* Envia uma mensagem pelo canal `ch` (handles viajam no vetor; pode ser NULL/0). */
static inline uint64_t nexo_channel_send(uint32_t ch, const void *data, uint64_t len,
                                         const uint32_t *handles, uint64_t n_handles) {
    return nexo_syscall5(NEXO_SYS_CHANNEL_SEND, ch, (uint64_t)data, len, (uint64_t)handles,
                         n_handles)
        .status;
}

/* Recebe uma mensagem (bloqueante). Devolve o status; com NEXO_STATUS_OK, escreve o numero de
 * bytes e de handles recebidos. */
static inline uint64_t nexo_channel_recv(uint32_t ch, void *buf, uint64_t cap, uint32_t *handles,
                                         uint64_t h_cap, uint64_t *n_bytes, uint64_t *n_handles) {
    nexo_ret r = nexo_syscall5(NEXO_SYS_CHANNEL_RECV, ch, (uint64_t)buf, cap, (uint64_t)handles,
                               h_cap);
    if (r.status == NEXO_STATUS_OK) {
        *n_bytes = r.value & 0xffffffffULL;
        *n_handles = r.value >> 32;
    }
    return r.status;
}

#endif /* NEXO_H */

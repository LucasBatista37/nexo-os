/* hello.c — primeiro processo em C do Nexo OS: loga pelo kernel e sai limpo.
 * Compilado freestanding (sem libc) por tools/build-c-demo com clang + rust-lld. */
#include "../../abi/c/nexo.h"

void _start(uint64_t arg) {
    (void)arg;
    static const char msg[] = "ola do C freestanding (headers abi/c)";
    nexo_log(msg, sizeof(msg) - 1);
    nexo_exit(0);
}

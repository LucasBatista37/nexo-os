/* hello.c — primeiro processo em C do Nexo OS, agora sobre a nexo-libc minima (string.h e
 * stdio.h proprios; puts sai pelo log do kernel). Compilado freestanding por
 * tools/build-c-demo com clang + rust-lld. */
#include "../../abi/c/nexo.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/string.h"

void _start(uint64_t arg) {
    (void)arg;
    char linha[64];
    const char *base = "ola da nexo-libc";
    memset(linha, 0, sizeof(linha));
    memcpy(linha, base, strlen(base));
    if (strcmp(linha, "ola da nexo-libc") != 0 || strncmp(linha, "ola", 3) != 0
        || memcmp(linha, base, strlen(base)) != 0 || strlen(linha) != 16) {
        nexo_exit(1);
    }
    puts(linha);
    nexo_exit(0);
}

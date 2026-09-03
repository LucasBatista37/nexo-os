/* hello.c — primeiro processo em C do Nexo OS, agora sobre a nexo-libc minima (string.h e
 * stdio.h proprios; puts sai pelo log do kernel). Compilado freestanding por
 * tools/build-c-demo com clang + rust-lld. */
#include "../../abi/c/nexo.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/stdlib.h"
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
    /* heap da libc: malloc/free/calloc/realloc sobre os objetos de memoria do kernel */
    char *a = malloc(1000);
    char *b = malloc(50);
    if (!a || !b) {
        nexo_exit(2);
    }
    memset(a, 0xAB, 1000);
    memcpy(b, "cinquenta", 10);
    free(a);
    char *c = malloc(900); /* reusa o buraco do `a` (primeira adequacao) */
    if (!c || memcmp(b, "cinquenta", 10) != 0) {
        nexo_exit(3);
    }
    int *z = calloc(64, sizeof(int));
    if (!z) {
        nexo_exit(4);
    }
    for (int i = 0; i < 64; i++) {
        if (z[i] != 0) {
            nexo_exit(5);
        }
    }
    char *r = realloc(b, 4000); /* cresce preservando o conteudo */
    if (!r || memcmp(r, "cinquenta", 10) != 0) {
        nexo_exit(6);
    }
    free(r);
    free(c);
    free(z);
    /* printf/snprintf: numeros com sinal e zero-pad, hex nos dois casos, truncamento com
     * retorno completo, modificadores l/ll/z, largura de %s, %c e %% — conferidos por strcmp */
    char f[40];
    if (snprintf(f, sizeof(f), "%d %05d %u %x %X", -42, -42, 7u, 0xbeefu, 0xBEEFu) != 21
        || strcmp(f, "-42 -0042 7 beef BEEF") != 0) {
        nexo_exit(7);
    }
    if (snprintf(f, 4, "abcdef") != 6 || strcmp(f, "abc") != 0) {
        nexo_exit(8);
    }
    if (snprintf(f, sizeof(f), "%lu:%llx:%zu:%3s:%c:%%", 123456789012UL, 0xffffffffffULL,
                 (size_t)9, "ab", 'k') != 33
        || strcmp(f, "123456789012:ffffffffff:9: ab:k:%") != 0) {
        nexo_exit(9);
    }
    puts(linha);
    puts("heap da nexo-libc ok (malloc/free/calloc/realloc)");
    printf("printf da nexo-libc ok (%d casos, ate %s)\n", 3, "truncamento");
    nexo_stdio_flush(); /* _start proprio, sem crt0 — flush explicito por clareza */
    nexo_exit(0);
}

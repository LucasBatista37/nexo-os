/* wc.c — o primeiro utilitario POSIX portado ao Nexo (Plano §Fase 6: "portar toolchain e
 * utilitarios POSIX prioritarios"): conta linhas, palavras e bytes de um arquivo. O corpo e
 * wc classico sobre open/read/close, saida via printf da nexo-libc.
 * Uso: wc <arquivo>  (argv pela convencao do crt0, canal no handle 1). */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/string.h"
#include "../../sdk/libc/include/unistd.h"

int main(int argc, char **argv) {
    int fd = 0; /* sem argumento: conta o stdin (canal no handle 2), como o wc classico */
    if (argc >= 2) {
        nexo_libc_use_fs(0);
        fd = open(argv[1], O_RDONLY);
        if (fd < 0) {
            puts("wc: nao abriu o arquivo");
            return 1;
        }
    }
    unsigned long linhas = 0, palavras = 0, bytes = 0;
    int dentro = 0;
    char buf[512];
    ssize_t n;
    while ((n = read(fd, buf, sizeof(buf))) > 0) {
        for (ssize_t i = 0; i < n; i++) {
            bytes++;
            char c = buf[i];
            if (c == '\n')
                linhas++;
            if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                dentro = 0;
            } else if (!dentro) {
                dentro = 1;
                palavras++;
            }
        }
    }
    if (fd != 0) {
        close(fd);
        printf("%lu %lu %lu %s\n", linhas, palavras, bytes, argv[1]);
    } else {
        printf("%lu %lu %lu\n", linhas, palavras, bytes);
    }
    return 0;
}

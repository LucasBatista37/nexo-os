/* wc.c — o primeiro utilitario POSIX portado ao Nexo (Plano §Fase 6: "portar toolchain e
 * utilitarios POSIX prioritarios"): conta linhas, palavras e bytes de um arquivo. O corpo e
 * wc classico sobre open/read/close; a saida usa puts (numeros formatados a mao — printf
 * ainda nao existe). Uso: wc <arquivo>  (argv pela convencao do crt0, canal no handle 1). */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/string.h"
#include "../../sdk/libc/include/unistd.h"

static char *poe_num(char *p, unsigned long v) {
    char d[20];
    int n = 0;
    do {
        d[n++] = (char)('0' + v % 10);
        v /= 10;
    } while (v);
    while (n)
        *p++ = d[--n];
    return p;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        puts("uso: wc <arquivo>");
        return 2;
    }
    nexo_libc_use_fs(0);
    int fd = open(argv[1], O_RDONLY);
    if (fd < 0) {
        puts("wc: nao abriu o arquivo");
        return 1;
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
    close(fd);
    char linha[96];
    char *p = linha;
    p = poe_num(p, linhas);
    *p++ = ' ';
    p = poe_num(p, palavras);
    *p++ = ' ';
    p = poe_num(p, bytes);
    *p++ = ' ';
    for (const char *s = argv[1]; *s; s++)
        *p++ = *s;
    *p = 0;
    puts(linha);
    return 0;
}

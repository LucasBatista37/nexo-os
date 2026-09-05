/* grep.c — utilitario POSIX portado: imprime as linhas que CONTEM o padrao (substring, sem
 * regex na v0). Sai 0 se achou alguma, 1 se nenhuma — a semantica classica de grep.
 * Uso: grep <padrao> [arquivo]  (sem arquivo, le o stdin). */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/string.h"
#include "../../sdk/libc/include/unistd.h"

#define LINHA_MAX 512

static char linha[LINHA_MAX];
static size_t ln;
static int achou;

static void fecha_linha(const char *padrao) {
    linha[ln] = 0;
    if (ln && strstr(linha, padrao)) {
        achou = 1;
        printf("%s\n", linha);
    }
    ln = 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        puts("uso: grep <padrao> [arquivo]");
        return 2;
    }
    int fd = 0;
    if (argc > 2) {
        nexo_libc_use_fs(0);
        fd = open(argv[2], O_RDONLY);
        if (fd < 0) {
            puts("grep: nao abriu o arquivo");
            return 2;
        }
    }
    char buf[512];
    ssize_t n;
    while ((n = read(fd, buf, sizeof(buf))) > 0) {
        for (ssize_t i = 0; i < n; i++) {
            if (buf[i] == '\n') {
                fecha_linha(argv[1]);
            } else if (ln + 1 < LINHA_MAX) {
                linha[ln++] = buf[i];
            }
        }
    }
    fecha_linha(argv[1]); /* ultima linha sem \n conta */
    if (fd != 0)
        close(fd);
    return achou ? 0 : 1;
}

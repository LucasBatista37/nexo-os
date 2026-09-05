/* head.c — utilitario POSIX portado: imprime as primeiras N linhas (padrao 10) de um arquivo
 * ou do stdin. Uso: head [-n N] [arquivo] */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/stdlib.h"
#include "../../sdk/libc/include/string.h"
#include "../../sdk/libc/include/unistd.h"

int main(int argc, char **argv) {
    unsigned long quer = 10;
    int i = 1;
    if (i + 1 < argc && strcmp(argv[i], "-n") == 0) {
        int n = atoi(argv[i + 1]);
        if (n <= 0) {
            puts("uso: head [-n N] [arquivo]");
            return 2;
        }
        quer = (unsigned long)n;
        i += 2;
    }
    int fd = 0;
    if (i < argc) {
        nexo_libc_use_fs(0);
        fd = open(argv[i], O_RDONLY);
        if (fd < 0) {
            puts("head: nao abriu o arquivo");
            return 1;
        }
    }
    unsigned long linhas = 0;
    char buf[512];
    ssize_t n;
    while (linhas < quer && (n = read(fd, buf, sizeof(buf))) > 0) {
        ssize_t corte = n;
        for (ssize_t k = 0; k < n; k++) {
            if (buf[k] == '\n' && ++linhas == quer) {
                corte = k + 1;
                break;
            }
        }
        write(1, buf, (size_t)corte);
    }
    if (fd != 0)
        close(fd);
    return 0;
}

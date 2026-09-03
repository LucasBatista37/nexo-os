/* cat.c — utilitario POSIX portado: copia um arquivo para a saida (write(1) sai pelo caminho
 * em linhas do stdio; um final sem '\n' e publicado pelo flush do crt0 na saida).
 * Uso: cat <arquivo>  (argv pela convencao do crt0, canal do nexo.fs no handle 0). */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/unistd.h"

int main(int argc, char **argv) {
    if (argc < 2) {
        puts("uso: cat <arquivo>");
        return 2;
    }
    nexo_libc_use_fs(0);
    int fd = open(argv[1], O_RDONLY);
    if (fd < 0) {
        puts("cat: nao abriu o arquivo");
        return 1;
    }
    char buf[512];
    ssize_t n;
    while ((n = read(fd, buf, sizeof(buf))) > 0)
        write(1, buf, (size_t)n);
    close(fd);
    return 0;
}

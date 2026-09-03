/* cp.c — utilitario POSIX portado: copia origem -> destino (O_CREAT|O_TRUNC, entao rodar de
 * novo sobrescreve — cp classico). Sem rename no nexo.fs, um mv atomico nao existe ainda; a
 * dupla cp+rm cobre o fluxo. Uso: cp <origem> <destino>  (argv pela convencao do crt0). */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/unistd.h"

int main(int argc, char **argv) {
    if (argc < 3) {
        puts("uso: cp <origem> <destino>");
        return 2;
    }
    nexo_libc_use_fs(0);
    int de = open(argv[1], O_RDONLY);
    if (de < 0) {
        puts("cp: nao abriu a origem");
        return 1;
    }
    int para = open(argv[2], O_WRONLY | O_CREAT | O_TRUNC);
    if (para < 0) {
        puts("cp: nao abriu o destino");
        close(de);
        return 1;
    }
    char buf[512];
    ssize_t n;
    while ((n = read(de, buf, sizeof(buf))) > 0) {
        if (write(para, buf, (size_t)n) != n) {
            puts("cp: escrita curta no destino");
            close(de);
            close(para);
            return 1;
        }
    }
    close(de);
    close(para);
    return 0;
}

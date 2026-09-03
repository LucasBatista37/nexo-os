/* rm.c — utilitario POSIX portado: remove um arquivo por caminho (unlink do nexo.fs; no
 * NexoFS o mesmo unlink remove diretorio VAZIO). Uso: rm <caminho>  (argv via crt0). */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/unistd.h"

int main(int argc, char **argv) {
    if (argc < 2) {
        puts("uso: rm <caminho>");
        return 2;
    }
    nexo_libc_use_fs(0);
    if (unlink(argv[1]) != 0) {
        puts("rm: nao removeu");
        return 1;
    }
    return 0;
}

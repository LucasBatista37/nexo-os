/* mkdir.c — utilitario POSIX portado: cria um diretorio (mkdir do nexo.fs; o modo da
 * assinatura POSIX e ignorado — sem permissoes por arquivo na v0). Uso: mkdir <caminho> */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/sys/stat.h"

int main(int argc, char **argv) {
    if (argc < 2) {
        puts("uso: mkdir <caminho>");
        return 2;
    }
    nexo_libc_use_fs(0);
    if (mkdir(argv[1], 0755) != 0) {
        puts("mkdir: nao criou o diretorio");
        return 1;
    }
    return 0;
}

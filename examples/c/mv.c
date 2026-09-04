/* mv.c — utilitario POSIX portado: renomeia/move via rename do nexo.fs (mesmo volume; o
 * destino nao pode existir — sem substituicao na v1.1). O commit e a entrada nova: um corte
 * deixa o arquivo com um ou dois nomes, nunca com zero.
 * Uso: mv <origem> <destino>  (argv pela convencao do crt0, canal do nexo.fs no handle 0). */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/unistd.h"

int main(int argc, char **argv) {
    if (argc < 3) {
        puts("uso: mv <origem> <destino>");
        return 2;
    }
    nexo_libc_use_fs(0);
    if (rename(argv[1], argv[2]) != 0) {
        puts("mv: nao moveu");
        return 1;
    }
    return 0;
}

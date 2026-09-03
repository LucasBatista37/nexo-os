/* ls.c — utilitario POSIX portado: lista um diretorio via opendir/readdir/closedir da
 * nexo-libc (uma linha por entrada: 'd' para diretorio, '-' para arquivo, como o ls do
 * shell de diagnostico). Uso: ls [diretorio]  (padrao "/"; argv pela convencao do crt0). */
#include "../../sdk/libc/include/dirent.h"
#include "../../sdk/libc/include/fcntl.h" /* nexo_libc_use_fs */
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/unistd.h"

int main(int argc, char **argv) {
    const char *dir = argc > 1 ? argv[1] : "/";
    nexo_libc_use_fs(0);
    DIR *d = opendir(dir);
    if (!d) {
        puts("ls: nao abriu o diretorio");
        return 1;
    }
    struct dirent *e;
    while ((e = readdir(d)))
        printf("%c %s\n", e->d_type == DT_DIR ? 'd' : '-', e->d_name);
    closedir(d);
    return 0;
}

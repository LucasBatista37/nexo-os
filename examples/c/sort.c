/* sort.c — utilitario POSIX portado: ordena as linhas do stdin (ou de um arquivo) por
 * strcmp e imprime. Limites da v0: 16 KiB de entrada, 256 linhas (excesso e erro explicito).
 * Uso: sort [arquivo] */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/string.h"
#include "../../sdk/libc/include/unistd.h"

#define DADOS_MAX 16384
#define LINHAS_MAX 256

static char dados[DADOS_MAX];
static char *linhas[LINHAS_MAX];

int main(int argc, char **argv) {
    int fd = 0;
    if (argc > 1) {
        nexo_libc_use_fs(0);
        fd = open(argv[1], O_RDONLY);
        if (fd < 0) {
            puts("sort: nao abriu o arquivo");
            return 2;
        }
    }
    size_t total = 0;
    ssize_t n;
    while ((n = read(fd, dados + total, DADOS_MAX - 1 - total)) > 0) {
        total += (size_t)n;
        if (total >= DADOS_MAX - 1) {
            puts("sort: entrada grande demais");
            return 2;
        }
    }
    if (fd != 0)
        close(fd);
    int nl = 0;
    for (size_t i = 0; i < total;) {
        if (nl == LINHAS_MAX) {
            puts("sort: linhas demais");
            return 2;
        }
        linhas[nl++] = dados + i;
        while (i < total && dados[i] != '\n')
            i++;
        dados[i] = 0; /* fim da linha (a ultima sem \n tambem fecha aqui) */
        i++;
    }
    for (int i = 1; i < nl; i++) { /* insercao: estavel e suficiente para 256 linhas */
        char *chave = linhas[i];
        int j = i - 1;
        while (j >= 0 && strcmp(linhas[j], chave) > 0) {
            linhas[j + 1] = linhas[j];
            j--;
        }
        linhas[j + 1] = chave;
    }
    for (int i = 0; i < nl; i++)
        printf("%s\n", linhas[i]);
    return 0;
}

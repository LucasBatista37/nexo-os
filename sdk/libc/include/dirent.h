/* dirent.h — nexo-libc minima: opendir/readdir/closedir sobre o `list` do nexo.fs.
 * Um opendir faz UMA chamada list (ate 3900 bytes de entradas — o limite do protocolo) e o
 * readdir percorre o resultado localmente. d_type espelha o Kind do NexoFS (1 = arquivo,
 * 2 = diretorio) — NAO os valores DT_* do Linux. Implementacao em ../src/fd.c (partilha o
 * canal e o rpc da camada de arquivos; o canal vem de nexo_libc_use_fs). */
#ifndef NEXO_LIBC_DIRENT_H
#define NEXO_LIBC_DIRENT_H

#include <stddef.h>


#ifdef __cplusplus
extern "C" {
#endif
#define DT_REG 1
#define DT_DIR 2

struct dirent {
    unsigned int d_ino;
    unsigned char d_type;
    char d_name[64];
};

typedef struct {
    int usado;
    unsigned int pos; /* proximo byte em dados */
    unsigned int len; /* bytes validos em dados */
    unsigned char dados[3900];
    struct dirent ent;
} DIR;

DIR *opendir(const char *path);
struct dirent *readdir(DIR *d);
int closedir(DIR *d);

#ifdef __cplusplus
}
#endif

#endif

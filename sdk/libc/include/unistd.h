/* unistd.h — nexo-libc: read/write/close/lseek sobre descritores de arquivo e unlink por
 * caminho (nexo.fs; no NexoFS o unlink tambem remove diretorio VAZIO — nao ha rmdir). */
#ifndef NEXO_LIBC_UNISTD_H
#define NEXO_LIBC_UNISTD_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef long long off_t;
typedef long long ssize_t;

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

ssize_t read(int fd, void *buf, size_t n);
ssize_t write(int fd, const void *buf, size_t n);
int close(int fd);
off_t lseek(int fd, off_t off, int whence);
int unlink(const char *path);

#ifdef __cplusplus
}
#endif

#endif

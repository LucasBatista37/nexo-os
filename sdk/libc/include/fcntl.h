/* fcntl.h — nexo-libc: abertura de arquivos sobre o protocolo nexo.fs (ADR-0014: descritores
 * mapeados para o canal do fs). O runtime precisa do canal: nexo_libc_use_fs(handle). */
#ifndef NEXO_LIBC_FCNTL_H
#define NEXO_LIBC_FCNTL_H

#ifdef __cplusplus
extern "C" {
#endif

#define O_RDONLY 0x0
#define O_WRONLY 0x1
#define O_RDWR 0x2
#define O_CREAT 0x40
#define O_TRUNC 0x200

/* Liga a camada de arquivos ao canal `nexo.fs` (um por processo). */
void nexo_libc_use_fs(unsigned int canal);

int open(const char *path, int flags);

#ifdef __cplusplus
}
#endif

#endif

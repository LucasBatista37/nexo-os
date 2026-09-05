/* sys/stat.h — nexo-libc minima: por ora so mkdir (o `mkdir` do nexo.fs). O modo e aceito
 * pela assinatura POSIX e ignorado — o NexoFS v0 nao tem permissoes por arquivo. */
#ifndef NEXO_LIBC_SYS_STAT_H
#define NEXO_LIBC_SYS_STAT_H


#ifdef __cplusplus
extern "C" {
#endif
int mkdir(const char *path, unsigned int mode);

#ifdef __cplusplus
}
#endif

#endif

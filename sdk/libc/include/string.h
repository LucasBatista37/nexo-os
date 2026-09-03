/* string.h — nexo-libc minima (ADR-0014, passo 2: biblioteca padrao propria).
 * Freestanding; implementacoes em ../src/string.c. */
#ifndef NEXO_LIBC_STRING_H
#define NEXO_LIBC_STRING_H

#include <stddef.h>


#ifdef __cplusplus
extern "C" {
#endif
void *memcpy(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
int memcmp(const void *a, const void *b, size_t n);
size_t strlen(const char *s);
int strcmp(const char *a, const char *b);
int strncmp(const char *a, const char *b, size_t n);

#ifdef __cplusplus
}
#endif

#endif

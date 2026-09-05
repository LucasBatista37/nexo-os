/* stdlib.h — nexo-libc minima: alocador de heap sobre os objetos de memoria do kernel
 * (nexo_memory_create/map). Arenas de 256 KiB com free-list de primeira adequacao; `free`
 * recicla na lista (as paginas voltam ao kernel quando o processo sai). */
#ifndef NEXO_LIBC_STDLIB_H
#define NEXO_LIBC_STDLIB_H

#include <stddef.h>


#ifdef __cplusplus
extern "C" {
#endif
void *malloc(size_t n);
void free(void *p);
void *calloc(size_t nmemb, size_t size);
void *realloc(void *p, size_t n);
int atoi(const char *s); /* decimal com sinal; para no primeiro nao-digito */

#ifdef __cplusplus
}
#endif

#endif

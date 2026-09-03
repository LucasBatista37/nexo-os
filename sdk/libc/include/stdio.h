/* stdio.h — nexo-libc minima: a saida (`puts`/`printf`) sai pelo log do kernel em linhas —
 * um '\n' publica a linha acumulada; um final sem '\n' fica pendente ate nexo_stdio_flush
 * (o crt0 chama ao sair). stdout de verdade (console/term) vem com a integracao do runtime.
 * printf cobre %% %c %s %d %i %u %x %X %p, flag '0', largura e l/ll/z; sem floats/precisao. */
#ifndef NEXO_LIBC_STDIO_H
#define NEXO_LIBC_STDIO_H

#include <stdarg.h>
#include <stddef.h>


#ifdef __cplusplus
extern "C" {
#endif
int puts(const char *s);
int printf(const char *fmt, ...);
int vprintf(const char *fmt, va_list ap);
int snprintf(char *dst, size_t cap, const char *fmt, ...);
int vsnprintf(char *dst, size_t cap, const char *fmt, va_list ap);
void nexo_stdio_flush(void);
void nexo_stdio_write(const char *s, size_t n); /* bytes crus no buffer de linhas */

#ifdef __cplusplus
}
#endif

#endif

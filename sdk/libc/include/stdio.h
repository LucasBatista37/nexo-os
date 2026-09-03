/* stdio.h — nexo-libc minima: por ora `puts` sai pelo log do kernel (nexo_log); stdout de
 * verdade (console/term) vem com a integracao do runtime. */
#ifndef NEXO_LIBC_STDIO_H
#define NEXO_LIBC_STDIO_H


#ifdef __cplusplus
extern "C" {
#endif
int puts(const char *s);

#ifdef __cplusplus
}
#endif

#endif

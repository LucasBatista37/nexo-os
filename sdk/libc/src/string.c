/* string.c — nexo-libc minima. Sem truques: laços simples e corretos (o compilador vetoriza;
 * -fno-builtin evita que estas funcoes virem chamadas a si mesmas). */
#include "../include/string.h"

void *memcpy(void *dst, const void *src, size_t n) {
    unsigned char *d = dst;
    const unsigned char *s = src;
    for (size_t i = 0; i < n; i++)
        d[i] = s[i];
    return dst;
}

void *memset(void *dst, int c, size_t n) {
    unsigned char *d = dst;
    for (size_t i = 0; i < n; i++)
        d[i] = (unsigned char)c;
    return dst;
}

int memcmp(const void *a, const void *b, size_t n) {
    const unsigned char *x = a, *y = b;
    for (size_t i = 0; i < n; i++)
        if (x[i] != y[i])
            return x[i] < y[i] ? -1 : 1;
    return 0;
}

size_t strlen(const char *s) {
    size_t n = 0;
    while (s[n])
        n++;
    return n;
}

int strcmp(const char *a, const char *b) {
    while (*a && *a == *b) {
        a++;
        b++;
    }
    return (unsigned char)*a < (unsigned char)*b ? -1 : (*a != *b);
}

int strncmp(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (a[i] != b[i])
            return (unsigned char)a[i] < (unsigned char)b[i] ? -1 : 1;
        if (!a[i])
            return 0;
    }
    return 0;
}

char *strstr(const char *pajar, const char *agulha) {
    size_t n = strlen(agulha);
    if (n == 0)
        return (char *)pajar;
    for (; *pajar; pajar++) {
        if (strncmp(pajar, agulha, n) == 0)
            return (char *)pajar;
    }
    return 0;
}

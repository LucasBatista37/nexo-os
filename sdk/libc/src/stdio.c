/* stdio.c — nexo-libc: puts/printf saem pelo log do kernel, em LINHAS (nexo_log enquadra uma
 * linha por chamada). puts e printf acumulam num buffer e publicam a cada '\n'; um final sem
 * '\n' fica pendente ate nexo_stdio_flush (o crt0 chama na saida). Linha vazia nao publica
 * nada (o log ja enquadra). Buffers estaticos: um fluxo de saida por processo, sem threads.
 *
 * vsnprintf cobre o nucleo de C: %% %c %s %d %i %u %x %X %p, flag '0', largura decimal e os
 * modificadores l/ll/z (todos 64 bits neste alvo). Sem ponto flutuante e sem precisao (.N):
 * o que os utilitarios portados usam hoje — crescer conforme a demanda, como o resto da libc. */
#include "../include/stdio.h"
#include "../include/string.h"
#include "../../../abi/c/nexo.h"

#define LINHA_MAX 256

static char pendente[LINHA_MAX];
static size_t pend_n;

void nexo_stdio_flush(void) {
    if (pend_n) {
        nexo_log(pendente, pend_n);
        pend_n = 0;
    }
}

static void escreve(const char *s, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (s[i] == '\n') {
            nexo_stdio_flush();
        } else {
            pendente[pend_n++] = s[i];
            if (pend_n == LINHA_MAX)
                nexo_stdio_flush();
        }
    }
}

int puts(const char *s) {
    escreve(s, strlen(s));
    escreve("\n", 1);
    return 0;
}

/* Coletor do vsnprintf: conta sempre (o retorno e o tamanho completo), grava so ate a
 * capacidade — o NUL final e posto pelo chamador. */
struct sink {
    char *dst;
    size_t cap;
    size_t len;
};

static void put1(struct sink *k, char c) {
    if (k->len + 1 < k->cap)
        k->dst[k->len] = c;
    k->len++;
}

static void put_num(struct sink *k, unsigned long long v, unsigned base, int upper, int neg,
                    int width, int zero) {
    const char *dig = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    char d[24];
    int n = 0;
    do {
        d[n++] = dig[v % base];
        v /= base;
    } while (v);
    int total = n + (neg ? 1 : 0);
    if (neg && zero)
        put1(k, '-'); /* o sinal vem ANTES dos zeros: %05d de -42 = "-0042" */
    for (int i = total; i < width; i++)
        put1(k, zero ? '0' : ' ');
    if (neg && !zero)
        put1(k, '-');
    while (n)
        put1(k, d[--n]);
}

int vsnprintf(char *dst, size_t cap, const char *fmt, va_list ap) {
    struct sink k = {dst, cap, 0};
    for (const char *p = fmt; *p; p++) {
        if (*p != '%') {
            put1(&k, *p);
            continue;
        }
        p++;
        int zero = 0, width = 0, l = 0;
        while (*p == '0') {
            zero = 1;
            p++;
        }
        while (*p >= '0' && *p <= '9')
            width = width * 10 + (*p++ - '0');
        while (*p == 'l') {
            l++;
            p++;
        }
        if (*p == 'z') {
            l = 2;
            p++;
        }
        switch (*p) {
        case '%':
            put1(&k, '%');
            break;
        case 'c':
            put1(&k, (char)va_arg(ap, int));
            break;
        case 's': {
            const char *s = va_arg(ap, const char *);
            if (!s)
                s = "(nulo)";
            for (size_t i = strlen(s); i < (size_t)width; i++)
                put1(&k, ' ');
            while (*s)
                put1(&k, *s++);
            break;
        }
        case 'd':
        case 'i': {
            long long v = l == 0 ? va_arg(ap, int)
                        : l == 1 ? va_arg(ap, long)
                                 : va_arg(ap, long long);
            unsigned long long u = v < 0 ? 0ULL - (unsigned long long)v : (unsigned long long)v;
            put_num(&k, u, 10, 0, v < 0, width, zero);
            break;
        }
        case 'u':
        case 'x':
        case 'X': {
            unsigned long long v = l == 0 ? va_arg(ap, unsigned int)
                                 : l == 1 ? va_arg(ap, unsigned long)
                                          : va_arg(ap, unsigned long long);
            put_num(&k, v, *p == 'u' ? 10 : 16, *p == 'X', 0, width, zero);
            break;
        }
        case 'p':
            put1(&k, '0');
            put1(&k, 'x');
            put_num(&k, (unsigned long long)(uintptr_t)va_arg(ap, void *), 16, 0, 0, 0, 0);
            break;
        case 0:
            p--; /* fmt terminou em '%': o for fecha no proximo passo */
            break;
        default: /* especificador desconhecido fica visivel na saida */
            put1(&k, '%');
            put1(&k, *p);
        }
    }
    if (cap)
        dst[k.len < cap ? k.len : cap - 1] = 0;
    return (int)k.len;
}

int snprintf(char *dst, size_t cap, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(dst, cap, fmt, ap);
    va_end(ap);
    return n;
}

int vprintf(const char *fmt, va_list ap) {
    char buf[512];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    size_t m = n < 0 ? 0 : ((size_t)n < sizeof(buf) ? (size_t)n : sizeof(buf) - 1);
    escreve(buf, m);
    return n;
}

int printf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vprintf(fmt, ap);
    va_end(ap);
    return n;
}

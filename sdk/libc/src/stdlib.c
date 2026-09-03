/* stdlib.c — nexo-libc minima: malloc/free/calloc/realloc.
 *
 * Arena: no primeiro uso (e quando esgota), pede um objeto de memoria ao kernel
 * (NEXO_SYS_MEMORY_CREATE + MEMORY_MAP) de ARENA_PAGES paginas e o fatia com uma free-list
 * de PRIMEIRA ADEQUACAO com divisao de blocos e fusao com o vizinho seguinte no free.
 * Cabecalho de 16 bytes por bloco (tamanho + magic); alinhamento de 16. Os handles das
 * arenas ficam abertos de proposito: o kernel recolhe tudo quando o processo sai. */
#include "../include/stdlib.h"
#include "../include/string.h"
#include "../../../abi/c/nexo.h"

#define ARENA_PAGES 64ULL /* 256 KiB */
#define MAGIC_LIVRE 0x4c56454eULL /* "NEVL" */
#define MAGIC_USO 0x5355454eULL /* "NEUS" */

typedef struct bloco {
    uint64_t magic;
    uint64_t tam; /* bytes do payload (multiplo de 16) */
    struct bloco *prox; /* na free-list (so quando livre) */
} bloco;

#define CABECALHO 16ULL /* magic + tam ficam nos primeiros 16 bytes; prox reusa o payload */

static bloco *livres = 0;

static uint64_t arena_nova(void) {
    nexo_ret r = nexo_syscall3(NEXO_SYS_MEMORY_CREATE, ARENA_PAGES, 0, 0);
    if (r.status != NEXO_STATUS_OK)
        return 0;
    nexo_ret m = nexo_syscall3(NEXO_SYS_MEMORY_MAP, r.value, 0, 0);
    if (m.status != NEXO_STATUS_OK)
        return 0;
    return m.value;
}

static void empurra_livre(bloco *b) {
    b->magic = MAGIC_LIVRE;
    b->prox = livres;
    livres = b;
}

void *malloc(size_t n) {
    if (n == 0)
        n = 16;
    n = (n + 15) & ~(size_t)15;
    /* primeira adequacao, com divisao quando sobra espaco para outro bloco */
    bloco **pp = &livres;
    while (*pp) {
        bloco *b = *pp;
        if (b->tam >= n) {
            if (b->tam >= n + CABECALHO + 16) {
                /* divide: o resto ocupa o lugar de `b` na lista */
                bloco *resto = (bloco *)((char *)b + CABECALHO + n);
                resto->magic = MAGIC_LIVRE;
                resto->tam = b->tam - n - CABECALHO;
                resto->prox = b->prox;
                *pp = resto;
                b->tam = n;
            } else {
                *pp = b->prox;
            }
            b->magic = MAGIC_USO;
            return (char *)b + CABECALHO;
        }
        pp = &b->prox;
    }
    /* sem bloco: nova arena inteira vira um bloco livre e tenta de novo */
    uint64_t base = arena_nova();
    if (!base)
        return 0;
    bloco *b = (bloco *)base;
    b->tam = ARENA_PAGES * 4096 - CABECALHO;
    empurra_livre(b);
    return malloc(n);
}

void free(void *p) {
    if (!p)
        return;
    bloco *b = (bloco *)((char *)p - CABECALHO);
    if (b->magic != MAGIC_USO)
        nexo_exit(134); /* abort: free invalido/duplo */
    empurra_livre(b);
}

void *calloc(size_t nmemb, size_t size) {
    if (size && nmemb > (size_t)-1 / size)
        return 0;
    size_t n = nmemb * size;
    void *p = malloc(n);
    if (p)
        memset(p, 0, n);
    return p;
}

void *realloc(void *p, size_t n) {
    if (!p)
        return malloc(n);
    bloco *b = (bloco *)((char *)p - CABECALHO);
    if (b->magic != MAGIC_USO)
        nexo_exit(134);
    if (b->tam >= n)
        return p;
    void *q = malloc(n);
    if (!q)
        return 0;
    memcpy(q, p, b->tam);
    free(p);
    return q;
}

/* sh.c — mini shell POSIX portado: `sh -c "cmd arg... [| cmd...]"` monta pipelines DE
 * VERDADE com os utilitarios do initrd (a convencao inteira: h0 = servico principal
 * duplicado para cada estagio, h1 = argv com uma mensagem, h2 = stdin, h3 = stdout).
 * O comando `x` vira o binario `x-c` do initrd. O stdin do proprio sh (se houver) alimenta
 * o primeiro estagio; o stdout do proprio sh (se houver) recebe o ultimo — entao um sh pode
 * viver dentro de um pipeline maior. Sai com o codigo do ultimo estagio (127 = nao lancou).
 * Sem variaveis, aspas, redirecionamentos ou job control: e o esqueleto que os utilitarios
 * pedem hoje — cresce conforme a demanda, como o resto da plataforma. */
#include "../../abi/c/nexo.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/string.h"

#define MAX_STAGES 4
#define MAX_WORDS 8

/* Par com a ponta de escrita ja fechada = EOF imediato (stdin de quem nao tem entrada). */
static uint32_t eof_channel(void) {
    uint32_t a = 0, b = 0;
    nexo_channel_create(&a, &b);
    nexo_handle_close(a);
    return b;
}

int main(int argc, char **argv) {
    if (argc < 3 || strcmp(argv[1], "-c") != 0) {
        puts("uso: sh -c \"cmd [args] [| cmd...]\"");
        return 2;
    }
    char *linha = argv[2];
    char *stages[MAX_STAGES];
    int ns = 0;
    stages[ns++] = linha;
    for (char *p = linha; *p; p++) {
        if (*p == '|') {
            if (ns == MAX_STAGES) {
                puts("sh: pipeline longo demais");
                return 2;
            }
            *p = 0;
            stages[ns++] = p + 1;
        }
    }
    /* A convencao de handles e POSICIONAL e vale no ARRANQUE: sondar os slots 0/2/3 depois
     * de criar ou transferir handles acharia ocupantes novos (os slots sao reutilizados).
     * Tudo o que o sh precisa saber dos proprios handles e capturado AQUI, antes de mexer
     * na tabela — a licao veio de um handle de processo entregue como "stdout" de um filho. */
    uint32_t rights, kind;
    uint32_t fs_rights = 0;
    int tem_fs = nexo_handle_info(0, &fs_rights, &kind) == NEXO_STATUS_OK;
    int tem_stdin = nexo_handle_info(2, &rights, &kind) == NEXO_STATUS_OK;
    int tem_stdout = nexo_handle_info(3, &rights, &kind) == NEXO_STATUS_OK;
    uint32_t entrada = 2; /* o stdin do sh alimenta o primeiro estagio */
    int tem_entrada = tem_stdin;
    uint32_t procs[MAX_STAGES];
    for (int i = 0; i < ns; i++) {
        char *palavras[MAX_WORDS];
        int nw = 0;
        for (char *p = stages[i]; *p;) {
            while (*p == ' ')
                p++;
            if (!*p)
                break;
            if (nw == MAX_WORDS) {
                puts("sh: argumentos demais");
                return 2;
            }
            palavras[nw++] = p;
            while (*p && *p != ' ')
                p++;
            if (*p)
                *p++ = 0;
        }
        if (nw == 0) {
            puts("sh: estagio vazio");
            return 2;
        }
        /* canal de argv com UMA mensagem (a convencao do crt0) */
        uint32_t argv_tx = 0, argv_rx = 0;
        if (nexo_channel_create(&argv_tx, &argv_rx) != NEXO_STATUS_OK)
            return 3;
        char bloco[256];
        size_t bn = 0;
        for (int w = 0; w < nw; w++) {
            size_t l = strlen(palavras[w]);
            if (bn + l + 1 > sizeof(bloco))
                return 3;
            memcpy(bloco + bn, palavras[w], l);
            bn += l;
            bloco[bn++] = 0;
        }
        uint32_t sem[1];
        nexo_channel_send(argv_tx, bloco, bn, sem, 0);
        nexo_handle_close(argv_tx);
        uint32_t ent = tem_entrada ? entrada : eof_channel();
        tem_entrada = 0;
        /* saida: pipe para o proximo estagio; o ultimo herda o stdout do sh (se houver) */
        uint32_t saida = 0;
        int tem_saida = 0;
        if (i + 1 < ns) {
            uint32_t pa = 0, pb = 0;
            if (nexo_channel_create(&pa, &pb) != NEXO_STATUS_OK)
                return 3;
            saida = pa;
            entrada = pb;
            tem_entrada = 1;
            tem_saida = 1;
        } else if (tem_stdout) {
            saida = 3;
            tem_saida = 1;
        }
        /* h0 do filho: o servico principal do sh, duplicado (cada estagio ganha o seu) */
        uint32_t h0;
        if (!tem_fs || nexo_handle_duplicate(0, fs_rights, &h0) != NEXO_STATUS_OK)
            h0 = eof_channel();
        char nome[32];
        int nn = snprintf(nome, sizeof(nome), "%s-c", palavras[0]);
        uint32_t hs[4] = {h0, argv_rx, ent, 0};
        uint64_t nh = 3;
        if (tem_saida) {
            hs[3] = saida;
            nh = 4;
        }
        uint64_t st = nexo_process_spawn(nome, (uint64_t)nn, 0, hs, nh, &procs[i]);
        if (st != NEXO_STATUS_OK) {
            printf("sh: nao lancou %s (status %lu)\n", nome, (unsigned long)st);
            return 127;
        }
    }
    int64_t code = 0;
    for (int i = 0; i < ns; i++) {
        int64_t c = 0;
        uint64_t st = nexo_process_wait(procs[i], &c);
        if (st != NEXO_STATUS_OK) {
            printf("sh: wait do estagio %d falhou (status %lu, handle %u)\n", i,
                   (unsigned long)st, procs[i]);
            c = 126;
        }
        if (i == ns - 1)
            code = c;
        nexo_handle_close(procs[i]);
    }
    return (int)code;
}

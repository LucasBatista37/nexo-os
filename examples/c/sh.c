/* sh.c — mini shell POSIX portado: `sh -c "cmd arg... [| cmd...]"` monta pipelines DE
 * VERDADE com os utilitarios do initrd (a convencao inteira: h0 = servico principal
 * duplicado para cada estagio, h1 = argv com uma mensagem, h2 = stdin, h3 = stdout).
 * O comando `x` vira o binario `x-c` do initrd. O stdin do proprio sh (se houver) alimenta
 * o primeiro estagio; o stdout do proprio sh (se houver) recebe o ultimo — entao um sh pode
 * viver dentro de um pipeline maior. Sai com o codigo do ultimo estagio (127 = nao lancou).
 *
 * Redirecoes: `< arq` (so no primeiro estagio) e `> arq` (so no ultimo). Sem concorrencia
 * no canal do fs: a entrada e BOMBEADA antes de qualquer spawn (arquivo -> fila do stdin) e
 * a saida e DRENADA depois de todos sairem (fila do pipe -> arquivo) — na v0 a saida
 * redirecionada cabe na fila do canal (64 mensagens; o flush da libc publica por linha).
 * Aspas simples ou duplas agrupam um argumento com espacos (`grep 'a b'`); o `|` continua
 * sendo separador de estagios mesmo entre aspas (limite da v0). Sem variaveis nem job
 * control: e o esqueleto que os utilitarios pedem hoje — cresce conforme a demanda. */
#include "../../abi/c/nexo.h"
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/string.h"
#include "../../sdk/libc/include/unistd.h"

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
    char *red_saida = 0; /* arquivo do `>` do ultimo estagio; drenado apos os waits */
    uint32_t red_saida_rx = 0;
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
            if (*p == '\'' || *p == '"') { /* aspas: um argumento com espacos */
                char q = *p++;
                palavras[nw++] = p;
                while (*p && *p != q)
                    p++;
                if (!*p) {
                    puts("sh: aspas sem fechar");
                    return 2;
                }
                *p++ = 0;
            } else {
                palavras[nw++] = p;
                while (*p && *p != ' ')
                    p++;
                if (*p)
                    *p++ = 0;
            }
        }
        if (nw == 0) {
            puts("sh: estagio vazio");
            return 2;
        }
        /* redirecoes: retira `< arq`/`> arq` (ou `<arq`/`>arq`) das palavras */
        char *red_in = 0;
        int w = 0;
        for (int k = 0; k < nw; k++) {
            if (palavras[k][0] == '<' || palavras[k][0] == '>') {
                char tipo = palavras[k][0];
                char *arq = palavras[k] + 1;
                if (!*arq) {
                    if (k + 1 >= nw) {
                        puts("sh: redirecao sem arquivo");
                        return 2;
                    }
                    arq = palavras[++k];
                }
                if (tipo == '<') {
                    if (i != 0) {
                        puts("sh: < so no primeiro estagio");
                        return 2;
                    }
                    red_in = arq;
                } else {
                    if (i + 1 != ns) {
                        puts("sh: > so no ultimo estagio");
                        return 2;
                    }
                    red_saida = arq;
                }
            } else {
                palavras[w++] = palavras[k];
            }
        }
        nw = w;
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
        if (red_in) { /* bomba de entrada ANTES de qualquer spawn: arquivo -> fila do stdin */
            nexo_libc_use_fs(0);
            int fd = open(red_in, O_RDONLY);
            if (fd < 0) {
                puts("sh: nao abriu a entrada");
                return 1;
            }
            uint32_t ia = 0, ib = 0;
            if (nexo_channel_create(&ia, &ib) != NEXO_STATUS_OK)
                return 3;
            char pedaco[256];
            ssize_t r;
            uint32_t sem2[1];
            while ((r = read(fd, pedaco, sizeof(pedaco))) > 0)
                nexo_channel_send(ia, pedaco, (uint64_t)r, sem2, 0);
            close(fd);
            nexo_handle_close(ia);
            ent = ib; /* substitui o stdin do estagio (o anterior fecha no exit do sh) */
        }
        /* saida: pipe para o proximo estagio; o ultimo herda o stdout do sh (se houver);
         * `>` cria um pipe cuja outra ponta o sh drena para o arquivo depois dos waits */
        uint32_t saida = 0;
        int tem_saida = 0;
        if (i + 1 == ns && red_saida) {
            uint32_t pa = 0, pb = 0;
            if (nexo_channel_create(&pa, &pb) != NEXO_STATUS_OK)
                return 3;
            saida = pa;
            red_saida_rx = pb;
            tem_saida = 1;
        } else if (i + 1 < ns) {
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
    if (red_saida) { /* drena a fila do pipe para o arquivo (todos ja sairam) */
        nexo_libc_use_fs(0);
        int fd = open(red_saida, O_WRONLY | O_CREAT | O_TRUNC);
        if (fd < 0) {
            puts("sh: nao abriu a saida");
            return 1;
        }
        static char sobra[4096];
        uint64_t nb = 0, nh = 0;
        uint32_t hs2[2];
        while (nexo_channel_recv(red_saida_rx, sobra, sizeof(sobra), hs2, 2, &nb, &nh)
               == NEXO_STATUS_OK) {
            if (nb && write(fd, sobra, (size_t)nb) != (ssize_t)nb) {
                puts("sh: escrita curta na saida");
                close(fd);
                return 1;
            }
        }
        close(fd);
        nexo_handle_close(red_saida_rx);
    }
    return (int)code;
}

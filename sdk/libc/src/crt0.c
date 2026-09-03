/* crt0.c — runtime de entrada da nexo-libc para programas com main(argc, argv).
 *
 * Convencao de argv do Nexo: o criador entrega um CANAL no handle 1 com UMA mensagem ja
 * enviada — os argumentos separados por '\0' (argv[0] incluso). Sem o canal (ou sem
 * mensagem), main recebe argc = 0. O handle 0 fica livre para o servico principal do
 * programa (ex.: o canal do nexo.fs para a camada de arquivos). */
#include "../../../abi/c/nexo.h"

extern int main(int argc, char **argv);
extern void nexo_run_ctors(void) __attribute__((weak));

#define ARGV_MAX 16
#define ARGS_BYTES 512

void _start(uint64_t arg) {
    (void)arg;
    if (nexo_run_ctors)
        nexo_run_ctors();
    static char bloco[ARGS_BYTES];
    static char *argv[ARGV_MAX + 1];
    int argc = 0;
    uint64_t nb = 0, nh = 0;
    uint32_t hs[2];
    if (nexo_channel_recv(1, bloco, ARGS_BYTES - 1, hs, 2, &nb, &nh) == NEXO_STATUS_OK && nb > 0) {
        bloco[nb] = 0;
        char *p = bloco;
        while (argc < ARGV_MAX && p < bloco + nb) {
            argv[argc++] = p;
            while (*p)
                p++;
            p++;
        }
    }
    argv[argc] = 0;
    nexo_exit(main(argc, argv));
}

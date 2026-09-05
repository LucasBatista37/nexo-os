/* grep.c — utilitario POSIX portado: imprime as linhas que CASAM com o padrao. Regex minima
 * (o casador de Pike): `^` inicio, `$` fim, `.` qualquer caractere, `c*` zero ou mais; o
 * resto e literal — um padrao sem metacaracteres e a busca de substring de antes. Sai 0 se
 * achou alguma linha, 1 se nenhuma — a semantica classica de grep.
 * Uso: grep <padrao> [arquivo]  (sem arquivo, le o stdin). */
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/unistd.h"

#define LINHA_MAX 512

static char linha[LINHA_MAX];
static size_t ln;
static int achou;

static int casa_aqui(const char *re, const char *texto);

/* `c*` seguido de `re`: tenta o resto a cada prefixo de repeticoes de `c` (ou `.`). */
static int casa_estrela(int c, const char *re, const char *texto) {
    do {
        if (casa_aqui(re, texto))
            return 1;
    } while (*texto != 0 && (*texto++ == c || c == '.'));
    return 0;
}

/* Casa `re` no inicio de `texto`. */
static int casa_aqui(const char *re, const char *texto) {
    if (re[0] == 0)
        return 1;
    if (re[1] == '*')
        return casa_estrela(re[0], re + 2, texto);
    if (re[0] == '$' && re[1] == 0)
        return *texto == 0;
    if (*texto != 0 && (re[0] == '.' || re[0] == *texto))
        return casa_aqui(re + 1, texto + 1);
    return 0;
}

/* Casa `re` em qualquer posicao de `texto` (`^` ancora no inicio). */
static int casa(const char *re, const char *texto) {
    if (re[0] == '^')
        return casa_aqui(re + 1, texto);
    do {
        if (casa_aqui(re, texto))
            return 1;
    } while (*texto++ != 0);
    return 0;
}

static void fecha_linha(const char *padrao) {
    linha[ln] = 0;
    if (ln && casa(padrao, linha)) {
        achou = 1;
        printf("%s\n", linha);
    }
    ln = 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        puts("uso: grep <padrao> [arquivo]");
        return 2;
    }
    int fd = 0;
    if (argc > 2) {
        nexo_libc_use_fs(0);
        fd = open(argv[2], O_RDONLY);
        if (fd < 0) {
            puts("grep: nao abriu o arquivo");
            return 2;
        }
    }
    char buf[512];
    ssize_t n;
    while ((n = read(fd, buf, sizeof(buf))) > 0) {
        for (ssize_t i = 0; i < n; i++) {
            if (buf[i] == '\n') {
                fecha_linha(argv[1]);
            } else if (ln + 1 < LINHA_MAX) {
                linha[ln++] = buf[i];
            }
        }
    }
    fecha_linha(argv[1]); /* ultima linha sem \n conta */
    if (fd != 0)
        close(fd);
    return achou ? 0 : 1;
}

/* echo.c — utilitario POSIX portado: imprime os argumentos separados por espaco. Exercita a
 * convencao de argv com VARIOS argumentos de uma vez. Uso: echo [palavras...] */
#include "../../sdk/libc/include/stdio.h"

int main(int argc, char **argv) {
    for (int i = 1; i < argc; i++)
        printf(i + 1 < argc ? "%s " : "%s", argv[i]);
    printf("\n");
    return 0;
}

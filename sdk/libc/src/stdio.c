/* stdio.c — nexo-libc minima: puts via log do kernel. */
#include "../include/stdio.h"
#include "../include/string.h"
#include "../../../abi/c/nexo.h"

int puts(const char *s) {
    nexo_log(s, strlen(s));
    return 0;
}

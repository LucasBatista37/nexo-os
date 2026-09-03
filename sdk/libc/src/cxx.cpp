/* cxx.cpp — runtime C++ minimo da nexo-libc (sem exceptions, sem RTTI): new/delete sobre o
 * malloc, guarda de virtual pura e os simbolos que o clang++ emite em freestanding. */
#include "../include/stdlib.h"
#include "../../../abi/c/nexo.h"

void *operator new(size_t n) { return malloc(n); }
void *operator new[](size_t n) { return malloc(n); }
void operator delete(void *p) noexcept { free(p); }
void operator delete[](void *p) noexcept { free(p); }
void operator delete(void *p, size_t) noexcept { free(p); }
void operator delete[](void *p, size_t) noexcept { free(p); }

extern "C" void __cxa_pure_virtual(void) { nexo_exit(133); }

/* construtores globais: o lld define os limites do .init_array quando referenciados */
typedef void (*ctor_t)(void);
extern "C" ctor_t __init_array_start[] __attribute__((weak));
extern "C" ctor_t __init_array_end[] __attribute__((weak));

extern "C" void nexo_run_ctors(void) {
    for (ctor_t *c = __init_array_start; c < __init_array_end; c++)
        (*c)();
}

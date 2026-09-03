/* hello-cpp.cpp — primeiro processo em C++ do Nexo OS (freestanding, sem exceptions/RTTI):
 * construtor global via .init_array, classe com metodo virtual (vtable), new/delete sobre o
 * heap da nexo-libc. Asserts derrubam o processo com codigos distintos. */
#include "../../abi/c/nexo.h"
#include "../../sdk/libc/include/string.h"

extern "C" void nexo_run_ctors(void);

static int global_ctor_rodou = 0;
struct Init {
    Init() { global_ctor_rodou = 42; }
};
static Init init_global;

struct Forma {
    virtual ~Forma() {}
    virtual int lados() const = 0;
};
struct Quadrado : Forma {
    int lados() const override { return 4; }
};
struct Triangulo : Forma {
    int lados() const override { return 3; }
};

extern "C" void _start(uint64_t) {
    nexo_run_ctors();
    if (global_ctor_rodou != 42)
        nexo_exit(2);
    Forma *formas[2] = {new Quadrado(), new Triangulo()};
    int soma = 0;
    for (int i = 0; i < 2; i++)
        soma += formas[i]->lados(); /* despacho virtual de verdade */
    delete formas[0];
    delete formas[1];
    if (soma != 7)
        nexo_exit(3);
    static const char msg[] = "ola do C++ (ctors globais, vtables e new/delete da nexo-libc)";
    nexo_log(msg, sizeof(msg) - 1);
    nexo_exit(0);
}

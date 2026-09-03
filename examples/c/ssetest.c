/* ssetest.c — FPU/SSE em modo usuario (compilado COM SSE, ao contrario do hello): faz
 * aritmetica double nos registradores XMM, cede a CPU centenas de vezes e confere que o
 * proprio estado sobrevive as trocas de contexto — dois processos com padroes distintos
 * provam o isolamento (FXSAVE/FXRSTOR por thread no kernel). */
#include "../../abi/c/nexo.h"

void _start(uint64_t arg) {
    volatile double base = (double)(arg + 1);
    double acc = base * 1.5;
    double keep = acc;
    for (int i = 0; i < 400; i++) {
        nexo_yield();
        /* usa e confere os XMM a cada volta: qualquer vazamento de outro processo quebra */
        acc = acc + base;
        keep = keep + base;
        if (acc != keep) {
            nexo_exit(2);
        }
    }
    double esperado = base * 1.5 + base * 400.0;
    if (acc < esperado - 0.001 || acc > esperado + 0.001) {
        nexo_exit(3);
    }
    static const char msg[] = "sse ok: aritmetica XMM sobreviveu as trocas de contexto";
    nexo_log(msg, sizeof(msg) - 1);
    nexo_exit(0);
}

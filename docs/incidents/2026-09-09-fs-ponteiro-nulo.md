# Incidente aberto: o `fs` morre com ponteiro nulo, raramente, no cenário `storage`

**Estado**: parcialmente explicado (2026-09-09). Uma terceira ocorrência mostrou uma causa
diferente das duas primeiras — e uma falha de projeto pior, já corrigida (bloco 119).

## Sintomas

Duas manifestações, ambas no serviço `fs`, sempre no **segundo boot** do cenário `storage`
(o boot que encontra o volume já formatado) e sempre com o teste `user_install` a reprovar
logo a seguir (`instalador saiu com 131`):

1. `instrucao invalida em modo usuario` (#UD) — 2026-09-09, antes do diagnóstico existir.
2. `falta de pagina em modo usuario` (#PF) com `err=0x6` (escrita, página ausente) e
   **`cr2 = 0x20`** — ou seja, escrita em `null + 0x20`.

Na segunda ocorrência o `rip` caiu no deslocamento `0x400ffe` do ELF do `fs`, dentro de
`<nexofs::Fs<nexo_fs::ChanDisk>>::write_entry`, na sequência
`movq $0x0, 0x38(%rcx)` … `movq $0x0, 0x20(%rcx)` — uma limpeza de estrutura com `%rcx` nulo
(provavelmente o ponteiro de retorno oculto de um `Result` grande).

## Por que não é um bug do `nexofs`

A biblioteca declara `#![forbid(unsafe_code)]` e o serviço `fs` não tem `unsafe` além do
`#[unsafe(no_mangle)]` do `_start`. Um ponteiro nulo ali não pode nascer de Rust seguro: ou
veio de memória corrompida por fora, ou o processo foi carregado/retomado com estado errado.

## Suspeitos, em ordem

1. **Carregamento PIE** (bloco 102): relocações `R_X86_64_RELATIVE` aplicadas pelo kernel
   depois de copiar os segmentos. Se alguma base aleatória expusesse um caso de borda, a
   falha seria rara e dependente do sorteio — o que casa com a raridade. Contra: o mesmo
   binário roda centenas de vezes por varredura sem falhar.
2. **Corrida no kernel** entre criar e destruir processos sob carga (o cenário `storage`
   cria e mata muitos). O bug do bloco 94 tinha essa forma.
3. Corrupção de pilha do processo (nada aponta para isso: `rsp` estava bem dentro da pilha).

## Instrumentação já no lugar

- Bloco 111: `#UD`/`#GP` de usuário registram 16 bytes em torno do `rip`.
- Bloco 117: toda exceção de usuário registra a **base do código** e `rip - base` (o
  deslocamento a procurar no ELF), além de `rax`, `rcx`, `rdx`, `rsi`, `rdi`.

## O que fazer quando reaparecer

1. Guardar o log inteiro do cenário (`build/logs/storage*.log`).
2. Converter `rip - base` no símbolo com `llvm-objdump -d` sobre
   `services/target/x86_64-unknown-none/release/nexo-fs`.
3. Comparar o registrador que carrega o ponteiro (`rcx` na ocorrência conhecida) com o que a
   função deveria ter recebido; se for o ponteiro de retorno, o problema está a montante, no
   chamador — e aí vale desligar o PIE só do `fs` (`relocation-model=static`) para ver se o
   fantasma some, isolando a hipótese 1.

## Terceira ocorrência (2026-09-09): erro de E/S, não corrupção

Com o diagnóstico do bloco 117 no lugar, a falha voltou a aparecer no mesmo teste
(`user_install`, código 131) — mas o `fs` **não** tinha morrido de exceção:

```
fs: AVISO: volume inutilizavel (Io); formatando NexoFS v0 (volume de teste)
fs: falha: send
process: pid 294 'fs' saiu com 30
```

A leitura do volume voltou `Io`. A carga do host estava em **6,4** (o stress de 7 dias ocupa
quatro vCPUs e havia uma varredura em curso): sob saturação, um pedido ao virtio-blk estoura
o tempo do driver e volta erro. A execução seguinte, com a mesma imagem, passou 128/128.

**A falha de projeto**: o `fs` tratava *qualquer* erro de montagem como "volume inutilizável"
e **formatava** o volume de teste. Um erro de E/S transitório destruía dados íntegros. Desde
o bloco 119, `Io` na montagem faz o serviço sair com código 33 sem tocar no disco; só
estrutura inválida em disco autoriza reformatar.

As duas primeiras ocorrências (`#UD` e escrita em `null + 0x20`) continuam **sem explicação**
e podem ter outra raiz; o plano de coleta acima segue valendo para elas.

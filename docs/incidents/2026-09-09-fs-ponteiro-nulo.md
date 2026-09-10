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

## Quarta ocorrência (2026-09-09, varredura do bloco 124) — e a causa raiz

Cenário `storage`, segunda execução, teste `user_install`. O `fs` morreu **na primeira
instrução**:

```
process: 'fs' pid 298 entry 0xe806254b0 (87 quadros) thread 417
trap: pid 298 'fs' Page Fault (#14) em rip=0xe806254b0 rsp=0x7ffff1533ff8 err=0x14 cr2=0xe806254b0
trap: pid 298 base do codigo 0xe80221000 (rip-base = 0x4044b0); rax=0x0 rcx=0x0 rdx=0x0 rsi=0x0 rdi=0x0
```

`rip == entry` e `cr2 == rip`: o processo faltou ao buscar a **própria primeira instrução**.
O código de erro `0x14` decodifica como **busca de instrução** (bit 4) numa página **não
presente** (bit 0 = 0) em **modo usuário** (bit 2). Não é um bug do `fs` — é o espaço de
endereçamento do processo que não está lá quando ele começa a executar.

Log preservado: `build/logs/incidente-fs-2026-09-09-storage2.log`.

Hipótese descartada no caminho: a contagem de quadros do `fs` variar (87/88/89) no mesmo boot
não indica página perdida — varia com a base aleatória do ASLR, que muda quantas tabelas
intermediárias são necessárias.

### Hipótese levantada — e REFUTADA pela medição

`kernel/src/sched.rs` evitava a troca de CR3 comparando o **quadro físico** da PML4:

```rust
if cpu::read_cr3() & 0x000f_ffff_ffff_f000 != target {
    unsafe { cpu::write_cr3(target) };
}
```

É a otimização clássica ("mesmo espaço de endereçamento, não precisa esvaziar a TLB"), e ela
está errada quando um quadro de PML4 é **liberado e reciclado**: o processo A termina, o
`Drop` do `AddressSpace` devolve o quadro raiz ao alocador, e o processo B nasce recebendo o
**mesmo quadro** (o alocador de bitmap devolve o quadro recém-liberado). Uma CPU que rodou A
tem `CR3` numericamente igual ao alvo de B, não recarrega, e segue com as traduções em cache
de um espaço que já morreu — TLB e caches de estrutura de paginação incluídos.

O resultado depende do que estava em cache: uma tradução intermediária obsoleta cobrindo a
faixa nova produz "não presente" na primeira busca de instrução (esta ocorrência), e uma
tradução obsoleta *presente* faz a CPU executar memória alheia — o que explica as ocorrências
anteriores (`#UD` = instrução inválida em memória que não é o código do `fs`; escrita em
`null + 0x20` dentro de `write_entry` = código errado a correr).

Se fosse isso, a gravidade passaria da queda: traduções de um espaço morto ainda válidas na
CPU deixariam um processo **alcançar memória de outro**.

**Não é isso.** A hipótese foi medida antes de virar conclusão, e caiu:

1. A **premissa** confirma-se: uma sonda que cria e destrói espaços de endereçamento viu o
   quadro da PML4 recém-liberada ser devolvido pela alocação seguinte **7 vezes em 7** — o
   alocador de bitmap recicla mesmo, e de imediato.
2. O **caso perigoso**, porém, nunca ocorre: um contador no escalonador, incrementado
   exatamente quando uma CPU vai carregar um espaço **diferente** cujo quadro de PML4 é
   **igual** ao que já está em CR3, marcou **0 em 467 processos criados** num boot completo.

O motivo é uma invariante que a versão antiga explorava sem dizer: uma CPU nunca fica com o
CR3 de um espaço morto. Enquanto uma thread do processo corre, o `Arc` mantém o espaço vivo; e
ao trocar para a thread ociosa (ou para qualquer thread sem processo) o escalonador já carrega
a PML4 do kernel. Quando o espaço finalmente morre, nenhuma CPU o tem em CR3.

A comparação passou a ser feita por **identidade** do espaço mesmo assim (bloco 125), como
endurecimento: comparar endereços de PML4 só é correto por causa daquela invariante não
declarada, e um dia alguém a quebra — adiando a destruição do espaço, adotando troca preguiçosa
de CR3 — sem que nada acuse. O contador continua no `[SCHED]` para que a mudança seja
observável: hoje ele é zero, e o dia em que não for é o dia em que a invariante caiu.

### O que fica para a próxima ocorrência

A causa continua **desconhecida**. O que se sabe: o processo não chega a executar a primeira
instrução, e a página do seu ponto de entrada não está presente. Instrumentação acrescentada no
bloco 125: quando um processo de usuário sofre falta de página, o kernel percorre as tabelas do
próprio processo e imprime as entradas de cada nível (PML4/PDPT/PD/PT) para o endereço da
falta. Na próxima vez saberemos se a tradução some num nível intermediário, se a entrada existe
sem o bit de presença, ou se o endereço nunca foi mapeado.

## Tentativa de reprodução dirigida (2026-09-10)

`tools/nexo-repro storage 12` — doze execuções do cenário onde a quarta ocorrência apareceu
(cada execução faz dois boots, logo **24 boots**), com toda a instrumentação nova ligada:
tradução nível a nível na falta de página (bloco 125) e preservação do log de qualquer cenário
que falhe (bloco 135).

**Não reproduziu: 0 falhas em 12 execuções, 40 minutos.**

O denominador é o que dá significado ao resultado. Historicamente a falha apareceu uma vez em
cerca de dez varreduras completas; 24 boots sem recorrência é compatível com uma taxa dessa
ordem e **não** permite concluir que o problema desapareceu. Serve para dois efeitos:

1. limita a frequência por cima — não é uma falha de uma em duas ou uma em cinco;
2. confirma que a ferramenta e a instrumentação estão prontas para quando acontecer, o que era
   metade do problema nas três primeiras ocorrências, em que a evidência se perdeu.

Vale notar o que mudou no kernel desde a quarta ocorrência: identidade do espaço de
endereçamento (125), SMEP/SMAP (126), teto de threads (128) e mensagens sem alocação (134).
Nenhum deles é candidato a explicação: o contador de reciclagem de PML4 do bloco 125 mede
exatamente o caso que se suspeitava e continua em **zero**, e os outros não tocam na carga de
processos. A causa continua desconhecida.

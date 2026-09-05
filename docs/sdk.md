# SDK do Nexo OS — como escrever um aplicativo

Plano §Fase 6 ("publicar SDK Rust", "criar documentação e exemplos", "criar gerador de projeto").
O SDK é Rust `no_std` (edition 2024, Rust 1.98, alvo `x86_64-unknown-none`), composto pelos crates
do repositório:

| Crate | Para quê |
|---|---|
| `nexo-sys` | syscalls (canais, memória compartilhada, processos) |
| `nexo-rt` | runtime mínimo (`log!`, panic handler) |
| `nexo-proto` | protocolos NXIP gerados da IDL (`idl/*.idl`), incl. `nexo.wm` |
| `nexo-gfx` | desenho 2D (`Surface`, cores, texto) |
| `nexo-ui` | toolkit (temas, `Label`/`Button`/`VStack`/`Nav`) |
| `nexo-pkg` | ler/validar pacotes NEXOPKG1 (para instaladores) |
| `nexo-inst` | instalação transacional, revogação e coleta de versões |
| `nexo-img` | decodificar imagens (PPM P6; entrada hostil sem pânico) |
| `nexo-cal` | datas civis (epoch ↔ ano/mês/dia, dia da semana) |
| `nexo-textgrid` | grade de texto (quebra, rolagem, `scrolled`) — a do terminal, do editor e do visor |
| `sdk/libc` (**C**) | nexo-libc: string/stdio (`printf`)/stdlib (heap, `atoi`)/fcntl+unistd (fd)/dirent/`rename`/`mkdir` |

## Começando

```text
tools/nexo-new meuapp          # gera services/meuapp/ já registrado no build
(cd services && cargo build --release -p nexo-meuapp)
tools/nexo-pack build --manifest services/meuapp/manifest.txt \
    --out meuapp.npk services/target/x86_64-unknown-none/release/nexo-meuapp
```

## O contrato de um aplicativo

1. **Handle 0 = canal com o orquestrador** (o shell). É por ele que o app recebe a sua sessão
   `nexo.wm` (mensagem `"sess"` com um handle) e é o **cordão de vida**: se fechar, o app encerra.
   (Sem observá-lo, app e compositor podem esperar um pelo outro no encerramento.)
2. **Janela**: `create_surface` devolve um `MemoryObject`; mapeie com `memory_map` e desenhe nos
   pixels (RGBX8888) com `nexo-gfx`/`nexo-ui`; `commit` recompõe. `set_title` dá nome à janela
   (Faixa de Atividades e leitores de tela usam).
3. **Entrada**: cliques chegam como evento `pointer{surface,x,y}` (coordenadas locais) e teclas
   como `key{surface,code,value}` — só quando a janela tem o foco (ou a captura). Respostas de RPC
   e eventos compartilham o canal da sessão: consuma com tolerância (pule eventos ao esperar uma
   resposta, guarde-os se precisar).
4. **Mediações**: clipboard só com a posse da entrada; `notify` para avisos; `grab` para entrada
   sensível; `prefs` para respeitar a redução de movimento.
5. **Manifesto** (`manifest.txt`): declare em `perms=` o que o app precisa. O lançador com
   consentimento (`services/lanc`) mostra as permissões declaradas ao usuário **antes** de
   executar: *Permitir* concede exatamente o que foi declarado; *Negar* significa que o app nem
   roda. Sem declarar, a capacidade não existe (negação-por-omissão).
6. **Relógio e introspecção**: `nexo_sys::debug_info(sel)` — 0 CPUs, 1 uptime ms, 2 syscalls do
   processo, 3 handles, 4 processos vivos, 5/6 quadros físicos livres/utilizáveis, 7 segundos
   Unix (UTC) do RTC (0 = sem relógio). Datas com `nexo-cal`.
7. **Arquivos**: um canal `nexo.fs` chega pelo orquestrador quando o app tem essa capacidade
   (ex.: o visor recebe `"abre <caminho>"` + o canal). Instalações ficam em
   `/apps/<nome>.v<N>/`; o ponteiro `/apps/<nome>.cur` guarda `v<N>` (com o prefixo `v`).

## Programas em C, a nexo-libc e a convenção de processos

O toolchain C/C++ é `clang --target=x86_64-unknown-none-elf` + `rust-lld`; `make toolchain`
empacota o sysroot (`include/nexo/`, headers da libc, `lib/libnexo.a`, wrappers `nexo-cc`/
`nexo-c++`) e compila um programa de fora como auto-teste. Os headers `abi/c/nexo.h` (syscalls,
canais, handles, spawn/wait) e `abi/c/proto/fs.h` (o `nexo.fs` gerado do MESMO IDL do Rust) são
a ABI; SSE é opt-in por programa (o kernel salva o estado por thread).

Um programa C com `main(argc, argv)` (linka o `crt0.c` da libc) segue a **convenção de
processos** de quatro handles — posicional e válida **no arranque**:

| Handle | Papel |
|---|---|
| 0 | serviço principal (ex.: o canal `nexo.fs`; `nexo_libc_use_fs(0)` liga a camada fd a ele) |
| 1 | argv: UMA mensagem já enviada, argumentos separados por `\0` (argv[0] incluso); sem ela, `argc = 0` |
| 2 | stdin (opcional): cada mensagem é um pedaço; fila vazia com a outra ponta viva bloqueia; `PeerClosed` = EOF |
| 3 | stdout (opcional): bytes fiéis por canal (`\n` incluso) — um pipe; sem ele, a saída vai ao log do kernel em linhas |

Regras que os testes pagaram para aprender: o crt0 fixa stdin/stdout **antes** do `main`
(`nexo_stdio_init`/`nexo_fd_init`) porque um programa que cria ou transfere handles reutiliza os
slots 2/3 — sondar tarde acha o ocupante novo; a saída sem `\n` final é publicada no `exit`
(`nexo_stdio_flush`); a sonda do stdout é uma mensagem vazia (consumidores ignoram).

Onze utilitários portados vivem em `examples/c/` (`wc cat ls cp rm mv echo head mkdir grep sort`)
e o mini `sh` (`sh -c "cmd [args] [| cmd...]"`, com `< arq`/`> arq`, aspas e até 4 estágios)
monta pipelines de verdade em ring 3: cria os canais, manda o argv de cada estágio, duplica o
h0 para cada um, lança `<cmd>-c` do initrd e espera. Nunca use o serviço do h0 enquanto um
filho segura o handle duplicado (as respostas iriam ao leitor errado): o sh bombeia a entrada
antes do spawn e drena a saída depois dos waits. No shell de diagnóstico, um nome não-builtin
roda o utilitário do initrd com o vfs duplicado (`wc /disk/arquivo.txt`).

## Exemplos no repositório

- `services/calc` — app completo: botões `nexo-ui` + eventos `pointer` + clipboard mediado.
- `services/greeter` — entrada sensível com captura (`grab`).
- `services/shellui` — o shell: privilégios (`surface_info`/`activate`), broker de sessões.
- `services/term` — uma janela que **serve** outro protocolo (`nexo.console`): o shell de texto
  roda dentro dela sem mudanças; grade de glifos `nexo-font` com rolagem.
- `services/visor` — abre um arquivo (`nexo.fs`), decodifica (`nexo-img`) e apresenta.
- `services/monitor` — estatísticas reais do kernel (`debug_info`) com *heartbeat* visível.
- `services/agenda` — o mês corrente com a data real (`debug_info` 7 + `nexo-cal`).
- `services/config` — toggles com efeito real nas mediações (`set_reduce_motion`/`set_dnd`).
- `services/lanc` — o lançador com consentimento: leia-o para entender o que o SEU manifesto
  causa na prática.
- `examples/c/hello.c`, `fstest.c`, `hello-cpp.cpp` — a libc, o fs pelo protocolo gerado e o
  runtime C++; `examples/c/sh.c` — a convenção de processos inteira num só programa.

## Depuração

`nexo_rt::log!` sai na serial (e no console). Os cenários de `tools/test-qemu` mostram como testar
um app de ponta a ponta com entrada sintética (`utest` modos 43–45 são drivers de exemplo; os
modos 50–57 testam os apps deste repositório e são o melhor modelo para o teste do seu).

Lições que os testes deste repositório pagaram para aprender (herde-as de graça):

- A saída composta é memória compartilhada: leituras de pixel concorrentes a recomposições devem
  **esperar convergência** (releia até o valor esperado), nunca ler uma vez.
- Pixels que **rolam** (terminal) não servem de sinal de encerramento — sincronize fim de vida
  por mensagens no canal do orquestrador (ex.: o term envia `"fim"`).
- O disco de dados persiste entre boots: testes de instalação devem ser **idempotentes**
  (versões relativas, nunca absolutas) e usar prefixos exclusivos (`/fm-teste`, `/sh-teste.txt`);
  um passo de faxina tolerante no início absorve restos de um boot interrompido.
- Ordem de saída não é testável por marcadores soltos: verifique por **composição**
  (`sort | head -n 1` só emite a primeira linha se a ordenação aconteceu).
- Ao prever rolagem numa grade de texto, conte linhas de **grade** (com quebra), não linhas lógicas.

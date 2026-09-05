# POSIX na prática e os apps fechando — 2026-09-03 a 2026-09-05

Dezesseis blocos em `main` (CI verde em cada um, SHA conferido a cada watch). Dois arcos:

- **Fase 6/7 — "portar toolchain e utilitários POSIX prioritários"**: de um `wc` isolado a um
  mini `sh` que monta pipelines de três estágios, com redireções, onze utilitários portados e
  os mesmos utilitários rodando dentro do shell de diagnóstico. Tudo sobre a **nexo-libc**
  própria (printf, stdin/stdout por canal, dirent, rename) e uma **convenção de processos**
  de quatro handles que nasceu aqui: h0 = serviço principal, h1 = argv (uma mensagem,
  `\0`-separada), h2 = stdin (PeerClosed = EOF), h3 = stdout (bytes fiéis).
- **Fase 5/6 — pendências dos apps gráficos**: editor (cursor livre + rolagem → item `[x]`),
  gerenciador de arquivos (voltar + rolagem por páginas), Configurações (tema do sistema em
  runtime, `nexo.wm` v1.20).

Suíte de boot: 99 → 116 testes; roadmap 179 → 180 itens concluídos (70 parciais).

| Bloco | Entrega | Evidência |
|---|---|---|
| 65 | Convenção de argv (crt0 da libc, canal no handle 1) + `wc` | `user_wc` (100º); marcador `0 5 26 /c-arquivo.txt` |
| 66 | `printf`/`vsnprintf` próprios; saída em linhas com flush no crt0 | asserts por `strcmp` no hello-c; `wc` migrado |
| 67 | `write(1/2)`, `dirent.h` (opendir/readdir sobre o `list`), `cat`, `ls` | marcadores de CONTEÚDO no cenário boot |
| 68 | `unlink`, `cp`, `rm` | `cp` conferido rodando o `wc` na cópia; dupla cp+rm idempotente |
| 69 | stdin por canal (handle 2; EOF por PeerClosed); `wc`/`cat` sem argumento | dados em dois pedaços, ponta fechada antes do spawn |
| 70 | `rename` crash-safe no NexoFS (v1.1, método 11) + `mv` | corte em cada escrita no host achou e fechou um buraco do mount |
| 71 | `echo`, `head -n`, `mkdir` (sys/stat.h), `atoi` | "cauda" não sai do `head -n 2` |
| 72 | stdout por canal (handle 3): **pipeline** `cat \| wc` entre processos | EOF em cascata; "2 3 13" |
| 73 | mini `sh -c "a \| b \| c"`: spawn de ring 3, h0 duplicado por estágio | "1 3 12"; lição da sonda posicional no arranque |
| 74 | `grep` (strstr) e `sort` | sort provado por composição: `sort \| head -n 1` |
| 75 | redireções `<`/`>` no sh (bomba antes do spawn, dreno após os waits) | `echo … > arq` conferido pelo `wc`; `sort < arq \| wc` |
| 76 | utilitários POSIX no shell de diagnóstico (vfs duplicado, stdout bombeado à console) | cenário `shell`: `wc /disk/sh76.txt` |
| 77 | cursor livre no editor (setas, inserção/remoção no meio) | `ola\nmeu mundo` relido de fora |
| 78 | gerenciador: `..` na linha 0, páginas de 4, `+N`/`<<` | volta pelo `..`, pagina até abrir a 5ª de seis entradas |
| 79 | tema do sistema em runtime (`set_theme`/`prefs.theme`, `nexo.wm` v1.20) | a janela das Configurações repinta clara/escura, por pixel |
| 80 | rolagem no editor (`scrolled` na textgrid; janela [topo, topo+6)) → item do editor `[x]` | sete linhas de grade, rola e volta com o cursor |

## Segundo dia (2026-09-05): blocos 81–88

| Bloco | Entrega | Evidência |
|---|---|---|
| 81 | documentos básicos no visor (texto numa grade 6×4) → item do visualizador `[x]` | segundo visor por `process_spawn` do driver; glifos por pixel |
| 82 | aspas no `sh` (`grep 'nexo dentro' \| wc`) | sem aspas o grep sairia 2; "1 3 16" |
| 83 | regex mínima no `grep` (`^ $ . *`, o casador de Pike) | `^ab*c$` deixa passar 3 de 4 linhas: "3 3 13" |
| 84 | permissão temporária no lançador ("Permitir por tempo") | a janela "calc" some sozinha após "expirou" |
| 85 | nomes longos no gerenciador (`~` de truncamento) → item `[x]` | "nome-co~" por pixel; `abrir` com o caminho inteiro |
| 86 | repositório de pacotes **em rede** (host publica `.npk` por HTTP; guest baixa, grava em `/repo`, instala) | fase 3 do cenário `net`: 23974 bytes, instalado v1 |
| 87 | descoberta/atualização: `indice.txt` publicado × manifesto instalado | 1º boot instala; 2º boot "já na versão 1.0; nada a fazer" |
| 88 | atualização de janela TCP ao drenar (persist timer do par) | download de 24 KiB: ~30 s → 431 ms; teste de host |

Entre os blocos 83 e 84 entrou um commit de docs (`docs/sdk.md`: programas em C, a nexo-libc
e a convenção de processos). Suíte de boot: 118 testes; roadmap 182 itens concluídos.

Lições novas:

- **`tcp_recv` com `closed = 1` pode carregar bytes e ainda deixar bytes na fila** (até 4 KiB,
  1400 por resposta): drenar até uma resposta vazia; conferir o `Content-Length` para um
  download truncado falhar com a mensagem certa, não no CRC. O pcap (`--net-dump`) foi o
  árbitro: o servidor entregou tudo, o netstack aceitou tudo — o erro era do consumidor.
- **Janela zero sem atualização de janela = persist timer do par** (sondas de 1 byte a cada
  ~5 s no pcap). O receptor precisa anunciar a janela nova quando a aplicação drena
  (RFC 1122 §4.2.3.3).
- **Processo**: `tools/nexo-unsafe-audit` exige um `// SAFETY:` por linha `unsafe` (um comentário
  partilhado não conta) e o `make lint` só mostra isso no fim do log; um commit por lista
  explícita de arquivos exige conferir `git status` depois (o `Cargo.lock` do workspace ficou
  de fora uma vez); a validação de blocos consecutivos pode ser feita em união (mesma imagem)
  com commits separados por arquivos — o CI de cada push valida cada commit isolado.

## Lições registradas

- **A convenção de handles é posicional e vale no arranque.** Depois que um programa cria ou
  transfere handles, os slots 2/3 são reutilizados; uma sonda tardia acha o ocupante novo (o sh
  chegou a entregar um handle de processo como "stdout" de um filho — status 8/BadHandle no wait,
  1/InvalidArgs por handle duplicado no spawn). O crt0 agora fixa stdin/stdout antes do `main`.
- **Teste de corte de energia no host é detector de invariantes de montagem.** O `rename`
  "entrada nova primeiro, antiga depois" deixa dois nomes num corte — e o mount recusava
  "inode referenciado duas vezes". Como a v0 não tem hardlinks, esse estado só pode ser um
  rename interrompido: o check repara. Rodar o corte-em-cada-escrita sempre que uma operação
  nova toca diretórios.
- **Sem concorrência num canal de RPC.** O sh nunca usa o fs enquanto um filho segura o
  handle duplicado (as respostas iriam ao leitor errado): entrada bombeada antes do spawn,
  saída drenada depois dos waits. O shell de diagnóstico segue a mesma regra.
- **Ordem não é testável por marcadores soltos** — verificar por composição (`sort | head -n 1`).
- **`build-image` chama `build-c-demo` best-effort**: um utilitário que não compila vira
  "ausente do initrd" no teste, nunca erro de build — rodar `tools/build-c-demo` direto ao
  adicionar um.
- **Prever rolagem contando linhas de grade (com quebra), não linhas lógicas** ("meu mundo"
  quebra em 8 colunas).
- **Processo**: `gh run watch --exit-status` devolveu 1 para um run que concluiu `success`
  (glitch de polling) — o veredito é `gh run view --json conclusion`; blocos consecutivos com
  arquivos mistos (utest/CHANGELOG/PLANO) são separados guardando o estado do bloco novo no
  scratchpad e commitando por lista explícita de arquivos; não recompilar serviços enquanto uma
  varredura do `test-qemu` roda (ela reempacota os binários atuais por cenário).

## Estado e próximos passos

- Os `[ ]` restantes do plano são todos gated: TLS/cripto (decisão do usuário), hardware real,
  usuários externos, releases e governança. Do executável nos `[-]`, o segundo dia fechou
  visor, gerenciador, aspas e regex, permissões temporárias e o repositório em rede (fica só a
  assinatura); restam o painel de escala nas Configurações (privilégio do shell — precisa de
  mediação pelo orquestrador), variáveis no sh e documentos por Contexto. Em curso: sockets
  BSD na libc C (`sys/socket.h` sobre o `nexo.sock` gerado) com um `fetch` HTTP em C.
- Stress de 7 dias (gate F1 estendido) em execução desde 2026-09-01 19:29; relatório previsto
  para 2026-09-08.

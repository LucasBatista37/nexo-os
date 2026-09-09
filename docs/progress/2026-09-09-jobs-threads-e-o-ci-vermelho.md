# Jobs, threads, binding por propriedades — e três dias de CI vermelho — 2026-09-07 a 2026-09-09

Nove blocos em `main`. O arco técnico é bom (o kernel ganhou jobs, threads de usuário e
binding por propriedades), mas a lição do período é de método: **três blocos foram para o
`main` com o CI vermelho** porque a validação local não conseguia reproduzir a falha.

| Bloco | Commit | O que entrou |
|---|---|---|
| 104 | `e5500f1` | relatório 96–103 |
| 105 | `25c0446` | jobs: `job_create`/`job_attach`/`job_kill` (syscalls 36–38), morte em cascata |
| 107 | `a4ca3e7` | relatório parcial do stress de 7 dias (5,84 dias, zero erros) |
| 108 | `8646f1a` | **tique é fronteira de 1 ms** — conserta o CI |
| 106 | `8e6caa3` | threads de usuário: `thread_create`/`thread_exit`/`thread_join` (39–41) |
| 109 | `da4c14b` | CI: remover só as fontes apt de terceiros do runner |
| 110 | `371ed30` | `devmgr`: binding por propriedades (NVMe/AHCI por classe), papel por identidade |
| 111 | `63b36cf` | sonda do timer sem corrida; diagnóstico de `#UD`/`#GP` com bytes do código |

## O CI vermelho

O bloco 103 (tique dinâmico) fez a BSP ser interrompida sempre que existe um prazo mais
cedo. O contador de tiques, porém, continuou a somar **por interrupção** — e ele é usado
como medida de tempo. No QEMU do macOS a entrega de interrupções tem ~1 ms de latência, de
modo que o contador continuava parecido com o relógio e a suíte passava; no runner do CI
(Linux, entrega imediata) o teste `tsc_clock` via 144 tiques em 100 ms e reprovava.

Empilhei os blocos 104 e 105 sobre esse commit sem ler o resultado do CI. Três commits
vermelhos depois, o bloco 108 separou as duas ideias: **interrupção** é um evento do
hardware, **tique** é uma fronteira de 1 ms. As dormidas e os timers continuam a ser
varridos a cada interrupção (é isso que dá resolução fina), mas o quantum e o relógio de
tiques só andam nas fronteiras.

Segunda lição, no mesmo período: o CI também caiu por causa de um `apt-get update` que
quebrou num repositório de terceiros pré-configurado no runner (Chrome). O passo passou a
remover as fontes que não apontam para `*.ubuntu.com` — e a primeira tentativa apagou junto
o `ubuntu.sources` (o runner guarda os repositórios do próprio Ubuntu no mesmo diretório, em
formato deb822), deixando o job sem pacote nenhum.

**Regra que fica**: um bloco não está pronto quando a suíte local passa; está pronto quando o
CI daquele commit fica verde. Ambientes diferentes violam invariantes diferentes.

## Jobs e threads

- **Jobs** (105): grupo de processos com morte em cascata. Um processo herda o job de quem o
  criou, e `job_kill` fecha as tabelas de handles na hora (os pares veem `PeerClosed`), marca
  os membros e acorda quem estava em `recv`/`wait_any`/`process_wait`/`sleep`. O teste mata um
  **neto** que o driver do teste nunca conheceu.
- **Threads de usuário** (106): pilha própria em endereço aleatório, handles e memória
  partilhados, `exit` de qualquer thread encerra o processo e `process_wait` só devolve quando
  todas saíram. O fuzz de syscalls precisou excluir `thread_create`: com uma entrada aleatória
  a thread nova salta para lixo dentro do próprio processo e o mata — correto, mas
  indistinguível de falha para o fuzzer.
- Um deadlock foi corrigido no caminho da herança de job: `if let Some(x) = m.lock().clone()`
  mantém o guard vivo durante todo o bloco, e `attach` volta a travar o mesmo lock.

## Binding por propriedades

O `devmgr` reconhecia drivers só por IDs do VirtIO; o NVMe do ambiente de teste ficava sem
driver e o SATA era encontrado por **BDF fixo** (`00:1f.2`) dentro da fase A/B. Agora há
binding por classe/subclasse/prog_if (`01:08:02` → `nvmedev`, `01:06:01` → `ahcidev`), o
papel de cada disco sai da **identidade** que ele responde em `nexo.block` (dados =
`nexodata`, boot = `nexoboot`) e não da ordem do barramento, e a fase A/B reutiliza um canal
AHCI já aberto em vez de subir um segundo driver no mesmo hardware. O boot de teste passou de
3 para 6 drivers iniciados.

## Incidente aberto

Numa varredura, o serviço `fs` morreu **uma vez** com instrução inválida (`#UD`) no segundo
boot do cenário `storage`; não reproduziu em repetições isoladas. Com PIE e ASLR o `rip`
sozinho não diz nada, então o bloco 111 passou a registrar os 16 bytes em torno dele: se o
fantasma voltar, o log dirá se aquilo era código ou lixo.

## Estado

- Suíte: 125 testes; varredura de 11 cenários verde; CI verde em `63b36cf`.
- ABI: 42 syscalls (0–41).
- Stress de 7 dias relançado em 2026-09-09, desacoplado da sessão (a rodada anterior chegou a
  5,84 dias com zero erros e foi morta de fora); veredito previsto para 2026-09-16.

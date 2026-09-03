# Changelog

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/). Versões seguem os marcos do Plano Mestre §7.

## [Unreleased] — rumo a `0.1-kernel` e início da Fase 2

### Adicionado (Fase 2, bloco 1 — modo usuário)
- ABI de syscalls v0 (`abi/syscall`, `docs/spec/syscall-abi.md`): `exit`, `log`, `time_now`, `yield`, `sleep`, `get_pid`, `abi_version`, `debug_info`; status em `RAX`, valor em `RDX`.
- Entrada `syscall`/`sysret` com troca para a pilha de kernel da thread (`gs:[8]`), `swapgs` nas traps vindas de ring 3, GDT com segmentos de usuário (`0x23`/`0x2b`) e TSS.RSP0 por thread.
- Processos (`process.rs`): PML4 própria com a metade do kernel compartilhada, carga de ELF64 estático com W^X e bit `USER` propagado nas tabelas, pilha de usuário com guard page, thread principal em ring 3, `wait`, liberação do espaço ao terminar; troca de CR3 no escalonador.
- Faltas em modo usuário (`#PF`, `#GP`, `#UD`…) encerram só o processo (`EXIT_KILLED`), com log do motivo.
- `sdk/nexo-sys` (invocação de syscalls) e `services/init` (primeiro programa de usuário: exercita a ABI e os cenários de isolamento).
- Loader/ABI de boot v2: `\nexo\init.elf` é carregado como initrd (`initrd_addr/len`, tipo `Initrd`); leitor ELF compartilhado (`kernel/lib/elf`).
- Auto-testes `user_process`, `user_isolation` (leitura do kernel, `cli`, escrita em `.rodata` → processo morto, kernel íntegro, sem vazamento de quadros) e `user_syscall_error`.
- **Handles e canais (bloco 2):** tabela de handles por processo com direitos que só diminuem (`handle_close/duplicate/info`), canais bidirecionais com filas por extremidade, transferência de handles em mensagens, bloqueio em `recv` (`park/unpark` no escalonador), `PeerClosed` ao fechar; syscalls 8–13. `init` em modos servidor/cliente testa ping/pong, transferência de canal, direitos reduzidos (`Denied`), handles inválidos (`BadHandle`) e limites (`TooBig`); teste `user_ipc` verifica ausência de vazamento de objetos. 31 testes no boot.

- **init + service-manager (bloco 3):** initramfs próprio (`NEXOIRD1`, `kernel/lib/initrd`, `tools/mkinitrd.py`) com cinco programas; processos como objetos com handle (`process_spawn` por nome do initrd com handles iniciais, `process_wait`, `process_info`; syscalls 14–16); `sdk/nexo-rt` (formatação sem alocação, `log!`, panic handler); `services/init` inicia `services/svcmgr`, que supervisiona `services/echo` (cai de propósito após 3 pedidos) e reinicia-o até 3 vezes enquanto `services/echo-client` reconecta — servidor reiniciado sem reiniciar o kernel, 4 processos simultâneos; teste `user_services`. 32 testes no boot.

- **Fuzzing e regras de IPC (bloco 4):** testes *fuzz-lite* determinísticos (mutação de entradas válidas) para os parsers de ELF, initrd, ACPI, símbolos/demangler e ABI de boot; modo 7 do `utest` bombardeia o kernel com 20 000 syscalls aleatórias (ponteiros nulos/do kernel/desalinhados, tamanhos absurdos, handles inválidos) — encontrou e corrigiu um `expect` em handles repetidos numa mensagem e um vazamento por ciclo de referência (mensagem carregando a ponta do próprio canal); agora enviar uma ponta pelo seu próprio canal é `InvalidArgs` e um coletor de pontas inalcançáveis roda ao terminar processos. `docs/spec/ipc-compat.md` (regras de compatibilidade de protocolos). `docs/ROADMAP_STATUS.md` gerado por `tools/roadmap-status` (`make roadmap`). 33 testes no boot.

### Adicionado (Fase 3, bloco 1 — dispositivos)
- Enumeração PCI (`kernel/src/pci.rs`, acesso `0xCF8/0xCFC` em `nexo-arch-x86_64::pci`): todas as funções do barramento 0 (multifunção), classe, IRQ legada e BARs com tamanho por sondagem (32/64 bits, E/S), registradas no boot e listadas no log.
- Concessões de dispositivo como objetos com handle (`kind` 3, ADR-0015) e syscalls 17–23: `pci_enum`, `pci_cfg_read/write`, `mmio_map` (só dentro de BARs enumerados, sem cache), `dma_alloc` (página física zerada do processo), `irq_alloc`/`irq_wait` (vetores MSI 0x50–0x6f com contadores; devolvidos ao pool com a concessão). Estruturas `PciInfo`, `DmaBuffer`, `IrqInfo` em `abi/syscall`; wrappers em `sdk/nexo-sys`.
- `services/blockdev`: driver VirtIO-block 1.x em modo usuário (capabilities PCI modernas, negociação de `VERSION_1`, fila dividida de 64 descritores, MSI-X na entrada 0, bus master); protocolo cru `nexo.block` v0 (ler/escrever até 7 setores por mensagem).
- `utest` modo 8 (cliente de bloco): escreve/lê 4 setores, verifica marcador de persistência no setor 8 (grava no 1º boot, encontra no 2º), pedido fora da capacidade é recusado sem derrubar o driver.
- `tools/run-qemu --disk` (padrão `build/data.img`, 16 MiB, criado se ausente; `--no-disk`) anexa `virtio-blk-pci` moderno; cenário `storage` do `test-qemu` roda dois boots com o mesmo disco novo e exige o marcador persistido; testes `pci` e `user_block` (verifica ≥ 1 interrupção MSI-X entregue e ausência de vazamento de quadros/canais/vetores). 35 testes no boot.

### Adicionado (Fase 3, bloco 2 — sistema de arquivos persistente)
- `libraries/nexofs`: NexoFS v0 (ADR-0016) — biblioteca `no_std`/sem alocação/`forbid(unsafe_code)` com superbloco, bitmap, inodes (12 diretos + 1 indireto) e diretórios, todos com CRC32; copy-on-write com commit atômico em um setor; montagem reconstrói o bitmap e recupera órfãos. Testes de host: ciclo completo de arquivos, arquivos grandes, muitas entradas, disco cheio, **corte de energia em cada escrita** (com escritas rasgadas) e **imagens corrompidas** (fuzz-lite).
- `services/fs`: servidor NexoFS sobre o canal do `blockdev` (formata se não há assinatura), protocolo cru `nexo.fs` v0 (stat/create/mkdir/unlink/read/write/list/sync/info/truncate); `blockdev` ganhou `op` 2 (capacidade); últimos 256 setores reservados para os testes crus.
- `utest` modo 9: cria, lê, altera parcialmente, estende sobre vários blocos, trunca, lista e remove arquivos e diretórios; contador de boots persistente (`boot.count`). Testes `user_fs` e `user_block_crash` (driver morto no 2º pedido: cliente vê o canal fechado, kernel íntegro, vetor de IRQ devolvido). 37 testes no boot.
- `tools/nexo-disk` (Python, independente do Rust): `info`, `ls`, `cat`, `check` de um volume; o cenário `storage` verifica com ele o disco após os dois boots (`boot.count` = 2, volume consistente).
- Pilha de usuário de 256 KiB.
- Cenário `powercut`: `run-qemu --kill-on REGEX [--kill-delay-ms N]` mata o QEMU (SIGKILL) durante escritas contínuas (`fs-churn=1`, `utest` modo 10); o boot seguinte monta o volume sem reformatar e o host o verifica com `nexo-disk check`.

### Adicionado (Fase 3, bloco 3 — gerenciador de dispositivos, transporte VirtIO, RNG)
- Concessões de dispositivo por função: `device_open` (syscall 24) deriva de uma concessão raiz (`RIGHTS_DEVICE_ALL`, com `ADMIN`) uma concessão restrita a um BDF; com escopo restrito, `pci_enum`/`pci_cfg_*`/`mmio_map` só enxergam aquela função.
- `services/devmgr`: enumera PCI, faz *binding* por IDs (vendor VirtIO + tipo → `blockdev`/`rngdev`), inicia cada driver com concessão restrita e canal, sobe o `fs` sobre o bloco e entrega os canais de serviço ao cliente (`fs`, `rng`, `done`).
- `libraries/virtio` (`nexo-virtio`): transporte VirtIO 1.x sobre PCI compartilhado (capabilities, reset/negociação, MSI-X, fila dividida) com testes de host; `blockdev` reescrito sobre ele.
- `services/rngdev`: driver VirtIO-RNG (entropia) com MSI-X; protocolo `nexo.rng` v0. `run-qemu` anexa `virtio-rng-pci`.
- `utest` modo 11 e teste `user_devmgr`: usa o `fs` e o `rng` entregues pelo `devmgr`; bytes aleatórios não nulos e distintos; pedido inválido recusado; todos os processos terminam sem vazar. 38 testes no boot.

### Adicionado (Fase 3, bloco 4 — FAT somente para EFI)
- `libraries/fat` (`nexo-fat`): leitor FAT12/16/32 somente leitura (nomes 8.3 com caixa NT e nomes longos VFAT, cadeias de clusters, leitura com offset) e localização da partição de sistema EFI na GPT; `no_std`, `forbid(unsafe_code)`. Testes de host: imagem FAT12 construída à mão, 500 imagens corrompidas, GPT sintética e uma imagem FAT32 real gerada com mtools (como o ESP do projeto).
- `services/espfs`: serve a ESP da imagem de boot (protocolo `nexo.esp` v0: list/stat/read); no boot registra os tamanhos de `/EFI/BOOT/BOOTX64.EFI` e `/nexo/kernel.elf`.
- `run-qemu` anexa a própria imagem de boot como um segundo `virtio-blk` **somente leitura** (`serial=nexoboot`); `blockdev` negocia `VIRTIO_BLK_F_RO`, lê o serial (`GET_ID`) e recusa escritas em dispositivos somente leitura; `devmgr` roteia por serial (`nexodata` → `fs`, `nexoboot` → `espfs`).
- `utest` modo 11 verifica pelo `espfs`: raiz com `EFI` e `nexo`, tamanhos, cabeçalhos `MZ` e `\x7fELF`, caminho inexistente → não encontrado.

### Adicionado (Fase 3, bloco 5 — VFS, namespace por processo, ramfs, cache de blocos)
- `services/vfs`: VFS servindo o próprio protocolo `nexo.fs` com **namespace por instância** (máscara de montagens no argumento): `/disk` → NexoFS (caminhos reescritos, inodes etiquetados), `/boot` → ESP via `espfs` (só leitura; escrita → status 13), `/tmp` → ramfs gravável interno (16 arquivos × 16 KiB, volátil, por instância).
- Cache de blocos de leitura no `fs` (8 blocos, write-through; estatísticas no log ao desmontar).
- `utest` modo 12 e teste `user_vfs`: dois namespaces simultâneos (completo e só `/tmp`) — roteamento, leitura do kernel pela ESP via VFS, ramfs isolado entre instâncias, montagens invisíveis fora do namespace. 39 testes no boot.

### Adicionado (Fase 3, bloco 6 — console VirtIO e shell de diagnóstico)
- `services/consoledev`: driver VirtIO-console (porta 0, filas rx/tx, buffers de recepção pré-postados, MSI-X); protocolo `nexo.console` v0 (ler sem bloquear / escrever).
- `services/shell`: shell de diagnóstico na console — `ajuda`, `info` (CPUs, uptime, processos, handles, syscalls), `tempo`, `ls`/`cat`/`escreve`/`remove` sobre o VFS (`/boot`, `/disk`, `/tmp`), `eco`, `sair`; edição de linha com backspace.
- `shell=1` na linha de comando sobe blockdev+fs (+espfs se houver disco de boot), vfs, consoledev e shell; `run-qemu --console-socket PATH` liga a console a um socket UNIX.
- Cenário `shell` do `test-qemu`: conversa de verdade com o shell pelo socket (13 comandos com respostas verificadas, incluindo escrita persistente em `/disk` e volátil em `/tmp`), e o boot termina limpo após `sair`.

### Adicionado (Fase 3, bloco 7 — VirtIO input)
- `services/inputdev`: driver VirtIO-input (fila de eventos com 32 buffers de 8 B pré-postados, MSI-X); protocolo `nexo.input` v0 (ler eventos sem bloquear; formato evdev).
- `run-qemu --input-keyboard` (virtio-keyboard-pci) e `--qmp-socket` (controle QMP); `input-test=1` na linha de comando sobe inputdev + utest(13).
- Cenário `input`: o harness injeta teclas reais por QMP (`send-key` a, b, Enter) e o driver de usuário as entrega ao cliente, que confere os códigos (30, 48, 28).

### Adicionado (Fase 3, bloco 8 — IDL e protocolos tipados)
- IDL própria (`idl/*.idl`) e gerador `tools/idlgen` (`make idl`): produz módulos Rust `no_std` em `abi/proto` (`nexo-proto`) com o cabeçalho NXIP (`ipc-compat` §2 — magic, `protocol_id` FNV-1a, versões, `method_id`, flags, `payload_len` validado), structs com encode/decode append-only (campos com padrão para leitores novos ↔ payloads antigos), enum de pedidos e erros remotos tipados; `make idl-check` no CI recusa módulos defasados.
- Testes do `nexo-proto`: layout do cabeçalho, ida-e-volta, protocolo/versão/método desconhecidos rejeitados, payload estendido lido por decodificador antigo, erro remoto, 20 000 mutações sem pânico.
- Primeiro protocolo migrado: `nexo.rng` v1.0 — `rngdev` e o cliente (`utest` 11) falam NXIP; mensagens malformadas e pedidos inválidos viram erros tipados (códigos 3 e 1) sem derrubar o driver.

### Adicionado (Fase 3, bloco 9 — fuzzing contínuo)
- `make fuzz DURATION=<s>`: rodadas de fuzz de syscalls (20 000 por rodada, utest modo 7) com **sementes aleatórias** derivadas do TSC — registradas no log (`utest: fuzz semente 0x…`) para reproduzir falhas — e verificação de vazamento de quadros/canais a cada rodada (`[FUZZ] PASS rodadas=…`).
- Workflow agendado `.github/workflows/fuzz.yml` (semanal + manual): fuzz-lite de host (parsers, NexoFS, FAT, protocolos NXIP) e 30 min de fuzz de syscalls em QEMU, com o log como artefato.

### Adicionado (Fase 3, bloco 10 — nexo.block tipado)
- `nexo.block` v1.0 migrado para a IDL (`idl/block.idl`): métodos `read`/`write`/`capacity`/`identity` com cabeçalho NXIP e erros remotos tipados; `blockdev` (servidor) e todos os clientes (`fs`, `espfs`, `devmgr`, `utest`) atualizados — restam crus `nexo.fs`, `nexo.esp`, `nexo.console` e `nexo.input`.
- `tools/idlgen`: métodos sem campos geram encode/decode limpos (sem avisos).

### Adicionado (Fase 3, bloco 11 — nexo.console e nexo.input tipados)
- `nexo.console` v1.0 (`idl/console.idl`: `read`/`write`) e `nexo.input` v1.0 (`idl/input.idl`: `poll`) migrados para a IDL; `consoledev`, `shell`, `inputdev` e o cliente de teste falam NXIP. Dos protocolos crus restam `nexo.fs`, `nexo.esp` e os do bring-up (`svcmgr`/`echo`).

### Adicionado (Fase 3, bloco 12 — nexo.fs e nexo.esp tipados: pilha IPC 100% NXIP)
- `nexo.fs` v1.0 (`idl/fs.idl`, 10 métodos) e `nexo.esp` v1.0 (`idl/esp.idl`, 3 métodos) migrados: `fs`, `espfs` e o `vfs` (nas duas faces — cliente e servidores de backend) falam NXIP; `utest` e `shell` idem. Todos os protocolos de serviço agora saem da IDL; crus restam só os canais do bring-up (`svcmgr`/`echo`) e as mensagens de entrega do `devmgr`.

### Adicionado (Fase 3, bloco 14 — fila assíncrona de blocos)
- `channel_try_recv` (syscall 25): como `channel_recv`, mas devolve `WouldBlock` em vez de bloquear (base para laços de serviço que também esperam E/S; espera múltipla real fica para os objetos de evento).
- `blockdev` assíncrono: até 4 pedidos de E/S em voo na virtqueue (páginas de DMA por slot), respostas **na ordem de chegada**; quando há E/S pendente o driver usa `try_recv` + MSI-X em vez de bloquear no canal. `utest` (modo 8) encadeia 4 leituras sem esperar e confere ordem e conteúdo.

### Adicionado (Fase 4, bloco 1 — VirtIO net)
- `services/netdev`: driver VirtIO-net em modo usuário (filas rx/tx, cabeçalho `virtio_net_hdr` de 12 B, MAC da configuração via `VIRTIO_NET_F_MAC`, MSI-X, fila local de 8 quadros recebidos); protocolo tipado `nexo.net` v1.0 (`idl/net.idl`: `mac`/`send`/`recv`).
- `run-qemu --net` (virtio-net-pci + slirp) e cenário `net`: o `utest` (modo 14) monta um ARP request pelo canal do driver e confere o ARP reply do gateway 10.0.2.2 — o primeiro pacote de rede de verdade indo e voltando por um driver em ring 3.

### Adicionado (Fase 4, bloco 2 — Ethernet/ARP/IPv4/ICMP)
- `libraries/net` (`nexo-netstack`): montagem e leitura de Ethernet, ARP (request/reply), IPv4 (cabeçalho + checksum RFC 1071, validação de versão/IHL/tamanho) e ICMP echo — `no_std`, sem alocação, `forbid(unsafe_code)`; testes de host com vetores conhecidos e fuzz-lite de 20 000 mutações.
- Cenário `net` agora faz um **ping de verdade**: ARP para resolver o gateway do slirp e ICMP echo request/reply (ident/seq/checksum conferidos, ttl reportado).

### Adicionado (Fase 4, bloco 3 — UDP e DHCP)
- `nexo-netstack`: UDP (montagem/leitura com validação de tamanho) e cliente DHCP (DISCOVER/REQUEST com opções 53/50/54/55; parser de OFFER/ACK com xid, magic cookie e opções 1/3/6/54); testes de host e fuzz-lite ampliado.
- Cenário `net` completo: **DHCP de verdade** contra o slirp (DISCOVER → OFFER → REQUEST → ACK; lease 10.0.2.15/24, gateway e DNS registrados no log) e o ARP + ping passam a usar o endereço do lease em vez de valores fixos.

### Adicionado (Fase 4, bloco 4 — DNS)
- `nexo-netstack`: consulta DNS A (montagem com rótulos validados) e parser de respostas (id/QR, perguntas puladas, ponteiros de compressão com limite, primeiro registro A); teste de host com resposta sintética e fuzz-lite ampliado.
- Cenário `net` fecha o ciclo: DHCP → ARP → ping → **consulta DNS real** por `example.com` ao servidor do lease (10.0.2.3, encaminhada pelo slirp ao resolvedor do host), com o rcode/registro A no log.

### Adicionado (Fase 4, bloco 5 — TCP: handshake e dados reais)
- `nexo-netstack`: segmentos TCP (montagem/leitura sem opções, checksum com pseudo-cabeçalho IPv4 validado nos dois sentidos, flags/seq/ack/janela); testes de host e fuzz-lite ampliado.
- Cenário `net`: o harness sobe um servidor TCP no host (porta efêmera, passada por `tcp-port=` na linha de comando) e o cliente no Nexo faz o handshake completo (SYN → SYN-ACK → ACK) com `10.0.2.2:<porta>` através do slirp, envia uma linha, recebe `nexo-tcp-ok` e encerra com RST — sequência DHCP → ARP → ICMP → DNS → TCP inteira num boot.

### Adicionado (Fase 4, bloco 6 — netd: serviço de rede residente com sockets nativos)
- `services/netd`: DHCP no arranque, ARP do gateway (e resposta a ARP pelo nosso IP), laço de eventos único (`channel_try_recv` + bomba de quadros) e a **API de sockets nativa** `nexo.sock` v1.0 (`idl/sock.idl`): `info`, `resolve` (DNS **com cache**), `udp_send`/`udp_recv` (filas por porta), `tcp_connect`/`tcp_send`/`tcp_recv`/`tcp_close`.
- Máquina de estados TCP do cliente documentada em `docs/spec/tcp-states.md` (SYN_SENT → ESTABLISHED → FIN_WAIT/CLOSE_WAIT/LAST_ACK; RST; limitações da v0: sem retransmissão própria, TIME_WAIT imediato).
- Cenário `net`, fase 2: `netdev` + `netd` + cliente — info do lease, resolução com cache comprovado (2ª consulta `cached=1`), eco UDP e conexão TCP completa com os servidores do harness no host (que agora atendem múltiplas conexões).

### Adicionado (Fase 4, bloco 7 — máquina de estados TCP testável e retransmissão)
- `nexo-netstack::tcp`: a máquina de estados TCP saiu do `netd` para a biblioteca (`TcpSocket`: connect/on_segment/send/close/poll/take_rx), **com retransmissão** (RTO 500 ms, 5 tentativas → conexão reiniciada) e ACK duplicado para segmentos fora de ordem; suíte de host com 7 testes de estados (handshake, dados, retransmissões até RST, fecho ativo/passivo, RST, contrapressão).
- `netd` reescrito sobre a biblioteca: só monta quadros e bombeia o temporizador (`tcp_timers` no laço de eventos); `docs/spec/tcp-states.md` atualizado.

### Adicionado (Fase 4, bloco 8 — escuta TCP: conexões de entrada)
- `nexo-netstack::tcp`: lado passivo — `listen()`/`on_syn()` (LISTEN → SYN_RCVD → ESTABLISHED, SYN-ACK retransmissível, dados no ACK final), com teste de host dedicado.
- `netd`: `tcp_listen{port}` no `nexo.sock` (bloqueia até aceitar; devolve conexão + par); SYNs de entrada roteados aos sockets em escuta.
- Cenário `net`, fase 3: `run-qemu --net-hostfwd` encaminha uma porta do host para `10.0.2.15:8080`; o harness **conecta para dentro do Nexo**, o `netd` aceita, recebe `ola do host` e responde `nexo-listen-ok`.

### Adicionado (Fase 4, bloco 9 — cliente HTTP sobre a API de sockets)
- Cenário `net`, fase 4: um GET HTTP/1.0 completo pela API `nexo.sock` (conectar, enviar o pedido, juntar a resposta em vários `tcp_recv`, validar `HTTP/1.0 200` + corpo, fechar) contra um servidor HTTP no host — base do item "cliente HTTP para atualizações".
- Capturas de rede para diagnóstico: `run-qemu --net-dump ARQ.pcap` (filter-dump do QEMU); o cenário `net` grava `build/logs/net.pcap`.

### Adicionado (Fase 4, bloco 10 — espera múltipla de canais)
- `channel_wait_any` (syscall 26): bloqueia até algum de até 16 canais ter mensagem ou par fechado, devolvendo o índice (acordado pelo `send`/fecho; tique de cobertura de 10 ms para a janela de registro — objetos de evento de verdade virão depois). `readable()`/`register_waiter()` no kernel, wrapper no SDK, documentação na ABI.
- Teste `user_wait_any` (utest modo 16): pronto imediato, ordem dos índices, par fechado, e os erros (`InvalidArgs`, `BadHandle`); o fuzzer de syscalls passa a pular a 26 (bloqueante por natureza). 40 testes no boot.

### Adicionado (Fase 4, bloco 11 — rede orientada a eventos: canal de IRQ e eventos na IDL)
- `irq_channel` (syscall 27): canal cuja ponta de leitura recebe 1 byte por disparo do vetor MSI (coalescido); com `channel_wait_any`, um driver espera {pedido do cliente, interrupção} sem varredura.
- IDL ganhou **eventos** (`event <id> <nome> { … }` → mensagens `FLAG_EVENT` sem resposta, com decodificador próprio); `nexo.net` **v1.1**: método `subscribe` + evento `frame` — o `netdev` passa a empurrar cada quadro recebido, dormindo em `wait_any({canal, canal de IRQ})`.
- `netd` idle agora dorme em `wait_any({cliente, driver})` (zero CPU ocioso) e recebe quadros por eventos; RPCs ao driver desviam eventos intercalados para uma fila local; DNS do `resolve` migrou para a fila UDP do próprio socket (novo `dns_parse_payload` na `nexo-netstack`).

### Adicionado (Fase 4, bloco 12 — janela deslizante de transmissão TCP)
- `nexo-netstack::tcp`: transmissão com até 4 segmentos em voo (dados, SYN e FIN compartilham a janela), ACKs cumulativos e parciais liberando slots, retransmissão do mais antigo vencido (go-back-1); API por slots (`TxSeg.slot` + `slot_payload`). Suíte de host ampliada para 19 testes (pipelining com ACK parcial, FIN em voo junto com dados, retransmissão do mais antigo).

### Adicionado (Fase 4, bloco 13 — netd multi-cliente e handles na IDL)
- IDL: tipo de campo `handle` (viaja no vetor de handles da mensagem, nunca no payload — ipc-compat §1); o gerador emite `HANDLE_COUNT`, `handles()` e `decode_request_with_handles(msg, hs)` (injeta os handles recebidos na ordem de declaração). Teste de host em `nexo-proto`.
- `nexo.sock` v1.0: método `open{chan}` — um cliente cria um canal e transfere a ponta ao `netd`, que passa a atendê-la como mais um cliente.
- `netd` multi-cliente: laço de eventos varre até 8 clientes com `channel_try_recv`, ocioso dorme em `channel_wait_any({todos os clientes, driver})`, encerra quando o último desconecta. `utest` (modo 15) abre uma 2ª sessão por `open` e usa `info` por ela.

### Adicionado (Fase 4, bloco 14 — IPv6 e NDP)
- `nexo-netstack::ipv6`: endereço link-local (EUI-64), cabeçalho IPv6 fixo, checksum ICMPv6/UDP/TCP com pseudo-cabeçalho, ICMPv6 echo e **NDP** — Neighbor Solicitation/Advertisement com endereços multicast solicited-node e MAC `33:33`; `no_std`, sem alocação. 4 grupos de testes de host (incl. NS→NA→echo completo e fuzz-lite).
- `netd` responde a Neighbor Solicitations pelo seu link-local (nó IPv6 alcançável). Cenário `net`: o guest emite um NS válido pela `nexo-netstack` e o harness confirma no pcap (`build/logs/net.pcap`) um ICMPv6 tipo 135 bem-formado saindo na interface.

### Adicionado (Fase 4, bloco 15 — firewall por perfil e permissões de rede por sessão)
- `nexo-netstack::firewall`: política de rede por perfil (`Profile`) — **negar por padrão**, até 8 regras (sub-rede IPv4/porta/protocolo) mais permissões `allow_dns`/`allow_listen`; `allows()` distingue "sem regra" de "protocolo/porta negados". 5 grupos de testes de host.
- `nexo.sock` `open{chan, allow_dns, allow_listen, rule_*}` agora carrega o perfil da sessão-filha: o `netd` associa um `Profile` a cada cliente (o primeiro é irrestrito; os abertos por `open` são restritos pelo pai) e nega `tcp_connect`/`udp_send`/`tcp_listen`/`resolve` fora do perfil (erro remoto 7). `utest` (modo 15) comprova permitido/negado.

### Adicionado (Fase 4, bloco 16 — captura autorizada e fuzz de estados de protocolo)
- `nexo-netstack::tcp`: fuzz-lite da **máquina de estados** — 3000 conexões (ativas e passivas) recebem sequências aleatórias de segmentos/ações e o teste verifica que a máquina nunca entra em pânico e mantém invariantes (`snd_una ≤ snd_nxt`, buffer de recepção nos limites, pendências ≤ slots). Entra no fuzzing semanal do CI (`cargo test ... fuzz`).
- `tools/netcap` + `make netcap`: sobe o Nexo com a rede user-mode, grava um pcap de todos os pacotes da interface (`run-qemu --net-dump`) e imprime um resumo legível por protocolo (ARP/ICMP/TCP/UDP/ICMPv6) e por fluxo; `--summary-only ARQ.pcap` resume um pcap existente. Captura sempre local ao QEMU do usuário e explicitamente pedida — nunca uma escuta silenciosa.

### Adicionado (Fase 4, bloco 17 — personalidade POSIX de sockets)
- `sdk/nexo-net`: camada de compatibilidade **BSD sockets** sobre a API nativa `nexo.sock` (ADR-0014 §3: POSIX como personalidade em espaço de usuário, descritores → handles). API `socket`/`connect`/`send`/`recv`/`sendto`/`recvfrom`/`close`/`getaddrinfo`, `sockaddr_in`, `AF_INET`/`SOCK_STREAM`/`SOCK_DGRAM`, erros estilo `errno` (mapeados dos códigos remotos do `netd`). `no_std`, `forbid(unsafe_code)`, tabela de 16 descritores por processo.
- `utest` (modo 15) faz um TCP connect/send/recv/close pela API POSIX contra o servidor do host — mesma conversa que o caminho nativo, agora pela personalidade.

### Adicionado (Fase 5, bloco 1 — renderizador 2D por software)
- `libraries/gfx` (`nexo-gfx`): renderizador 2D `no_std`/`forbid(unsafe_code)` sobre uma `Surface` (buffer do chamador com largura/altura/stride/formato `Rgbx8888`/`Bgrx8888`) — `Color` RGBA, `Rect` com interseção, `put`/`get`, **composição alfa src-over** (`blend`), `fill_rect`/`stroke_rect`/`clear`, `blit` entre superfícies (converte formatos) e **retângulo de clipping**. 7 grupos de testes de host (ordem de bytes por formato, alfa, clipping, stride, blit).
- Auto-teste `gfx` no boot: renderiza numa superfície de rascunho no heap e confere pixels — o renderizador roda no ambiente `no_std`/alloc do kernel. 41 testes no boot.

### Adicionado (Fase 5, bloco 2 — rasterização de texto e fallback de fontes)
- `nexo-gfx::text`: rasteriza strings sobre uma `Surface` com a fonte bitmap 8×8 (`nexo-font`) — `draw_glyph`/`draw_text` (escala inteira, cor de frente, fundo opcional, `\n` quebra linha), `text_width`/`cell_width`/`cell_height`; glifos fora da faixa imprimível caem no glifo de **fallback** da fonte. 4 grupos de testes de host (dimensões, desenho + fundo, quebra de linha, fallback).
- O auto-teste `gfx` do boot passou a desenhar um glifo e conferir a largura de texto.

### Adicionado (Fase 5, bloco 3 — motor de composição e rastreamento de danos)
- `libraries/wm` (`nexo-wm`): motor de composição `no_std`/`forbid(unsafe_code)` — dada uma lista de `Window` (retângulo + buffer de pixels do cliente + z + formato) e uma região de dano, compõe as janelas visíveis sobre um fundo na superfície de saída **por ordem de z** (ordenação estável, independente da ordem de entrada), redesenhando só os pixels dentro do dano. `Damage` acumula até 16 retângulos sujos e coalesce no envelope quando enche (`bounds`/`rects`/`clear`); `Rect::union` no `nexo-gfx`. 4 grupos de testes de host (z-order, ordem de entrada, dano limitando o repintar, coalescência).
- Nota de arquitetura: o **serviço** compositor e o transporte por `MemoryObject` compartilhado (buffers dos clientes) ficam para um bloco futuro — dependem de memória compartilhada entre processos, ainda não implementada.

### Adicionado (Fase 5, bloco 4 — serviço compositor em modo usuário)
- IDL `nexo.wm` (`idl/wm.idl`): `create_surface{x,y,w,h,z}→{id,mem:handle}`, `commit{id}`, `move{id,x,y}`, `destroy{id}`, `output→{w,h,mem:handle}` — os handles de `MemoryObject` viajam no vetor de handles da mensagem (ipc-compat §1), nunca no payload; gerado por `make idl`.
- `services/wm` (`nexo-wm`, binário `nexo-wm` no initrd): compositor em **ring 3**. Cada `create_surface` aloca um `MemoryObject` do tamanho da superfície, mapeia-o para leitura e transfere um duplicado do handle ao cliente (que o mapeia para escrever os pixels). O `commit`/`move`/`destroy` recompõe a cena com `nexo-wm::composite` (ordem-Z sobre fundo) numa **saída** que também é um `MemoryObject`, devolvida por `output`. Um cliente por instância; a apresentação num framebuffer real fica para a integração com o serviço de vídeo.
- `utest` modo 19 (cliente do compositor) e auto-teste de boot `user_wm`: o cliente cria duas superfícies sobrepostas (vermelha em z=0, verde em z=1), escreve as cores na memória compartilhada, faz `commit` e lê a saída composta conferindo a ordem-Z pixel a pixel (sobreposição → verde por cima, áreas exclusivas → cor de cada janela, fundo → preto); verifica também que nenhum quadro vaza quando o serviço e o cliente encerram (os `MemoryObject` são liberados no Drop). Composição **fim a fim entre processos** por memória compartilhada. 43 testes no boot.

### Adicionado (Fase 5, bloco 5 — compositor multi-cliente)
- `nexo.wm` v1.1: método `open{chan:handle}` — um cliente abre outra sessão transferindo a ponta de um canal novo (mesmo padrão do `netd`); o wm passa a atendê-la como mais um cliente (até 8 sessões).
- `services/wm` agora é **multi-sessão**: laço sobre as sessões com `channel_try_recv` + `channel_wait_any` no ócio (sem polling ativo). As superfícies pertencem à **sessão** que as criou — só ela pode `commit`/`move`/`destroy` (erro remoto 3 caso contrário) — e são liberadas quando a sessão desconecta (recompondo a cena); o serviço encerra quando a última sessão sai. A saída composta continua **global** (todas as superfícies de todas as sessões por z-order).
- `utest` modo 20 + auto-teste de boot `user_wm_multi`: a sessão 1 cria a superfície vermelha (z=0), abre a sessão 2 por `open` e cria nela a verde (z=1); confere a composição Z das duas sessões na saída e o **isolamento** (a sessão 1 recebe erro ao tentar `commit` na superfície da sessão 2); sem vazamento de quadros/canais. Prova de **duas aplicações independentes compartilhando a tela**. 44 testes no boot.

### Adicionado (Fase 5, bloco 6 — restacking de janelas)
- `nexo.wm` v1.2: métodos `raise{id}` (traz a superfície para a frente — z acima de todas) e `lower{id}` (envia para trás — z abaixo de todas); ambos recompõem a cena. Enforçam a posse por sessão (erro remoto 3 para superfície de outra sessão).
- `utest` modo 21 + auto-teste de boot `user_wm_restack`: com duas superfícies sobrepostas (vermelha e verde), alterna quem fica na frente com `raise`/`lower` e confere na saída composta que o pixel da sobreposição muda de cor conforme o z (verde↔vermelho a cada reordenação); sem vazamento de quadros/canais. Base do "clique traz para a frente" do gerenciador de janelas. 45 testes no boot.

### Adicionado (Fase 5, bloco 7 — redimensionamento de superfícies + `munmap`)
- Syscall `memory_unmap` (30): desmapeia páginas compartilhadas (`memory_map`/`mmio_map`) na região de dispositivos — limpa as PTEs e invalida o TLB (local `invlpg` + shootdown das outras CPUs) **sem** liberar os quadros físicos (que pertencem ao objeto). `AddressSpace::unmap_user_shared` no kernel; wrapper `nexo_sys::memory_unmap`. Primeira primitiva de `munmap` do sistema.
- `nexo.wm` v1.3: método `resize{id,w,h}→{mem:handle}` — o compositor desmapeia e libera o buffer antigo da superfície, aloca um novo `MemoryObject` do novo tamanho e devolve um handle; o cliente remapeia (o conteúdo anterior é perdido). Enforça a posse por sessão.
- `utest` modo 22 + auto-teste de boot `user_wm_resize`: cria uma superfície 8×8, confere que uma área além dela é fundo, redimensiona para 16×16 (o cliente desmapeia com `memory_unmap` e fecha o buffer antigo, mapeia o novo), pinta e confere que a área nova agora aparece na saída composta; sem vazamento de quadros (o buffer antigo é liberado quando as duas pontas o fecham). 46 testes no boot.

### Adicionado (Fase 5, bloco 8 — entrada de ponteiro e foco por clique)
- `nexo.wm` v1.4: método `set_input{chan:handle}` — registra uma **fonte de entrada** transferindo a ponta de leitura de um canal por onde chegam eventos **evdev crus** (8 B: `[type u16][code u16][value u32]`). Em produção o serviço `inputdev` alimenta esse canal; o compositor o inclui no `channel_wait_any`.
- `services/wm` processa entrada: `EV_ABS`/`ABS_X`/`ABS_Y` movem o ponteiro; `EV_KEY`/`BTN_LEFT` (press) fazem **foco por clique** — a superfície de maior z cujo retângulo contém o ponteiro é trazida para a frente (recompõe).
- `utest` modo 23 + auto-teste de boot `user_wm_input`: com duas superfícies sobrepostas, registra uma fonte de entrada e injeta cliques evdev sintéticos; confere na saída composta (com *polling*, pois a entrada é assíncrona) que o pixel da sobreposição muda de cor conforme a janela clicada vem para a frente. 47 testes no boot.

### Adicionado (Fase 5, bloco 9 — teclado à janela em foco)
- `nexo.wm` v1.5: evento `key{surface,code,value}` (`FLAG_EVENT`) — o compositor entrega cada tecla (`EV_KEY` que não seja `BTN_LEFT`) à **janela em foco**, na sessão dona da superfície. O foco é definido pelo clique (bloco 8); o foco é solto se a superfície focada é destruída/desconecta (não vaza para um slot reutilizado).
- `utest` modo 24 + auto-teste de boot `user_wm_keyboard`: cria uma superfície, foca-a por clique e injeta teclas (press/release); confere que chegam como eventos `key` com o id da superfície e o código/valor corretos. 48 testes no boot.

### Adicionado (Fase 5, bloco 10 — opacidade por superfície)
- `nexo-wm`: `Window` ganhou `alpha` (opacidade da janela inteira); `composite` compõe cada janela src-over com essa opacidade sobre o que está abaixo. Buffers seguem `*x8888` (sem alfa por pixel), então a transparência é por janela. Teste de host de composição alfa.
- `nexo.wm` v1.6: método `set_alpha{id, alpha}` — define a opacidade de uma superfície (255 = opaca; nascem opacas), recompõe. Enforça a posse por sessão.
- `utest` modo 25 + auto-teste de boot `user_wm_alpha`: duas superfícies opacas sobrepostas (a de cima verde); define a opacidade da de cima em ~50% e confere na saída composta que a sobreposição vira uma mistura verde+vermelho (~(127,128,0)), a área só da translúcida mistura com o fundo preto e a área da opaca embaixo fica intacta. 49 testes no boot.

### Adicionado (Fase 5, bloco 11 — toolkit de UI e temas)
- `libraries/ui` (`nexo-ui`): toolkit de UI `no_std`/`forbid(unsafe_code)` sobre `nexo-gfx` — **tokens de design** (`Theme`: fundo, texto, acento, borda, estados de botão) com variantes **claro/escuro/alto contraste**; widgets `Label` (texto + `size`), `Button` (retângulo + rótulo centralizado + estados `Normal`/`Hover`/`Pressed`, `contains`/`update` para hit-test) e layout `VStack` (empilha filhos com espaçamento). Os widgets pintam só a partir do tema. 5 grupos de testes de host (temas distintos, tamanho de rótulo, hit-test/estado do botão, desenho do botão, layout).
- `utest` modo 26 + auto-teste de boot `user_wm_ui`: um cliente pinta o fundo do tema e um botão (`nexo-ui`) na sua superfície e o `wm` compõe a cena; a saída composta é conferida (fundo do tema fora do botão, fundo do botão no interior, borda nas arestas). Prova a pilha **app → ui → gfx → compositor**. 50 testes no boot.

### Adicionado (Fase 5, bloco 12 — maximizar/restaurar janela)
- `nexo.wm` v1.7: métodos `maximize{id}→{mem}` (move para (0,0) e redimensiona para a saída inteira, guardando o retângulo anterior) e `restore{id}→{mem}` (volta ao retângulo salvo). Ambos realocam o `MemoryObject` (novo handle; o conteúdo antigo é perdido) — o serviço fatorou um helper `realloc_surface` reusado por `resize`/`maximize`/`restore`.
- `utest` modo 27 + auto-teste de boot `user_wm_maximize`: cria uma superfície pequena, maximiza (o cliente remapeia o novo buffer de tela cheia e o pinta) e depois restaura (volta ao tamanho e posição originais), conferindo na saída composta a cada passo; sem vazamento de quadros (a realocação usa `memory_unmap`). 51 testes no boot.

### Adicionado (Fase 5, bloco 13 — atalho global de teclado)
- `services/wm`: o compositor mantém o estado do modificador (Super/Meta, `KEY_LEFTMETA`) e intercepta o **atalho global Meta+Tab** (`KEY_TAB`) — cicla o foco trazendo a janela de trás (menor z) para a frente — **antes** de entregar teclas à janela em foco (o modificador e o atalho não são entregues).
- `utest` modo 28 + auto-teste de boot `user_wm_shortcut`: com duas superfícies sobrepostas, injeta Meta+Tab e confere na saída composta que a janela de trás vem para a frente, e que repetir o Tab alterna qual janela fica no topo. 52 testes no boot.

### Adicionado (Fase 5, bloco 14 — apresentação no framebuffer real)
- Syscall `fb_info` (31): escreve o layout do framebuffer de boot (`FbInfo`, 40 bytes: base física, tamanho, largura/altura/stride, formato, bpp) num ponteiro do usuário. Só **informação** — o **mapeamento** da tela continua gated pelo modelo de capacidades existente: o framebuffer é um BAR do dispositivo de vídeo, então mapeá-lo exige a **concessão** desse dispositivo (`mmio_map` já validava a faixa contra os BARs da concessão). Nenhum privilégio novo foi criado.
- `services/wm` **apresenta na tela**: se recebe a concessão do dispositivo de vídeo (handle 1), consulta `fb_info`, mapeia o framebuffer e, a cada recomposição, copia a saída composta para o canto superior esquerdo da tela com conversão de formato (RGBX→BGRX via `blit`). Sem a concessão, segue compondo só na saída compartilhada (todos os testes anteriores inalterados).
- `klog::disable_console`/`enable_console`: o espelho do log no console gráfico pode ser suspenso (a serial segue) — o *handoff* do framebuffer para o compositor durante a apresentação.
- `utest` modo 29 + auto-teste de boot `user_wm_present`: o kernel suspende o console, concede o dispositivo de vídeo (classe 03 cujo BAR contém o framebuffer) ao `wm` e o cliente compõe uma superfície magenta; o kernel **lê os pixels do framebuffer físico** (via physmap) e confere que a cena composta chegou à tela de verdade. Pulado (com aviso) se não há framebuffer ou se ele não está num BAR de vídeo. 53 testes no boot. **O compositor do Nexo OS agora desenha num display real.**

### Adicionado (Fase 5, bloco 15 — mosaico com escalonamento na composição)
- `nexo-wm`: o **retângulo de exibição foi desacoplado do tamanho do buffer** — `Window` ganhou `src_w`/`src_h` e, quando diferem de `rect.w/h`, o conteúdo é **escalado** (vizinho mais próximo) na composição. Base do mosaico e de miniaturas. Teste de host (buffer 2×2 exibido em 8×8, quadrantes conferidos).
- `nexo.wm` v1.8: método `tile{}` — organiza **todas** as superfícies (de todas as sessões) numa grade que cobre a saída, **sem realocar buffer nenhum** (só muda os retângulos de exibição; `Slot` agora guarda `buf_w`/`buf_h` separados do rect). Gesto global de layout.
- `utest` modo 30 + auto-teste de boot `user_wm_tile`: duas janelas totalmente sobrepostas; após `tile`, a saída composta as mostra lado a lado (célula 32×48 cada, conteúdo escalado), sem sobreposição. 54 testes no boot. Fecha o item "janelas, redimensionamento, maximização e mosaico" do plano.

### Adicionado (Fase 5, bloco 16 — teclado real de ponta a ponta)
- `nexo.input` v1.1: método `subscribe{chan:handle}` — o `inputdev` passa a **empurrar** cada lote de eventos evdev **crus** (8 B/evento, sem cabeçalho NXIP) no canal transferido, guiado por interrupção (`irq_channel` + `channel_wait_any`, sem varredura ociosa; sem MSI-X cai num cochilo curto). A outra ponta do canal pode ir direto ao `set_input` do compositor, que lê o mesmo formato. Sem assinante, o comportamento antigo do `poll` fica intacto (a interrupção só é consumida; os eventos ficam na fila) — um primeiro corte drenava e descartava eventos no caminho de IRQ, roubando-os do `poll`; a fase 1 do cenário pegou a regressão.
- `services/wm`: **foco na criação** — a primeira janela criada sem foco prévio passa a receber o teclado (antes só o clique dava foco, e teclas reais chegavam sem ponteiro).
- `input-test=2` + `utest` modo 31: o kernel arma a cadeia completa (`inputdev` com a concessão do teclado virtio + `wm` + cliente); o cliente cria uma janela, assina o `inputdev` e entrega a ponta ao `wm`. A fase 2 do cenário `input` injeta teclas **físicas** por QMP e confere que `a`(30), `b`(48) e `Enter`(28) chegam como eventos `key` da janela em foco: **teclado real → driver → compositor → aplicação**, tudo em ring 3.

### Adicionado (Fase 5, bloco 17 — captura segura de entrada)
- `nexo.wm` v1.9: métodos `grab{id}` e `ungrab{id}` — enquanto a captura vigora, **todo o teclado vai para a superfície capturada** (ignorando o foco) e **cliques são engolidos** (nenhuma janela é trazida à frente/focada): entrada sensível (ex.: senha) sem roubo de foco. Só a sessão dona captura/solta; uma captura por vez (erro remoto 5); solta sozinha se a superfície é destruída ou a sessão desconecta.
- `utest` modo 32 + auto-teste de boot `user_wm_grab`: foca B por clique, captura A e confere que a tecla seguinte chega como evento de A (não de B) e que um clique na região de A é engolido (B continua na frente — verificado por pixel, com a ordem garantida pelo FIFO do canal de entrada); após `ungrab`, o clique volta a trazer A à frente e a focá-la. 55 testes no boot. Fecha o item "foco, atalhos e captura segura" do plano.

### Adicionado (Fase 5, bloco 18 — múltiplos displays emulados)
- `nexo.wm` v1.10: o compositor gerencia **2 displays emulados**, cada um com sua saída (`MemoryObject` independente). `create_surface` ganhou `display` e `output` ganhou `{display}` — ambos com o **padrão 0** do ipc-compat §3 (clientes antigos continuam funcionando no display primário); novo `move_to_display{id,display}`. A recomposição compõe cada display só com as suas janelas; o display 0 é o apresentado no framebuffer real.
- `utest` modo 33 + auto-teste de boot `user_wm_displays`: A no display 0 e B no display 1 (mesmas coordenadas) — cada saída mostra só a sua janela; `move_to_display` leva A ao display 1 (o 0 fica vazio; no 1, B cobre A por z). 56 testes no boot.

### Adicionado (Fase 5, bloco 19 — login e bloqueio (`greeter`))
- `services/greeter`: tela de login/bloqueio — cria uma superfície em tela cheia (tema escuro + rótulo, via `nexo-ui`), **captura a entrada** (`grab`: as teclas da senha não podem ser lidas por outra janela e cliques não roubam o foco) e lê a senha pelos eventos `key` do compositor. Senha errada: reporta e permanece bloqueado; senha certa (demo: "nexo"+Enter): solta a captura, destrói a tela e devolve a entrada à sessão. Credencial fixa por ora — armazenamento seguro vem com o modelo de usuários (Fase 6).
- `utest` modo 34 (driver) + auto-teste de boot `user_greeter` (**multi-processo**: wm + greeter + driver): o driver abre uma 2ª sessão do wm e a entrega ao greeter por um canal, injeta a senha errada (recebe "wrong"; segue bloqueado) e depois a certa (recebe "unlocked"); então confere que a entrada voltou (clique + tecla chegam à janela do driver). 57 testes no boot.

### Adicionado (Fase 5, bloco 20 — Contextos (protótipo))
- `nexo.wm` v1.11: **Contextos** — grupos de janelas (4, `0..3`). Só as janelas do contexto **ativo** são compostas e recebem cliques/atalhos; as dos outros ficam ocultas com o estado preservado (buffers intactos). `set_context{id,context}` move uma janela de grupo; `switch_context{context}` ativa outro grupo, recompõe e move o foco para a janela de maior z do novo contexto. A **captura** (grab) sobrevive à troca — uma tela segura (ex.: bloqueio) não é contornável trocando de contexto.
- `utest` modo 35 + auto-teste de boot `user_wm_context`: duas janelas no mesmo lugar; mover B ao contexto 1 a esconde (A aparece), ativar o 1 mostra B e o foco a acompanha (tecla chega a B; clique ignora a A oculta); voltar ao 0 mostra A intacta e refocada. 58 testes no boot.

### Adicionado (Fase 5, bloco 21 — clipboard mediado com histórico opt-in)
- `nexo.wm` v1.12: área de transferência **mediada pelo compositor** — `clipboard_set`/`clipboard_get` (até 256 B) só funcionam para a sessão **dona da entrada** (a janela focada ou, com grab, a capturada; erro remoto 6 para as demais): aplicativos em segundo plano não conseguem farejar nem injetar conteúdo. **Histórico opt-in**: desligado por padrão; `clipboard_enable_history` liga um anel de 4 entradas lido por `clipboard_history{index}` (0 = mais recente), sob a mesma mediação.
- `utest` modo 36 + auto-teste de boot `user_wm_clipboard`: a sessão focada escreve/lê; a outra sessão é negada nas duas direções; após o clique mover o foco, a segunda sessão lê o que a primeira escreveu (o conteúdo atravessa sessões pela mediação) e a primeira passa a ser negada; o histórico só existe depois do opt-in e devolve as entradas na ordem. 59 testes no boot.

### Adicionado (Fase 5, bloco 22 — notificações e não-perturbe)
- `nexo.wm` v1.13: `notify{title}` — qualquer sessão (inclusive em segundo plano) publica um aviso e o compositor desenha um **banner de sobreposição** (topo direito do display 0, acima de janelas e contextos, com o título rasterizado); `dismiss_notification` o remove. **Controle de atenção**: `set_dnd{enabled}` liga o não-perturbe (avisos descartados) e é mediado pela posse da entrada (erro 6) — só o app com que o usuário interage muda o modo.
- `utest` modo 37 + auto-teste de boot `user_wm_notify`: aviso pinta o banner sobre a janela de fundo (pixel conferido), `dismiss` restaura, DND descarta o aviso, uma sessão em segundo plano **pode** notificar mas **não pode** mudar o DND. 60 testes no boot.

### Adicionado (Fase 5, bloco 23 — drag-and-drop por grants)
- `nexo.wm` v1.14: `drag_start{data}` (até 256 B) — só a sessão dona da **entrada** inicia um arrasto (erro 6). No **soltar** (BTN_LEFT release), o compositor faz o hit-test no contexto ativo e entrega os dados **só à sessão dona da janela alvo** (evento `drop{surface,data}`) — o grant: nenhuma outra janela pode ler o payload. Soltar no vazio, ou durante uma captura (grab), descarta.
- `utest` modo 38 + auto-teste de boot `user_wm_dnd`: a sessão sem a entrada é negada; a dona arrasta "doc" e solta sobre a janela da outra sessão, que recebe o evento com os dados; um segundo arrasto solto no vazio não entrega nada (conferido por `try_recv` após sincronizar pela tecla). 61 testes no boot.

### Adicionado (Fase 5, bloco 24 — arquitetura de leitor de tela)
- `nexo.wm` v1.15: **eventos semânticos de acessibilidade** — `a11y_subscribe{chan}` assina um canal onde o compositor emite eventos `a11y{kind,surface,text}`: foco mudou (kind 1, com o **título** da janela — novo `set_title{id,title}`; emitido no clique **e** no ciclo Meta+Tab), aviso publicado (kind 2, texto) e Contexto trocado (kind 3). É a arquitetura do leitor de tela: um fluxo semântico desacoplado da renderização, que tecnologia assistiva consome sem raspar pixels.
- `utest` modo 39 + auto-teste de boot `user_wm_a11y`: o cliente faz o papel do leitor — assina o fluxo e confere, na ordem, os eventos de foco (com títulos "editor"/"chat"), de aviso ("oi") e de troca de contexto. 62 testes no boot.

### Adicionado (Fase 5, bloco 25 — mecanismo da Faixa de Atividades e privilégio de shell)
- `nexo.wm` v1.16: **modelo de privilégio de shell** — a sessão bootstrap (a que o kernel entrega ao subir o compositor) é o *shell*; só ela usa `surface_info{index}` (enumera janelas: id, contexto, display, título) e `activate{id}` (troca para o Contexto da janela, traz à frente e foca, com evento a11y — o clique da Faixa de Atividades). Sessões comuns recebem o erro remoto 7.
- `utest` modo 40 + auto-teste de boot `user_wm_shell`: o shell enumera "editor" (ctx 0) e "chat" (ctx 1), ativa "chat" (a saída passa a mostrá-la — troca de contexto + frente + foco conferidos por pixel) e uma sessão comum é negada em `surface_info` e `activate`. 63 testes no boot.

### Adicionado (Fase 5, bloco 26 — escala fracionária e redução de movimento)
- `nexo.wm` v1.17: `set_scale{id,num,den}` — escala fracionária **por janela**: o retângulo de exibição vira buffer×num/den (200% = 2/1, 150% = 3/2) e a composição escala por vizinho mais próximo, sem realocar o buffer (o desacoplamento do bloco 15). Validação de razões e limites da saída.
- Preferência de acessibilidade **redução de movimento**: `set_reduce_motion{enabled}` (mediado pela posse da entrada, erro 6) e `prefs{}` de leitura livre — apps consultam para desligar animações.
- `utest` modo 41 + auto-teste de boot `user_wm_scale`: 200% e 150% conferidos por pixel (área ampliada mostra o conteúdo; fora dela, fundo), razão inválida recusada; a preferência muda só pela sessão dona da entrada e é legível por qualquer uma. 64 testes no boot.

### Adicionado (Fase 5, bloco 27 — mecanismo da Central de Ações)
- `nexo.wm` v1.18: o compositor registra as **8 notificações mais recentes** — inclusive as suprimidas pelo não-perturbe (o DND corta a interrupção; a Central preserva o registro). O shell lista por índice (`notification_info`, 0 = mais recente) e limpa (`notifications_clear`, que também remove o banner); sessões comuns recebem o erro 7.
- `utest` modo 42 + auto-teste de boot `user_wm_center`: publica "a", liga o DND, publica "b" (sem banner) e o shell lê ["b", "a"] na ordem; `clear` esvazia; sessão comum é negada. 65 testes no boot.

### Adicionado (Fase 5, bloco 28 — navegação por teclado no toolkit)
- `nexo-ui`: `Nav` — índice de foco sobre N widgets com `focus_next()`/`focus_prev()` (Tab/Shift+Tab) ciclando nas duas direções; `draw_focus_ring` desenha o anel de foco (cor de acento, 1 px por fora do widget) para a navegação ser visível. Com o Meta+Tab do compositor (entre janelas), forma a base da navegação por teclado. Testes de host (ciclo com wrap; anel desenhado sem cobrir o widget).

### Adicionado (Fase 5, bloco 29 — Faixa de Atividades de verdade (`shellui`) e eventos de ponteiro)
- `nexo.wm` v1.19: evento `pointer{surface,x,y}` — o clique (BTN_LEFT press) é **entregue à janela clicada** em coordenadas locais, após o foco/frente (durante uma captura, cliques seguem engolidos). É a base de qualquer widget interativo. `wm_recv_key` dos testes passou a pular eventos de ponteiro (o novo evento pegou uma dependência implícita no teste de a11y, corrigida drenando-o).
- `services/shellui`: o **shell gráfico** — na sessão privilegiada do compositor, desenha a **Faixa de Atividades** (barra no rodapé do display 0, tema `nexo-ui`, uma célula por janela via `surface_info`), faz ***broker* de sessões** (um app pede "sess" pelo canal e recebe a ponta de uma sessão `nexo.wm` nova — a arquitetura de como apps chegam ao compositor) e, ao receber o clique na célula (evento `pointer`), **ativa** a janela (`activate`).
- `utest` modo 43 + auto-teste de boot `user_shellui` (**multi-processo**: wm + shellui + app): o app obtém a sessão pelo broker, cria a janela "app1", pede o `sync` da barra e confere os pixels (fundo e célula); o clique sintético na célula faz o shell ativar a janela — conferido porque a tecla seguinte chega a ela. 66 testes no boot. **A Faixa de Atividades está completa.**

### Adicionado (Fase 5, bloco 30 — painel visual da Central de Ações)
- `services/shellui`: o clique na **zona direita** da Faixa (x ≥ 50) abre/fecha a **Central de Ações** — um painel (40×28, tema `nexo-ui`, borda de acento) com um marcador por notificação do registro do compositor (inclusive as suprimidas pelo não-perturbe), lido pelo privilégio de shell (`notification_info`); o segundo clique o destrói.
- `utest` modo 44 + auto-teste de boot `user_shellcenter` (wm + shellui + app): publica dois avisos, clica na zona direita (recebe "copen" do shell) e confere por pixel o fundo do painel, os dois marcadores e a terceira linha vazia; o segundo clique fecha ("cclosed", pixel volta ao fundo da cena). 67 testes no boot. **A Central de Ações (MVP) está completa.**

### Adicionado (Fase 6, bloco 1 — o primeiro aplicativo: calculadora)
- `services/calc`: o **primeiro aplicativo real** da plataforma — janela com visor e botões (`nexo-ui`) acionados pelos **eventos `pointer`** do compositor (clique em coordenadas locais). Versão inicial com `1`, `+`, `2`, `=`; ao calcular, mostra o resultado no visor e o escreve no **clipboard mediado** (permitido porque o clique deu o foco à calculadora). Recebe a sessão `nexo.wm` de um orquestrador por canal, que também é o seu **cordão de vida** — sem isso, app e compositor esperavam um pelo outro no encerramento (deadlock pego pelo auto-teste, corrigido com `try_recv` + `wait_any` sobre {sessão, canal}).
- `utest` modo 45 + auto-teste de boot `user_calc` (**wm + calc + driver**): o driver entrega a sessão à calc, clica "1 + 2 =" nas coordenadas dos botões e lê o resultado "3" pelo clipboard após retomar o foco. A pilha inteira num gesto de usuário: entrada → compositor → app → toolkit → clipboard. 68 testes no boot.

### Adicionado (Fase 6, bloco 2 — formato de pacote e manifesto)
- Formato **NEXOPKG1** v1 (`docs/spec/pkg.md`): cabeçalho com CRC32 do payload + **manifesto** textual (`name`/`version`/`entry`/`perms` — a lista de **permissões declaradas** do app; chaves desconhecidas são erro: o manifesto é a superfície de auditoria) + arquivos (`name_len`/nome/`data_len`/dados; bytes sobrando são erro).
- `libraries/pkg` (`nexo-pkg`): parser `no_std`/sem alocação/`forbid(unsafe_code)` — `Package::parse` valida assinatura, versão, CRC e a tabela inteira de arquivos; `Manifest` com `perms()`/`declares()`. Testes de host: round-trip, rejeições (magic/versão/CRC/manifesto), fuzz-lite de truncamentos e mutações de 1 byte sem pânico.
- `tools/nexo-pack` (`build`/`inspect`): empacota e inspeciona (revalidando CRC e estrutura); validado empacotando o binário real da calculadora.

### Adicionado (Fase 6, bloco 3 — gerador de projeto e documentação do SDK)
- `tools/nexo-new <nome>`: gera um aplicativo **funcional** (janela `nexo-ui` seguindo o contrato de apps — sessão `nexo.wm` recebida pelo canal do orquestrador, que é o cordão de vida; eventos `pointer` logados) já **registrado** no workspace de services e no initrd, com `manifest.txt` NEXOPKG1 pronto para o `nexo-pack`. Validado gerando um app do zero e compilando sem avisos.
- `docs/sdk.md`: a documentação do SDK — os crates, o **contrato de um aplicativo** (canal do orquestrador/cordão de vida, janela/commit, eventos e RPCs no mesmo canal, mediações de clipboard/notificações/captura/preferências), o fluxo `nexo-new` → `cargo build` → `nexo-pack`, e os exemplos reais (`calc`, `greeter`, `shellui`).

### Adicionado (Fase 6, bloco 4 — instalação transacional)
- `libraries/inst` (`nexo-inst`, `no_std`/sem alocação/`forbid(unsafe)`): instala pacotes NEXOPKG1 num **diretório versionado** (`/apps/<nome>.v<N>/`, com o manifesto junto) e grava o ponteiro `/apps/<nome>.cur` **por último** — a escrita do ponteiro é o commit (o espírito do NexoFS). Antes do commit, a versão corrente não muda e a retentativa re-preenche os mesmos caminhos; o pacote é validado inteiro antes de tocar o disco. Agnóstica do transporte (trait `AppFs`).
- Testes de host: instalação + atualização com o ponteiro virando e a v1 intacta; pacote corrompido não toca nada; **corte de energia simulado após cada operação** (mock com falha injetada) — em todo ponto de corte, ou a versão velha segue corrente e completa, ou a nova está completa; a retentativa sempre conclui.
- `nexo-pkg` ganhou `Package::manifest_bytes()` (o instalador grava o manifesto junto do app).
- `utest` modo 46 + auto-teste de boot `user_install` (blockdev + fs + instalador): adaptador `AppFs` sobre o protocolo tipado `nexo.fs` (stat/create/truncate/write/read por inode, em blocos de 3900 B); instala v1, lê de volta pelo caminho versionado, atualiza para v2 (ponteiro vira; v1 intacta) e rejeita um pacote com CRC quebrado sem mudar nada. 69 testes no boot.

### Adicionado (Fase 6, bloco 5 — executar da memória (`process_spawn_mem`))
- Syscall `process_spawn_mem` (32): cria um processo a partir de um **ELF na memória do chamador** (≤ 2 MiB), com argumento e transferência de handles como no `process_spawn` — o elo "instalar → **executar**": o lançador lê o ELF da instalação e o entrega aqui. Mesmas validações e isolamento de qualquer processo (faixa de usuário, W^X, espaço próprio, só os handles transferidos). A retirada de handles para o filho foi fatorada (`take_spawn_handles`) e é compartilhada pelos dois spawns.
- `utest` modo 47 + auto-teste de boot `user_spawn_mem`: o kernel entrega o ELF do `echo` num `MemoryObject`; o cliente o mapeia, spawna da memória com um canal de controle, conversa com o filho (pedido → "echo: …"), fecha o controle (o filho sai limpo com 0) e confirma que lixo não-ELF é recusado. 70 testes no boot.
- Correção no `user_install`: o disco de dados **persiste** entre boots — as asserções de versão viraram relativas à corrente (o teste é idempotente; verificado em dois boots consecutivos).

### Adicionado (Fase 6, bloco 6 — o laço completo: lançador com permissões declarativas)
- **O laço da plataforma fecha**: empacotar (NEXOPKG1) → instalar transacionalmente (NexoFS) → **lançador** lê o manifesto instalado, concede capacidades **só pelas permissões declaradas** e executa o app **da instalação** (`process_spawn_mem`).
- Imposição de permissões demonstrada nos dois sentidos: o app com `perms=ipc` nasce com o canal de controle e funciona (pedido → eco → saída limpa); o **mesmo binário** instalado **sem** a permissão nasce **sem** o handle — a capacidade não existe para ele (modelo de capabilities: negação por omissão, sem checagens em runtime).
- `utest` modo 48 + auto-teste de boot `user_launcher` (blockdev + fs + lançador; o kernel entrega o ELF real do `echo` por `MemoryObject`): instala os dois pacotes no NexoFS e exercita concessão e negação. 71 testes no boot.

### Adicionado (Fase 6, bloco 8 — app gráfico instalado (permissão `janelas`))
- O mapeamento permissão→capacidade ganhou **`janelas`**: o lançador abre uma sessão do compositor para o app **só se o manifesto a declara**, e a entrega pelo canal do app (o contrato `"sess"`).
- `utest` modo 49 + auto-teste de boot `user_launch_gui` (blockdev + fs + **wm** + lançador; o kernel entrega o ELF real da **calculadora**): empacota, instala no NexoFS e lança — a janela **"calc" aparece** (conferida por `surface_info` na sessão shell) vinda de um binário executado **da instalação, não do initrd**; o app encerra limpo pelo cordão de vida. O mesmo binário instalado **sem** a permissão nasce sem sessão e sai com o próprio erro. 72 testes no boot. **Um aplicativo gráfico instalado, rodando com capacidades concedidas pelo manifesto.**

### Adicionado (Fase 6, bloco 9 — Configurações)
- `services/config`: o app de **Configurações** — janela com dois *toggles* reais (`RM` = movimento reduzido, `NP` = não-perturbe) acionados pelos eventos `pointer`. O desenho do modelo se fecha: o clique que aciona o toggle é o que dá o **foco** à janela, e a posse da entrada é exatamente o que `set_reduce_motion`/`set_dnd` exigem — a mediação do compositor trabalhando a favor do app certo, contra os errados.
- `utest` modo 50 + auto-teste de boot `user_config` (wm + config + driver): clica os toggles e confere os **efeitos reais** de fora — `prefs` reflete o movimento reduzido (liga/desliga) e, com o não-perturbe, um aviso não desenha banner (com DND off, desenha). 73 testes no boot.
- Lição de teste registrada no código: a saída composta é memória compartilhada e o `composite` pinta o fundo antes do banner — leituras de pixel concorrentes a recomposições devem **esperar a convergência** (`wm_wait_px`), nunca ler uma vez (uma corrida transiente foi pega e corrigida; suíte verde 3× seguidas).

### Adicionado (Fase 8, bloco 50 — reset preservando arquivos)
- `services/reset`: limpa o volume de dados em **pós-ordem** (o `unlink` do nexo.fs remove diretórios vazios), preservando a subárvore do usuário e os ancestrais dela; o canal do fs viaja **emprestado por pedido e volta com a resposta** (o padrão de capacidade do editor/backup). O reset do sistema já existia por outro caminho (slots A/B + recuperação) — este cuida dos dados.
- "Quando possível" de verdade: no fluxo testado, o diretório preservado é **espelhado no disco de backup antes** do reset (serviço `backup` — integração real entre os dois). Auto-teste `user_reset` (95º), autocontido na subárvore `/rst-teste` (o pedido leva a **base** explícita — `limpa <base> <keep>`; o reset de fábrica usa base `/`): o home do teste intacto byte a byte, estado de sistema e entulho aninhado removidos; idempotente entre boots e sem tocar a persistência alheia do volume (a 1ª versão apagava o `/boot.count` do cenário 'storage' do CI — pego pelos cenários).

### Adicionado (Fase 8, bloco 49 — ambiente de recuperação independente)
- `\nexo\recovery\` (kernel + initrd): cópia do sistema que **nenhuma atualização toca** — só o build a grava. O loader cai nela quando os dois slots falham estruturalmente ou quando nenhum é elegível (updates esgotados sem confirmação): **a máquina sempre arranca**.
- `tools/test-recovery`: as duas variantes provadas — kernels de A e B corrompidos, e estado inelegível — com a suíte inteira (94/94) passando dentro do ambiente de recuperação; os auto-testes de esp/vfs aceitam o recovery como terceiro caminho de kernel íntegro.

### Adicionado (Fase 8, bloco 48 — atualização atômica por dentro do FAT)
- `nexo-fat` ganhou **escrita mínima**: `rewrite_file` reescreve o conteúdo de um arquivo existente (FAT12/16/32, todas as cópias da FAT, entradas 12 bits inclusive na fronteira de setor) com ordem à prova de cortes — cadeia e dados NOVOS primeiro, a entrada de diretório é o commit, a cadeia antiga liberada por último; um corte deixa o arquivo antigo intacto ou o novo completo, nunca um rasgado. Interop provado nos testes de host: o mtools extrai e valida byte a byte o que NÓS gravamos numa FAT32 real.
- `upd` "aplica": a **atualização atômica** — copia `kernel.elf` + `initrd` do slot ativo para o inativo por dentro do FAT do disco de boot e o marca pendente (prioridade 3, 3 tentativas); "verifica" compara os dois slots byte a byte; "confirma" agora normaliza prioridades sem tocar em update pendente em voo. Auto-teste `user_update` (94º).
- `tools/test-update`: o ciclo entre boots reais — boot 1 aplica (suíte), boot 2 arranca pelo slot pendente (`tentativas 2`), a suíte inteira passa nele e o health check o confirma como corrente. Com a suíte ativa o sistema **alterna os slots a cada par de boots** — o mecanismo inteiro é exercitado continuamente; `test-ab`/`test-rollback` resetam o estado ao canônico nas suas cópias.

### Adicionado (Fase 8, bloco 47 — health check ligado no sistema real)
- O health check A/B saiu do laboratório: o `devmgr`, assim que o armazenamento sobe, spawna `ahcidev` no SATA integrado (o disco de boot gravável) + `upd` e **confirma o slot arrancado a cada boot saudável do sistema completo** (best-effort: imagem sem layout A/B só gera aviso). O cenário 'boot' do CI passa a exigir o marcador da confirmação.
- `tools/test-rollback` endurecido com **falha genuína**: o initrd do slot pendente é corrompido — o kernel (ELF válido) arranca, o sistema morre sem espaço de usuário, ninguém confirma, e o boot seguinte volta sozinho ao slot antigo; no boot do rollback, a suíte passa inteira e o slot bom é re-confirmado pelo caminho de produção (o mesmo `devmgr`→`upd` — visto duas vezes no log: suíte e sistema).

### Adicionado (Fase 8, bloco 46 — health check pós-boot e rollback automático)
- `services/upd`: o health check pós-boot do A/B. Com o canal `nexo.block` do **disco de boot real** (o `ahcidev` restrito ao SATA integrado serve a porta 5, onde o QEMU anexa a imagem), localiza `\nexo\slots.bin` no ESP (GPT → FAT, novo `nexo-fat::first_sector_lba`) e **"confirma"** marca o slot arrancado como saudável (sucesso = 1, tentativas repostas), gravando o setor de volta; **"estado"** relê do disco. Auto-teste `user_slots` (93º).
- **Rollback automático provado** (`tools/test-rollback`): slot A marcado como atualização pendente (1 tentativa) → boot 1: o loader consome a tentativa e persiste; sem a suíte ninguém confirma → boot 2: A inelegível, o loader volta sozinho ao B; o estado relido da imagem confere byte a byte. E o **caminho feliz por acidente glorioso**: com a suíte ativa, o `user_slots` confirmou o slot pendente no primeiro boot e o segundo arrancou por ele com as tentativas repostas — o ciclo de vida completo de uma atualização A/B, dos dois lados.
- Cenários do `test-qemu` atualizados para o layout A/B (`/nexo/a/kernel.elf`, `ls /boot/nexo/a`) — a causa do CI vermelho anterior; validados todos localmente (10/11; `gdb` com timeout pré-existente do ambiente local, verde no CI).

### Adicionado (Fase 8, bloco 45 — layout A/B de boot com fallback estrutural)
- O ESP passa a carregar **dois slots completos do sistema** (`\nexo\a\` e `\nexo\b\`: kernel + initrd) e o estado `\nexo\slots.bin` (512 bytes — um setor, reescrito in-place: prioridade/tentativas/sucesso por slot + CRC32/IEEE; formato em `nexo_boot_abi::slots`, com testes de host, espelhado byte a byte pelo `build-image` em python e documentado no §1.1 da spec de boot). ADR-0010 começa a virar código.
- O loader escolhe o slot **elegível** de maior prioridade (elegível = confirmado ou com tentativas restantes), desconta uma tentativa de slot pendente **antes** de carregar — um travamento consome a tentativa — e **cai para o outro slot** se o kernel do escolhido não for um ELF válido; sem `slots.bin`, vale o layout clássico (imagens antigas seguem arrancando). O estado é gravado de volta no ESP pelo próprio loader (o QEMU agora arranca pelo disco gravável — `bootindex=0` — em vez do anexo virtio somente-leitura).
- `ahcidev` agora faz o **bring-up da porta** sozinho (spin-up + COMRESET + espera do enlace; a validação do dispositivo fica com o IDENTIFY, já que a assinatura só aparece após o primeiro FIS D2H): o driver dependia, sem saber, do *connect-all* do firmware — com `bootindex` o OVMF conecta só o dispositivo de boot e uma porta nunca tocada ficava "sem disco". Lição de campo do bloco.
- `tools/test-ab`: prova do fallback — corrompe o kernel do slot A numa **cópia** da imagem e o boot cai para o B com a suíte inteira verde; os auto-testes de esp/vfs aceitam qualquer slot com ELF íntegro (exatamente a semântica A/B: um slot quebrado não é doença).

### Adicionado (Fase 5, bloco 44 — duplo buffer com seqlock de frame no compositor)
- A saída composta do `wm` agora tem **duplo buffer publicado por troca, não por cópia**: o `MemoryObject` segue o layout `nexo_wm::frame` (página de cabeçalho com seqlock + dois buffers de frame); a composição vai sempre no buffer de trás e publica trocando o índice da frente sob o seqlock (`seq` ímpar = troca em andamento). O frame que um leitor pode estar lendo **nunca é escrito** — leitores que sigam o protocolo (seq par → ler frente → reconferir seq) jamais observam um frame rasgado.
- Auto-teste `user_wm_flip` (92º) + modo 66 do `utest`: confere magic/dimensões do cabeçalho, o front alternando a cada commit (trocou, não copiou), `frames` avançando, `seq` par — e a garantia anti-rasgo: o frame publicado anterior continua byte a byte intacto após compor o seguinte. Os 25 pontos de leitura da saída nos testes existentes passaram ao protocolo do seqlock por um único funil (`wm_px`).

### Adicionado (Fase 8 antecipada, bloco 43 — backup e restauração entre discos)
- `services/backup`: espelha os arquivos de um diretório entre **dois volumes `nexo.fs` independentes em discos físicos distintos** (principal virtio-blk ⇄ backup AHCI) — perder um disco não perde os dados. O canal do fs de origem é **emprestado por pedido e devolvido com a resposta** (o fs atende um cliente por vez; mesmo padrão de capacidade do editor); o serviço só copia, nunca apaga.
- Auto-teste `user_backup` (91º) + modo 65 do `utest`: duas pilhas completas de armazenamento no mesmo boot (blockdev+fs e ahcidev+fs), espelha 2 arquivos, provoca um "desastre" no principal (arquivo apagado + arquivo adulterado) e a restauração devolve os conteúdos originais, byte a byte.

### Adicionado (Fase 6, bloco 42 — headers C e o primeiro processo em C)
- `abi/c/nexo.h`: headers C **freestanding** da ABI (wrapper inline da convenção de syscall — rax→status, rdx→valor, rcx/r11 destruídos — e helpers `nexo_exit`/`nexo_log`/`nexo_yield`/`nexo_wall_epoch`); `abi/c/nexo_syscalls.h` é **gerado da fonte Rust** por `tools/nexo-cheaders` (49 definições — números de syscall e códigos de `Status` nunca desatualizam).
- `examples/c/hello.c` + `tools/build-c-demo`: o primeiro processo em C do sistema — compilado sem libc (clang `--target=x86_64-unknown-none-elf`) e linkado com `rust-lld` no mesmo layout dos serviços (estático, no-PIE, `_start`); 1144 bytes, empacotado no initrd como `hello-c` quando o host tem o toolchain (CI tem).
- Auto-teste `user_c_hello` (90º): o binário C loga pelo kernel ("ola do C freestanding") e sai 0 — a ABI funciona fora de Rust.

### Adicionado (Fase 7 antecipada, bloco 41 — NexoFS sobre AHCI)
- Auto-teste `user_ahci_fs` (89º): a pilha de armazenamento inteira (fs + cliente persistente `boot.count`) sobre o **terceiro** controlador — NexoFS formatado/montado no disco AHCI via `ahcidev`, sem mudar uma linha de fs nem de cliente. A substituibilidade por protocolo agora está provada em virtio-blk, NVMe e AHCI, tanto no nível cru quanto com o sistema de arquivos completo.

### Adicionado (Fase 7 antecipada, bloco 40 — driver AHCI)
- `services/ahcidev`: **driver SATA/AHCI** em ring 3 (QEMU `ich9-ahci`) — ABAR (BAR5) por concessão, modo AHCI habilitado, porta com disco detectada por SSTS/SIG, command list/FIS receive próprios (a porta é parada e reprogramada), comandos ATA **READ/WRITE DMA EXT** (LBA48) com PRDT de página única e **IDENTIFY** real (capacidade LBA48 + serial com os bytes de word trocados, como manda a ATA). MVP síncrono/polling.
- Servindo o MESMO `nexo.block` v0: o cliente cru do modo 8 agora validou **três controladores** (virtio-blk, NVMe, AHCI) sem mudar uma linha — auto-teste `user_ahci` (88º) com persistência entre boots. `run-qemu` anexa um disco AHCI por padrão (par do disco de dados, `--no-ahci` desativa).

### Adicionado (Fase 8 antecipada, bloco 39 — crash dumps protegidos)
- No pânico, o kernel grava um **crash dump** (mensagem, local, uptime, thread e backtrace **simbolizado**) na sub-área reservada do disco de dados (setores `cap-16..cap-8`, disjuntos dos testes crus). Caminho de EMERGÊNCIA por desenho: nada de alocação nem locks — páginas de DMA pré-alocadas e **BARs mapeados no boot** (no q35 os BARs virtio ficam acima de 4 GiB, fora do physmap — page fault dentro do pânico foi o achado do bloco), e um mini virtio-blk síncrono (reset → fila própria → uma escrita → poll) que escolhe o disco **gravável** (pula o de boot, feature RO).
- `tools/nexo-disk crashdump [--clear]` extrai/limpa o dump; o cenário `panic` agora valida o conteúdo gravado (host_check) a cada CI. Consentimento de envio pende (o dump nunca sai da máquina sozinho — ADR-0011).

### Alterado (Fase 6, bloco 38 — bring-up no protocolo tipado `nexo.svc`)
- Fecha a limitação anunciada na release 0.2 ("canais do bring-up com mensagens cruas"): `svcmgr`/`echo`/`echo-client` agora falam **`nexo.svc` v1.0** (`idl/svc.idl`): `serve{chan}` (sem resposta, por desenho — o svcmgr não bloqueia num serviço que pode cair), `connect` → canal (erro remoto 2 = "tente de novo", serviço reiniciando) e `echo{text}`. Comportamento, logs e a queda proposital preservados (reinícios do `user_services` intactos).
- Os testes do lançador (que executam o binário do `echo` instalado) migraram para o helper tipado `svc_echo_round` — a primeira troca de protocolo de um "app da loja" exercitada de ponta a ponta.

### Segurança (Fase 8 antecipada, bloco 37 — quota de memória compartilhável por processo)
- `memory_create` agora respeita uma **quota por processo criador**: `SHM_PAGES_MAX_PER_PROCESS` = 4096 páginas (16 MiB), devolvida quando o objeto morre (`Weak` para o criador no `MemoryObject` — sem ciclo de referência); exceder devolve `NoMemory`. Fecha o vetor de DoS local por exaustão de quadros via objetos compartilháveis (threat model §2/§3 atualizado com os tetos compostos de IPC).
- Auto-teste `shm_quota` (87º) + modo 64 do `utest`: 16 objetos de 256 páginas cabem, o 17º falha com `NoMemory`, e fechar tudo **devolve** a quota (criação volta a funcionar); sem vazamento de quadros.

### Adicionado (Fase 6, bloco 36 — depuração remota GDB/LLDB)
- `tools/nexo-debug [símbolo]`: depuração interativa do kernel em um comando — QEMU pausado com gdbstub em `:1234` + lldb (macOS) ou gdb (Linux) conectado com os símbolos de `kernel.elf` e breakpoint (padrão `kmain`).
- Cenário **`gdb`** no `tools/test-qemu` (roda no CI): conecta o depurador disponível, breakpoint em `kmain`, confirma o *hit*, desanexa e verifica que o convidado completa o boot (código 33). Sem depurador no host, o cenário pula declaradamente.

### Segurança (Fase 8 antecipada, bloco 35 — trace atrás de capability de depuração)
- Fecha a última prioridade "pequena" do threat model: `SYS_TRACE` (ligar/ler) agora exige a **capability de depuração** (`Object::Debug`, `KIND_DEBUG` = 5) — posse explícita apresentada como handle, o mesmo desenho das concessões de dispositivo. Sem ela: `Denied` (testes negativos de enable e read no `user_trace`). Op 3 (total) segue livre. Endurecimento no mesmo dia da introdução da syscall (ABI experimental; nota de compat no `syscall-abi.md`).
- `run-qemu`: o disco NVMe default agora é o **par do disco de dados** (`<disk>-nvme.img`) — execuções concorrentes (stress de 7 dias + suíte) não disputam mais o mesmo arquivo (o QEMU tranca imagens; colisão vista em campo ao disparar o stress longo).

### Releases `v0.2-userspace` e `v0.3-storage` (2026-09-01, tags assinadas)
- **`v0.2-userspace`** (gate F2): processos isolados, IPC por handles/canais com direitos, init + svcmgr com reinício de serviço, ABI v0→v1 documentada, IDL/protocolos tipados, shell de diagnóstico, fuzzing contínuo. Publicada na sequência da `v0.1-kernel`; limitações do rascunho atualizadas (memória compartilhada e espera múltipla existem desde a Fase 5).
- **`v0.3-storage`** (gate F3): blockdev VirtIO + NexoFS v0 (commits atômicos por setor) + fs/vfs/espfs; reiniciar preserva dados; queda de driver não corrompe o kernel; **cortes de energia simulados** (cenário `powercut` + corte injetado em cada escrita nos testes de host). Extra pós-gate: a mesma pilha rodando sobre NVMe.
- A partir da 0.2 as tags são **assinadas** (SSH, política do RELEASE.md). Evidência do stress de 24 h anexada à release `v0.1-kernel` (log gzip).
- `Makefile`: margem do stress prolongado agora é **proporcional** (+900 s + 1% da duração) — 7 dias de TCG sob carga atrasam mais do que uma folga fixa cobre.

### Segurança (Fase 8 antecipada, bloco 34 — saída composta é privilégio do shell)
- Fecha a prioridade nº 3 do threat model: `output` do compositor agora é **privilégio do shell** (sessão 0; erro 7 para as demais — a saída composta é a tela inteira). Teste negativo no `user_wm_shell`; IDL anotada.
- O shell (`shellui`) ganhou `"saida"` no protocolo do pipe: exporta a tela ao **seu orquestrador** (resposta do wm + handle encaminhados) — os testes de shell passaram a pedir a tela por esse caminho, que é o desenho certo: quem não é shell só vê a tela se o shell der.

### Adicionado (Fase 8 antecipada, bloco 33 — threat model + auditoria mecânica de `unsafe`)
- `docs/security/threat-model.md`: **threat model por subsistema** (kernel/syscalls, IPC — com o bug do coletor como estudo de caso —, memória/DMA, drivers, compositor+entrada, armazenamento, pacotes, rede, observabilidade, boot/atualização): ativos, adversário, superfície, mitigações **com os testes que as provam** e lacunas honestas. Prioridades decorrentes viram blocos (as pequenas: `output` do wm e `SYS_TRACE` como privilégio).
- `tools/nexo-unsafe-audit` **no `make lint`**: todo uso de `unsafe` (bloco/fn/impl/extern) exige `SAFETY:`/`# Safety` adjacente — 424 usos na árvore, 17 sítios sem justificativa corrigidos ao ligar o gate, 0 restantes. Inventário atualizado (`docs/unsafe-inventory.md`).

### Adicionado (Fase 7 antecipada, bloco 32 — fusos horários na nexo-cal)
- `nexo-cal`: `civil_from_epoch_tz(secs, offset_min)` — data civil com deslocamento de fuso em minutos (leste positivo; saturado em 1970). O kernel segue fornecendo só UTC (`debug_info` 7), por desenho: fuso é política de quem apresenta. Testes de host cruzam a meia-noite nos dois sentidos (UTC−3 volta para 31/08; UTC+9 avança). Pendem persistência da escolha e UI.

### Adicionado (Fase 7 antecipada, bloco 31 — NVMe com múltiplos pedidos em voo)
- `nvmedev` agora é **assíncrono** no espírito do blockdev: até 4 pedidos em voo (um slot de página PRP1 por pedido; CID = slot), conclusões colhidas por `poll_completion` e respostas entregues **na ordem de chegada** — respostas imediatas (`capacity`/`identity`/erros de validação) entram na mesma fila (`Ready`) para não furar a ordem; bloqueia no canal só sem nada em voo, senão dorme em `irq_wait`.
- Auto-teste `user_nvme_pipe` (86º) + modo 63 do `utest`: dispara 4 escritas + capacidade + 4 leituras **sem esperar** e colhe as 9 respostas exatamente em ordem, com os padrões conferidos byte a byte; o teste de pipeline do modo 8 também passou a exercitar o NVMe.

### Adicionado (Fase 6, bloco 30 — trace de syscalls + visualizador)
- Kernel: **trace de syscalls** — anel global de 4096 eventos `{tsc, pid, nr}` (16 B, `repr(C)`); desabilitado custa um load relaxado, habilitado um `fetch_add` + stores relaxados. Syscall **aditiva** 33 (`trace`): liga/desliga, leitura não destrutiva (dos mais antigos disponíveis aos mais novos) e total gravado. ABI v1 segue aditiva (`SYS_MAX = 33`; specs atualizadas).
- `tools/nexo-trace`: **visualizador** — agrega linhas `[TRACE] tsc= pid= nr=` de logs seriais por syscall (nomes lidos de `abi/syscall/src/lib.rs`, sempre em dia com a ABI) e por processo, com janela de TSC.
- Auto-teste `user_trace` (85º) + modo 62 do `utest`: liga o trace, faz 50 yields, lê o anel e confere os próprios eventos (pid, nr, TSC monotônico); despeja uma amostra no formato do visualizador (validado contra o log real do boot). Pendem o profiler por amostragem e eventos além de syscalls.

### Adicionado (Fase 6, bloco 29 — índice do repositório validado no boot)
- `user_install` agora também escreve um `indice.txt` no `/repo` do NexoFS real e o relê pelo parser oficial (`nexo_pkg::RepoIndex`): entrada listada encontrada, ausente não — fecha a pendência declarada no bloco 28 (que ficou sem teste de boot de propósito, durante a janela final do stress de 24 h).

### Corrigido (release: SHA256SUMS agora é gerado pelo build)
- O `build/SHA256SUMS` estava **estagnado desde 2026-08-29** — o "gerado por `tools/build-image`" do RELEASE.md nunca tinha sido implementado, e os artefatos da release v0.1-kernel subiram com somas velhas (detectado na conferência pós-upload; anexos substituídos por um conjunto verificado). Agora o `build-image` (apenas no build **canônico**, `build/nexo.img`) regrava o SHA256SUMS dos cinco artefatos de release; builds por cenário não o tocam.

### Infraestrutura (CI: retry também para travamento de firmware)
- `tools/test-qemu`: além de morte por sinal do host, agora é falha de **infraestrutura** (com retry) o timeout em que o convidado nem saiu do firmware (log sem `nexo-loader` — OVMF travou antes de entregar o controle; visto intermitentemente no host de desenvolvimento durante o `make ci` do gate F1, com os mesmos cenários passando isolados em seguida).

### Release `v0.1-kernel` (gate F1 ✅ — 2026-09-01)
- **24 horas de stress SMP sem erros** (QEMU TCG, 4 CPUs, em paralelo com um dia inteiro de desenvolvimento no host): 102 046 707 trocas de contexto, 11,3 M preempções, 7 648 552 processos criados, contador com lock **exato** (22 048 880 000/22 048 880 000), 1,69 T operações atômicas, 89,3 M map/unmap de páginas, 4/4 CPUs em todas as 86 401 amostras, `erros=0` em todas, quadros 128 490→128 487 e heap estável. Relatório: `docs/progress/2026-09-01-stress-24h.md`.
- Marco do plano (§7): kernel x86_64 com memória virtual isolada, SMP, escalonador preemptivo, relógio TSC e pânico com backtrace — estável sob stress prolongado. Tag `v0.1-kernel`.

### Adicionado (Fase 6, bloco 28 — índice do repositório + nexo-repo)
- `nexo-pkg`: **`RepoIndex`** — o `indice.txt` do repositório local (`nome versao` por linha, `#` comenta) validado no parse (`no_std`, sem alocação; UTF-8, limites de nome/versão, nome sem `/`), com `entries()`/`find()`. Informativo por desenho: a fonte da verdade é o `.npk`, validado inteiro na instalação.
- `tools/nexo-repo` (`build`/`check`): gera e confere o índice a partir dos `.npk` de um diretório; impõe a convenção **nome do arquivo = nome do manifesto** e rejeita duplicatas. Testado com pacotes reais do `nexo-pack`.
- `Makefile`: margem do stress prolongado `DURATION+300` → `+900` — o relógio do guest (TCG) atrasa sob carga do host e 300 s quase mataram um gate de 24 h (lição de 2026-09-01, documentada no alvo).

### Infraestrutura (CI: retry por sinal do host também nas fases de input e no shell)
- Terceiro SIGSEGV do QEMU no runner do GitHub em um dia, desta vez na fase 1 do cenário `input` — o caminho `Popen`+QMP que o retry do harness ainda não cobria. `run_input_phase` e `run_shell_scenario` agora reexecutam (até 3×) quando o código de saída é morte por sinal do host (245/246/250), como os demais cenários.

### Adicionado (Fase 7 antecipada, bloco 27 — MSI-X no nvmedev)
- `nvmedev` agora usa **MSI-X**: programação genérica da entrada 0 da tabela (walk da cap list PCI 0x11 — sem depender do transporte virtio), CQ de E/S criada com IEN/IV=0 e a espera de conclusão dorme em `irq_wait` (poll curto antes, para conclusões rápidas). Sem a capability, cai para polling e o log diz o modo. Ambos os testes NVMe verdes sob interrupção (o `fs` fez 83 pedidos dormindo entre conclusões).

### Adicionado (Fase 6, bloco 26 — repositório local de pacotes)
- `nexo-inst`: **repositório local** — `install_from_repo(fs, nome, buf)` instala a partir de `/repo/<nome>.npk` pelo caminho oficial: toda a validação de sempre (NEXOPKG1 completo no parse, revogação, transação com commit por último, coleta de versões). O buffer de leitura vem do chamador — o tamanho aceito fica sob controle de quem instala.
- Testes: host (instala do repositório; pacote ausente falha sem tocar nada; app revogado é recusado pelo MESMO caminho) e `user_install` no NexoFS real (idempotente entre boots, versões relativas).

### Adicionado (Fase 7 antecipada, bloco 25 — NexoFS sobre NVMe)
- Auto-teste `user_nvme_fs` (84º): a pilha de armazenamento **inteira** sobre o controlador novo — o `fs` monta um NexoFS no disco NVMe através do `nvmedev` (formatação automática na primeira vez: 4032 blocos no disco de 8 MiB) e o cliente persistente de sempre (`boot.count`, modo 9) roda **sem mudar uma linha**. Nem o `fs` nem o cliente sabem que o disco mudou de virtio-blk para NVMe: é o valor concreto da substituibilidade por protocolo.

### Adicionado (Fase 7 antecipada, bloco 24 — driver NVMe em modo usuário)
- `services/nvmedev`: **driver NVMe** em ring 3 — mesmo modelo de segurança dos demais drivers (concessão de UMA função PCI, BAR0 via `mmio_map`, DMA por páginas concedidas) e o mesmo protocolo **`nexo.block` v0** do blockdev: o cliente cru existente (modo 8 do `utest`) roda contra o NVMe **sem mudar uma linha** — substituibilidade por protocolo com um controlador PCI real. MVP síncrono: filas de admin + um par de E/S (qid 1), identify controller/namespace (serial, NSZE, LBAF 512 B), PRP1 único (≤ 1 página por pedido — cobre os 3584 B do protocolo), conclusões por *polling* com fase.
- `run-qemu`: anexa um disco **NVMe** de dados por padrão (`build/nvme-data.img`, 8 MiB; `--no-nvme` desativa; `-device nvme,serial=nexonvme` depois dos virtio para não mudar BDFs existentes).
- Auto-teste `user_nvme` (83º): capacidade, escrita/leitura com padrão e marcador de **persistência entre boots**, tudo pelo cliente `nexo.block` de sempre.

### Corrigido (kernel: coletor de pontas × handles em trânsito — Fase 6, bloco 23)
- **Bug de IPC achado pelo CI**: o coletor de pontas inalcançáveis (`collect_unreachable`), que roda na saída de cada processo, podia **fechar handles em trânsito** — entre o *pop* de uma mensagem da fila e a inserção dos seus handles na tabela do destinatário (e, simetricamente, entre a retirada da tabela do remetente e o enfileiramento no `send`, e na janela do `spawn`), os handles vivem só em mãos do kernel e eram invisíveis à marcação. Sintoma de campo (runner do CI, SMP): o `fs` via "cliente desconectou" com o canal em trânsito e o `user_editor` morria com 131; localmente a janela era estreita demais para reproduzir.
- Correção: janelas em-trânsito agora são demarcadas por uma guarda RAII (`ipc::InFlight`, criada **sob o lock do canal** no pop — `recv_guarded`/`try_recv_guarded` — e nos caminhos de `send`/`spawn`); o coletor espera as janelas zerarem e **valida a geração** depois de marcar — se uma janela abriu no meio, recomeça (ou adia a coleta: desistir nunca fecha nada vivo).
- Regressão coberta: teste novo `ipc_handoff` (82º — remetente envia uma ponta e sai imediatamente, 20×; a ponta segue viva e utilizável) e o cenário `storage` (o reprodutor orgânico). Provável causa também dos "canais vazaram 3 → 0" intermitentes vistos hoje.

### Corrigido (isolamento de testes: diretórios exclusivos por teste)
- **Correção de diagnóstico**: as falhas intermitentes do trio `user_fs`/`user_devmgr`/`user_vfs` atribuídas a contenção nos blocos 21–22 eram, na verdade, uma **colisão de estado persistente** introduzida pelos modos 59/60: ambos criavam `/docs` com conteúdo, e o `user_fs` (mais antigo) faz `unlink docs` (falha em diretório não-vazio) + `mkdir docs` (falha `Exists`) → 141 a partir do 2º boot no mesmo disco — exatamente o padrão "1º boot passa, seguintes falham". Os testes novos agora usam diretórios exclusivos (`/fm-teste`, `/portal-teste`); três boots consecutivos verdes no mesmo disco (81/81). Lição registrada: teste que persiste estado usa **prefixo próprio** — e "flake" com padrão determinístico de primeiro-boot-passa é colisão de estado, não contenção.

### Adicionado (Fase 6, bloco 22 — portal de arquivos)
- `services/portal`: **portal de arquivos** — o app pede um arquivo (`"escolhe"` no seu canal com o portal); o portal, que é quem tem o `nexo.fs` e a janela, mostra a lista (pastas apagadas, não escolhíveis no MVP) e espera o **usuário** clicar; só então lê o arquivo e devolve ao app **apenas o conteúdo** (≤ 3900 B). O app nunca vê o sistema de arquivos nem os nomes dos outros arquivos: a escolha do usuário é o limite da concessão — o desenho dos portais de desktop, com capacidades de verdade.
- Auto-teste `user_portal` (81º) + modo 60 do `utest`: o driver faz os dois papéis — como app, pede e recebe exatamente `portal-conteudo`; como usuário, espera a lista aparecer (glifo por pixel) e clica. O fs fica no portal, estruturalmente.

### Adicionado (Fase 6, bloco 21 — gerenciador de arquivos)
- `services/arquivos`: **gerenciador de arquivos** (MVP de navegação) — lista um diretório do `nexo.fs` (entrada por linha, pastas em acento, arquivos em branco); clique numa pasta **entra nela** (re-lista e repinta, emite `"pasta <dir>"`); clique num arquivo emite `"abrir <caminho>"` ao orquestrador — **o gerenciador aponta; quem abre é quem tem as capacidades**.
- Auto-teste `user_arquivos` (80º) + modo 59 do `utest`: prepara `/docs` (arquivo + subpasta), lista por conta própria para conhecer a ordem, e navega clicando: entra em `sub` (mensagem + listagem nova conferida por glifos) e "abre" `c.txt` (mensagem exata). Idempotente entre boots.

### Adicionado (Fase 6, bloco 20 — editor de texto + nexo-textgrid)
- `nexo-textgrid`: a grade de texto do terminal virou biblioteca compartilhada (quebra, rolagem, backspace, mapa evdev→ASCII) com testes de host; **`\n` agora inclui o retorno de carro** (modo *newline* — idempotente para o `\r\n` que o shell emite, e o que um editor espera de Enter). `services/term` refatorado para usá-la (o `user_term` prova a equivalência).
- `services/editor`: **editor de texto** (MVP de notas) — abre um arquivo do `nexo.fs`, mostra o texto na grade com cursor de acento, edita no fim do texto e **F2 salva** (truncate + write reais). Em "fecha", **devolve o canal do fs** ao orquestrador antes de sair: capacidades emprestadas voltam.
- Auto-teste `user_editor` (79º) + modo 58 do `utest`: escreve `/nota.txt`, digita "mundoq" + backspace pela entrada sintética do compositor, confere os glifos na saída composta, salva com F2 e re-lê o arquivo **de fora** com o canal devolvido: `ola\nmundo`.

### Documentação (Fase 6, bloco 19 — SDK atualizado com a plataforma dos blocos 10–18)
- `docs/sdk.md`: tabela de crates ganhou `nexo-inst`/`nexo-img`/`nexo-cal`; o contrato de apps documenta o consentimento do lançador, a introspecção (`debug_info`, incl. o relógio de parede), o padrão de arquivos (`"abre <caminho>"` + canal `nexo.fs`; formato `v<N>` do `.cur`); exemplos apontam os sete apps reais do repositório; a seção de depuração herda as três lições pagas pelos testes (convergência em memória compartilhada, fim de vida por mensagem e não por pixel, idempotência com disco persistente).

### Adicionado (Fase 6, bloco 18 — consentimento no lançador)
- `services/lanc`: **lançador com consentimento** — antes de executar um app instalado, lê o manifesto no disco e mostra a janela de consentimento (uma célula de acento por permissão declarada + botões **Permitir**/**Negar**); só concede as capacidades depois do clique em Permitir, e concede **exatamente o que o manifesto declara** ("janelas" → sessão nova do compositor via `open`, entregue pelo cordão de vida do app). **Negar significa que o app nem é executado.** O clique é entregue pelo compositor à janela sob o cursor: a decisão vem do usuário, não do app.
- Auto-teste `user_consent` (78º) + modo 57 do `utest`: instala dois apps reais (binário da calculadora, `perms=janelas`), decide os dois lados **clicando de verdade** (entrada sintética no compositor): Permitir → a janela "calc" aparece (`surface_info`), o app encerra limpo pelo cordão de vida; Negar → "negado" chega **depois** da decisão de não lançar e nenhuma janela do app existe. Detalhe de campo: o ponteiro `.cur` guarda `v<N>` — o lançador lê o mesmo formato que o instalador escreve.

### Adicionado (Fase 6, bloco 17 — relógio de parede e calendário)
- Kernel: **RTC CMOS** lido uma vez no boot (`arch/x86_64/src/rtc.rs`: guarda de atualização-em-andamento, leitura dupla até estabilizar, BCD/12h conforme o registrador B; século fixo 20xx) ancora o relógio de parede: `debug_info` seletor 7 = **segundos Unix (UTC)** agora (0 = RTC ilegível). No QEMU o RTC é UTC por padrão; conferido contra o relógio do host (Δ = tempo de boot).
- `nexo-cal`: datas civis `no_std`/sem alocação (algoritmos de Hinnant): epoch ↔ (ano, mês, dia), dia da semana, bissextos, dias no mês — testes de host com datas conhecidas (inclusive além de 2038) e **round-trip dia a dia por 400 anos**.
- `services/agenda`: **calendário** — o mês corrente numa grade 7×6 (segunda primeiro), hoje em acento, data do relógio de parede real. Auto-teste `user_agenda` (77º) + modo 56 do `utest`: o driver computa a mesma grade com a `nexo-cal` e confere na saída composta hoje (acento), o dia 1 (cinza) e o vazio depois do fim do mês.

### Adicionado (Fase 6, bloco 16 — entrada mesclada: teclado + tablet num canal)
- Fase 4 do cenário `input`: dois `virtio-input` reais no mesmo boot (teclado e tablet), um `inputdev` para cada, ambos empurrando no **mesmo canal** de entrada do compositor — o assinante duplica a ponta de escrita (`RIGHT_DUPLICATE`) e entrega uma cópia a cada driver no `subscribe`; lotes evdev são atômicos por `send`, então a mescla é limpa e sem cabeçalhos extras. A tecla e o clique QMP chegam mesclados à mesma janela (modo 55 do `utest`, `input-test=4`). Fecha por inteiro "integrar mouse e teclado pelo serviço de entrada".

### Adicionado (Fase 6, bloco 15 — ponteiro real: virtio-tablet)
- `nexo.input` v1.2 (aditivo): `subscribe{chan, abs_w, abs_h}` — com dimensões > 0 o `inputdev` **normaliza** os eventos `EV_ABS` X/Y do `absinfo` do dispositivo (tablet: 0..32767, lido da config do virtio-input — select/subsel/size/payload) para `0..dim-1`, tipicamente o tamanho da saída do compositor, que espera ABS em pixels. Com 0, tudo passa cru (fontes sintéticas já mandam pixels — nenhum teste existente mudou de contrato).
- `run-qemu --input-tablet` (`virtio-tablet-pci`); cenário `input` ganhou a **fase 3**: boot com tablet real, clique absoluto injetado por QMP (`input-send-event` abs+btn), cadeia `inputdev → wm` (modo 54 do `utest`, variante 3 do `input-test`) e o clique chega como evento `pointer` em coordenadas locais da janela sob o cursor (tolerância ±1 px pelo arredondamento do eixo Y). O helper QMP tolera o QEMU sair no meio do clique — o clique válido encerra o teste.
- Vários dispositivos (teclado + tablet) podem alimentar o mesmo canal de entrada por **duplicação de handle** — cada lote evdev é atômico por `send`; a mescla fica para o boot de produção do shell.

### Adicionado (Fase 6, bloco 14 — coleta de versões antigas no instalador)
- `nexo-inst`: **GC de versões** — cada instalação grava um `files.txt` (nomes dos arquivos da versão) e, **após** o commit do ponteiro, `gc()` remove por inteiro toda `vN` com `N ≤ corrente − KEEP_VERSIONS` (=2: corrente + anterior para rollback): arquivos listados, `files.txt`, `manifest.txt` e o diretório. Best-effort: falha na coleta nunca desfaz a instalação (a próxima tenta de novo); versões gravadas antes do `files.txt` existir são toleradas e não são tocadas. `AppFs` ganhou `unlink` (o NexoFS remove arquivo ou diretório vazio).
- Motivação de campo: o ENOSPC determinístico do bloco 13 (disco de testes cheio de `v1..vN`). Testes: host (janela {corrente, anterior} exata; power-cut inalterado — a coleta é pós-commit) e `user_install` confere no NexoFS real que nenhuma versão coletável sobra, inclusive com disco legado.

### Infraestrutura (CI: retry quando o QEMU do runner morre por sinal)
- `tools/test-qemu`: quando o QEMU morre por um **sinal do host** (245=SIGSEGV, 246=SIGBUS, 250=SIGABRT — visto duas vezes no runner do GitHub em 2026-09-01, em cenários distintos, com o guest saudável), o cenário é reexecutado (até 3 tentativas). Cenários que matam o QEMU de propósito (`kill_on`) não fazem retry. Escritas parciais no disco da tentativa abortada equivalem a um corte de energia — que o sistema precisa aguentar de qualquer forma (e o `powercut` prova).

### Adicionado (Fase 6, bloco 13 — visualizador de imagens)
- `nexo-img`: decodificador de imagens `no_std` e sem alocação — primeiro formato **PPM P6** (cabeçalho texto + trios RGB), com comentários, limites de dimensão e validação hostil: nenhum prefixo ou mutação pode causar pânico (testes de host incl. fuzz-lite de truncamentos).
- `services/visor`: **visualizador de imagens** — recebe do orquestrador a sessão do compositor e um canal `nexo.fs` com "abre <caminho>", lê o arquivo em blocos, decodifica e apresenta numa janela do tamanho exato da imagem.
- Auto-teste `user_visor` (76º) + modo 53 do `utest`: escreve um PPM 16×12 de quadrantes coloridos no NexoFS real (idempotente entre boots), transfere o canal do fs ao visor pelo pipe e confere os quatro quadrantes na saída composta.

### Notas (disco de dados cheio ≠ corrupção)
- Diagnóstico de campo: dezenas de execuções da suíte encheram o `build/data.img` local (16 MiB) — as instalações versionadas acumulam `v1..vN` sem coleta — e os testes de armazenamento passaram a falhar de forma **determinística** (ENOSPC gracioso, sem corrupção; disco fresco = suíte verde). Próximo passo natural: coleta de versões antigas no `nexo-inst`.

### Adicionado (Fase 6, bloco 12 — terminal gráfico)
- `services/term`: **terminal gráfico** — uma janela que *serve* o protocolo `nexo.console` v1.0, e com isso o shell de diagnóstico existente roda dentro dela **sem mudar uma linha**: escritas do shell viram texto numa grade de glifos 8×8 (`nexo-font`) com quebra automática, `\r`/`\n`/backspace e rolagem (linha nova nasce limpa); as teclas que o compositor entrega à janela em foco viram a leitura da console — a mediação do compositor vale para o shell também.
- Auto-teste `user_term` (75º) + modo 52 do `utest`: injeta `eco ola` e `sair` tecla a tecla pelo canal de entrada do compositor e confere os glifos do shell na saída composta usando um espelho da grade alimentado com o fluxo determinístico que o shell emite (fonte real `nexo-font` diz qual pixel acende). Encerramento por handshake: `sair` → shell despede-se e sai 0 → term detecta o console fechado, avisa `"fim"` ao orquestrador e sai 0 (pixels rolam durante ecos e não servem de sinal de encerramento — lição registrada no teste).

### Adicionado (Fase 6, bloco 11 — monitor de sistema)
- `services/monitor`: **monitor de sistema** — janela que lê a saúde do kernel via `debug_info` (CPUs online, uptime, processos vivos, memória física) e pinta uma célula verde/vermelha por estatística mais um *heartbeat* que alterna branco/magenta a cada releitura (~100 ms), provando de fora que o monitor está vivo.
- `debug_info`: seletores novos (aditivos) — 5 quadros físicos livres, 6 quadros físicos utilizáveis (specs atualizadas).
- Auto-teste `user_monitor` (74º) + modo 51 do `utest`: espera as quatro células verdes na saída composta e vê o heartbeat trocar de cor duas vezes.

### Adicionado (Fase 6, bloco 10 — mecanismo de revogação)
- `nexo-inst`: **revogação** — a lista `/apps/.revoked` (um nome por linha) é consultada pelo `install`, que recusa apps revogados (`InstError::Revoked`); `is_revoked()` serve aos lançadores (não executar o que foi revogado) e `revoke()` alimenta a lista (idempotente). Lista ilegível = **falha fechada** (nega tudo). Testes de host (revogado não instala; outros seguem livres) e o `user_install` ganhou a etapa no NexoFS real, idempotente entre boots (na 2ª execução o app já está revogado e o teste confere a recusa direto).

### Alterado (Fase 6, bloco 7 — ABI nativa v1 experimental)
- **`ABI_VERSION` 0 → 1**: a ABI de syscalls foi declarada **v1 experimental** — o conjunto atual (33 syscalls 0–32, handles com direitos que só diminuem, canais NXIP, memória compartilhada, os dois spawns) passa a evoluir por **política aditiva**: syscalls novas ganham números novos, structs só crescem por campos com padrão zero no fim, protocolos IPC seguem o ipc-compat §3; qualquer quebra sobe a versão e é registrada aqui. `docs/spec/syscall-abi.md` atualizada com a política. Programas consultam por `abi_version` (os testes comparam contra a constante e seguem verdes).

### Adicionado (Fase 2, extra — memória compartilhada entre processos)
- `MemoryObject` (`kind` 4) e syscalls `memory_create` (28, aloca N páginas zeradas ≤ 256) / `memory_map` (29, mapeia `USER|RW` cacheável na região de dispositivos). Transferir o handle por canal compartilha as **mesmas páginas físicas** entre processos; o objeto possui os frames e os libera quando ninguém mais o referencia (mapeamento sem posse, como MMIO). `sdk/nexo-sys` ganhou os wrappers.
- Teste `user_shmem`: um produtor cria memória, escreve um marcador e transfere o handle a um consumidor por canal; o consumidor lê o marcador e responde na mesma memória; sem vazamento de quadros. 42 testes no boot. Base para o compositor (buffers de janela) e payloads grandes de IPC (ipc-compat §2.6).

### Adicionado (Fase 1)
- `nexo-acpi`: parser de RSDP/XSDT/RSDT/MADT/HPET sem alocação (testes de host).
- LAPIC (xAPIC) com timer calibrado pelo PIT; I/O APIC com overrides ISA (teste roteia o PIT pelo GSI 2); PIC remapeado e mascarado; vetores de IPI (resched, halt, TLB flush) e espúria.
- TSC calibrado como relógio monotônico (`monotonic_ns`, `uptime`, `sleep`, `delay_us`); ticks de 1000 Hz por CPU só para o escalonador.
- SMP: trampolim INIT/SIPI (modo real → longo), dados por CPU via `gs:[0]` (GDT/TSS/#DF próprias), pilhas das APs com guard page, 4/4 CPUs online no QEMU; panic/exceção fatal param as outras CPUs; shootdown de TLB por IPI.
- Escalonador preemptivo de threads de kernel: fila global, quantum de 10 ms, idle por CPU, `spawn/yield/sleep/join/exit/reap`, pilhas em slots com guard page, preempção dentro do handler do timer, IPI para CPU ociosa.
- `IrqLock` (spinlock com interrupções desabilitadas) e regra de locks do kernel.
- Modo de stress `stress=<s>` (lock, atomics, heap, sleep, spawn/join, map/unmap, todas as CPUs) com invariantes; cenário `stress` no CI (15 s) e `make stress DURATION=86400` para o gate F1.
- Timers de kernel (`timer.rs`): callbacks únicos e periódicos por prazo em ns, despachados pela thread `ktimer` fora de contexto de interrupção; cancelamento.
- Afinidade de CPU por thread (`spawn_on`, `spawn_with_affinity`, `set_affinity`); a thread `main` fica presa à BSP.
- 27 auto-testes no boot (novos: acpi, apic_timer, tsc_clock, ioapic, ipi_self, smp, threads_*, timers, threads_affinity); `run-qemu --smp` (padrão 4).

### Alterado
- Tarefas cooperativas substituídas por threads preemptivas (`sched.rs`); `time::sleep_ms` usa o TSC.
- Lint inclui `clippy --target x86_64-unknown-none` para o código `cfg(x86_64)`.

## [0.0.1-boot] — 2026-08-29

Primeira release: fundação reproduzível, observável e testada (Fase 0 + Plano dos 90 dias).

### Adicionado
- Loader UEFI `nexo-loader` (x86_64-unknown-uefi): leitura de `kernel.elf`/`boot.cfg`, GOP, RSDP, physmap de 2 MiB (NX), carga ELF com W^X, pilha com guard page, cópia do ELF para símbolos, `ExitBootServices`, conversão do mapa de memória, `BootInfo` v1.
- Kernel `nexo-kernel` (x86_64-unknown-none, Rust estável): logger serial estruturado, GDT/TSS (IST1 para #DF), IDT com 256 stubs em assembly, handlers de #BP/#PF/#DF/IRQ, sondas de falta esperada, panic com backtrace por frame pointers e symbolication (demangler legado e v0), normalização do mapa de memória, alocador de quadros por bitmap, paginação (map/unmap/update/translate), heap com crescimento sob demanda e guard pages, PIT 1000 Hz, tarefas cooperativas (spawn/yield/exit/reap), console de framebuffer com fonte 8×8 própria.
- 15 auto-testes no boot com protocolo serial `[TEST]/[RESULT]`; cenários `test=panic|fault|overflow`.
- Crates de host com testes: `nexo-boot-abi`, `nexo-mm`, `nexo-heap`, `nexo-sync`, `nexo-symbols`, `nexo-font`, `nexo-arch-x86_64`.
- Ferramentas: `tools/build-image` (GPT + ESP FAT32 determinística), `tools/run-qemu`, `tools/test-qemu`, `tools/symbolize`, `tools/check-toolchain`; `Makefile`; CI GitHub Actions.
- Documentação: charter, arquitetura, segurança/threat model v0, ADR-0001..0014, spec da ABI de boot, toolchain, testes, inventário de `unsafe`, compatibilidade, release, recuperação, hardware suportado.

### Limitações conhecidas
- Apenas QEMU q35/UEFI; sem SMP, APIC, modo usuário, armazenamento ou rede.
- PIC/PIT legados; timer com resolução de 1 ms; sem relógio TSC calibrado.
- Console de framebuffer sem rolagem (volta ao topo).
- Regiões de memória "loader-reclaim" incluem código/dados de boot services que o kernel já pode reutilizar; runtime services não são chamados.

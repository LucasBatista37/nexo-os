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

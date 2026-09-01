# Checklist consolidada do projeto — estado e caminho até a 1.0

Gerado por `tools/roadmap-status` a partir de `PLANO_MESTRE_SISTEMA_OPERACIONAL.md` em 2026-08-31 (commit `498918c`). Legenda: ✅ concluído · 🟡 parcial · ⬜ pendente · ⛔ bloqueado. Percentual = (concluídos + ½ parciais) / total.

**Total de itens do plano:** 535 — ✅ 161 · 🟡 64 · ⬜ 310 · ⛔ 0 → **36% do caminho até a 1.0** (ponderado por item, não por esforço: as fases restantes são muito maiores).

## 1. Visão por fase

| Fase | Gate | ✅ | 🟡 | ⬜ | % | Esforço restante estimado | O que falta (resumo) |
|---|---|---|---|---|---|---|---|
| Fase 0 — Fundação e preparação (meses 0–6) | ✅ atendido | 13 | 2 | 0 | 93% | 0 (concluída) | gate F0 atendido em clone limpo; release 0.0.1-boot publicada |
| Fase 1 — Kernel mínimo confiável (meses 6–18) | 🟡 quase | 17 | 2 | 0 | 95% | 1–3 semanas | só falta executar o stress de 24 h e cortar 0.1-kernel; melhorias: fila por CPU, shootdown com confirmação |
| Fase 2 — Modo usuário, IPC e capabilities (ano 2) | 🟡 quase | 10 | 7 | 1 | 75% | 2–4 meses | IDL + gerador de código, fuzzing sistemático, shell de diagnóstico, jobs/domínios, múltiplas threads por processo, memória compartilhada, eventos/espera múltipla, release 0.2-userspace |
| Fase 3 — Dispositivos virtuais e armazenamento (anos 2–3) | 🟡 critérios atendidos; release pendente | 17 | 3 | 1 | 88% | 6–12 meses | PCI/ACPI, VirtIO (block/input/rng/console), drivers isolados com DMA/MMIO por capability, cache de blocos, VFS, ramfs, FAT, escrita persistente, testes de corte de energia |
| Fase 4 — Rede e serviços básicos (anos 3–4) | ⬜ não iniciado | 2 | 14 | 3 | 47% | 6–12 meses | VirtIO net, Ethernet/ARP/IPv4/ICMP/UDP/TCP/DHCP/DNS, sockets, IPv6, firewall, TLS portado, HTTP, fuzzing de rede |
| Fase 5 — Gráficos, entrada e shell próprio (anos 3–5) | ⬜ não iniciado | 9 | 14 | 2 | 64% | 12–18 meses | renderer 2D, compositor, entrada, janelas, toolkit, temas, login/sessão, Contextos, Central de Ações, Faixa de Atividades, acessibilidade, testes de usabilidade |
| Fase 6 — Plataforma de aplicativos e desktop essencial (anos 4–6) | ⬜ não iniciado | 4 | 7 | 14 | 30% | 12–18 meses | ABI v1, SDK Rust/C, pacotes assinados, permissões/portais, apps essenciais, terminal, gerenciador de arquivos, editor, configurações, motor web portado |
| Fase 7 — Áudio, mídia, USB e hardware real (anos 5–7) | ⬜ não iniciado | 0 | 0 | 20 | 0% | 12–24 meses | USB/HID/armazenamento, NVMe/AHCI, áudio, Ethernet real, Wi-Fi, GPU/display, energia/suspensão, laboratório de hardware |
| Fase 8 — Segurança, instalação, atualização e recuperação (anos 5–8) | ⬜ não iniciado | 0 | 0 | 22 | 0% | 9–15 meses | criptografia de disco, TPM, trust root, A/B, rollback, recovery, instalador, Secure Boot, backup, crash dumps, fuzzing contínuo, revisão externa |
| Fase 9 — Beta público controlado (anos 7–10) | ⬜ não iniciado | 0 | 0 | 20 | 0% | 12–24 meses | hardware certificado, canais de release, servidores de símbolos/bugs, i18n, acessibilidade AA, documentação, 20→100 testadores, ABI candidata |
| Fase 10 — Versão 1.0 e expansão (anos 8–12+) | ⬜ não iniciado | 0 | 0 | 18 | 0% | contínuo (12+ meses até 1.0) | contrato de ABI 1.x, auditoria, certificação de modelos, repositório stable, SBOM, governança, 1.0; depois aarch64, GPU, VM Linux |

Estimativas assumem uma pessoa com 15–25 h/semana (Plano §13). Somando as faixas, o caminho até a 1.0 fica em **~6 a 10 anos** de trabalho de uma pessoa; com uma equipe pequena (Plano §13.1) cai para 3–5 anos. As fases 0–2 avançaram em dias, não meses, por terem sido feitas com apoio de IA em sessões intensivas — o ritmo real das fases seguintes depende de hardware, drivers e testes com pessoas, que não se comprimem da mesma forma.

## 2. Gates (Plano §5 e §7)

| Gate | Critério (resumo) | Estado | Evidência |
|---|---|---|---|
| Fase 0 | clone limpo → um comando gera imagem que inicia em QEMU; CI comprova a mensagem do kernel | ✅ atendido | clone limpo + `make ci` verde (2026-08-29); `docs/releases/0.0.1-boot.md` |
| Fase 1 | 24 h de stress em QEMU, múltiplas CPUs, memória isolada, exceções tratadas, zero falha inexplicada | 🟡 quase | 2 h de stress com 4 CPUs sem erros (2026-08-30: 17 M trocas, contador com lock exato, 1,27 M processos); as 24 h estão em execução (`make stress DURATION=86400`) |
| Fase 2 | 3 processos isolados simultâneos; servidor reinicia sem reiniciar o kernel; acesso sem capability falha de forma testada | 🟡 quase | 4 processos isolados simultâneos e serviço reiniciado sem reiniciar o kernel (`user_services`); negação por direitos testada; falta fuzzing sistemático e protocolo tipado |
| Fase 3 | arquivos sobre VirtIO block persistem; driver de armazenamento pode falhar sem corromper o kernel; cortes de energia simulados | 🟡 critérios atendidos; release pendente | arquivos criados/alterados/removidos sobre VirtIO-block persistem entre boots (NexoFS v0, `test-qemu --scenario storage`, 2026-08-30); driver de bloco morto sem afetar o kernel (`user_block_crash`); cortes de energia simulados no host em cada escrita — faltam VFS/namespace, cache de blocos, FAT do ESP, IOMMU, VirtIO input/rng/console e cortes no QEMU real |
| Fase 4 | DHCP, DNS, TLS; tráfego malformado não derruba outros serviços | ⬜ não iniciado |  |
| Fase 5 | login, dois apps, janelas/Contextos, teclado e mouse, sessão reiniciada sem reiniciar o kernel | ⬜ não iniciado |  |
| Fase 6 | desenvolvedor novo cria, empacota, instala, depura e atualiza um app só pela documentação | ⬜ não iniciado |  |
| Fase 7 | instalar em um PC de referência com disco, entrada, tela, rede, áudio, suspensão e regressão repetível | ⬜ não iniciado |  |
| Fase 8 | falha de energia na atualização, slot corrompido, pacote malicioso e chave comprometida detectados/recuperados | ⬜ não iniciado |  |
| Fase 9 | instalação/atualização acima da meta, zero corrupção, falhas críticas abaixo do limite, uso diário comprovado | ⬜ não iniciado |  |
| Fase 10 | promessas públicas testáveis, hardware explícito, atualização e recuperação confiáveis, sem bloqueador crítico | ⬜ não iniciado |  |

## 3. Itens por fase

### Fase 0 — Fundação e preparação (meses 0–6) — 93% (13 ✅, 2 🟡, 0 ⬜)

Gate: ✅ atendido. clone limpo + `make ci` verde (2026-08-29); `docs/releases/0.0.1-boot.md`

| | Item | Evidência / nota |
|---|---|---|
| ✅ | definir nome provisório, visão e público da versão 1.0 | Nexo OS (provisório), PROJECT_CHARTER.md |
| 🟡 | definir computador de desenvolvimento e computador de referência | host macOS definido; PC de referência a escolher antes da Fase 7 |
| ✅ | escolher licença do projeto | MIT OR Apache-2.0, ADR-0012 |
| ✅ | criar repositório, branches protegidas e convenções de commit | github.com/LucasBatista37/nexo-os, `main` protegida, convenções em CONTRIBUTING.md |
| ✅ | configurar Rust bare-metal, linker, Assembly e QEMU |  |
| ✅ | fixar versões do toolchain e registrar atualização controlada | rust-toolchain.toml, docs/toolchain.md |
| ✅ | configurar UEFI para QEMU | edk2 via tools/run-qemu |
| ✅ | gerar imagem de disco reproduzível | make reproducible |
| ✅ | iniciar logs por porta serial |  |
| ✅ | criar CI que compila e inicia a imagem em QEMU | .github/workflows/ci.yml, verde no GitHub Actions |
| ✅ | criar teste que reconhece sucesso/falha pelo serial | tools/test-qemu |
| ✅ | criar template de ADR e RFC |  |
| ✅ | criar threat model v0 | SECURITY.md |
| 🟡 | concluir currículo básico de arquitetura de computadores | guia em docs/study/; estudo pessoal contínuo |
| ✅ | publicar release `0.0.1-boot` | tag v0.0.1-boot publicada no GitHub; notas em docs/releases/ |

### Fase 1 — Kernel mínimo confiável (meses 6–18) — 95% (17 ✅, 2 🟡, 0 ⬜)

Gate: 🟡 quase. 2 h de stress com 4 CPUs sem erros (2026-08-30: 17 M trocas, contador com lock exato, 1,27 M processos); as 24 h estão em execução (`make stress DURATION=86400`)

| | Item | Evidência / nota |
|---|---|---|
| ✅ | carregar mapa de memória fornecido pelo firmware |  |
| ✅ | abandonar corretamente os boot services UEFI |  |
| ✅ | inicializar GDT/TSS e estruturas x86_64 necessárias | GDT/TSS por CPU, IST para #DF, GS por CPU |
| ✅ | implementar IDT e handlers de exceção |  |
| ✅ | emitir panic com contexto e backtrace quando possível | backtrace simbolizado; para as outras CPUs por IPI |
| ✅ | implementar alocador de páginas físicas |  |
| 🟡 | implementar tabelas de páginas e espaços de endereçamento | tabelas e mapper prontos; só o espaço do kernel (espaços de usuário na Fase 2) |
| ✅ | implementar heap do kernel |  |
| ✅ | proteger regiões como read-only, NX e guard pages |  |
| ✅ | inicializar APIC, timer e interrupções externas | LAPIC + timer calibrado, I/O APIC com override ISA testado, PIC mascarado |
| ✅ | descobrir CPUs e iniciar multiprocessamento SMP | ACPI MADT + INIT/SIPI; 4/4 CPUs online no QEMU |
| ✅ | criar threads do kernel e troca de contexto |  |
| ✅ | criar escalonador simples preemptivo | round-robin com quantum de 10 ms em todas as CPUs |
| ✅ | implementar relógio monotônico e timers | TSC calibrado (ns) + timers de kernel únicos/periódicos com thread `ktimer` |
| ✅ | adicionar locks, atomics e primitivas de sincronização | SpinLock, IrqLock, Once; regra: locks sempre com IRQs desabilitadas |
| ✅ | criar testes de concorrência e stress em QEMU | `make stress DURATION=…` e cenário `stress` no CI (15 s) |
| ✅ | limitar e registrar todo uso de `unsafe` | docs/unsafe-inventory.md |
| ✅ | adicionar symbolication e dump mínimo de falhas |  |
| 🟡 | publicar release `0.1-kernel` | gate F1 exige 24 h de stress; ferramenta pronta, execução pendente |

### Fase 2 — Modo usuário, IPC e capabilities (ano 2) — 75% (10 ✅, 7 🟡, 1 ⬜)

Gate: 🟡 quase. 4 processos isolados simultâneos e serviço reiniciado sem reiniciar o kernel (`user_services`); negação por direitos testada; falta fuzzing sistemático e protocolo tipado

| | Item | Evidência / nota |
|---|---|---|
| ✅ | entrar em ring 3 e retornar por syscall | GDT de usuário, `syscall`/`sysret`, `swapgs`; `services/init` roda em ring 3 |
| ✅ | definir ABI de syscall e convenções de erro | ABI v0 em `abi/syscall` + `docs/spec/syscall-abi.md` |
| 🟡 | implementar processos, threads de usuário e jobs/domínios | processos como objetos (spawn/wait/info), uma thread cada; múltiplas threads de usuário e jobs/domínios pendentes |
| ✅ | implementar handles e tabela por processo | `kernel/src/ipc.rs`, syscalls 8–13 |
| 🟡 | implementar direitos: ler, escrever, sinalizar, mapear, transferir e administrar | ler/escrever/transferir/duplicar aplicados; sinalizar/mapear/administrar definidos, sem objetos que os usem |
| ✅ | implementar canais IPC e transferência de handles | canais com filas por extremidade, bloqueio em recv, transferência testada entre processos |
| 🟡 | implementar espera múltipla, eventos e timers | espera múltipla de canais (`channel_wait_any`), canal de interrupções (`irq_channel`) e eventos tipados na IDL (`nexo.net` v1.1: driver empurra quadros); objetos de evento genéricos e timers de usuário pendentes |
| 🟡 | validar cópias entre usuário e kernel | ponteiros validados por faixa e bit USER antes de copiar (`copy_from_user`); cópia para o usuário ainda não existe |
| ✅ | criar formato de protocolo tipado e gerador de código | IDL própria (`idl/*.idl`) + `tools/idlgen` → `abi/proto` (cabeçalho NXIP, ipc-compat §2, testes de compatibilidade e fuzz-lite); `nexo.rng` migrado, demais protocolos na fila |
| ✅ | definir regras de compatibilidade do IPC | docs/spec/ipc-compat.md |
| ✅ | criar `init` e `service-manager` | `services/init` e `services/svcmgr` em ring 3 |
| 🟡 | criar políticas de reinício e dependências | reinício com limite implementado e testado (`echo` cai e volta); dependências declarativas pendentes |
| ✅ | criar loader ELF de usuário | `process::spawn_elf` (W^X, USER, pilha com guarda) |
| 🟡 | criar runtime mínimo Rust e ABI C | `sdk/nexo-sys` + `sdk/nexo-rt` (Rust, sem alocação); ABI C pendente |
| ✅ | criar shell de diagnóstico no espaço de usuário | `services/shell` sobre a console VirtIO e o VFS; cenário `shell` interativo |
| 🟡 | testar isolamento e negação de capabilities | isolamento de memória/instruções e negação por direitos (Denied) testados; fuzzing pendente |
| ✅ | fuzzar decodificador de IPC e syscalls | fuzz-lite determinístico dos parsers e dos decodificadores NXIP no host; fuzz de syscalls aleatório com sementes registradas, agendado toda semana no CI (`make fuzz`, workflow `fuzz`); cobertura guiada (cargo-fuzz) pendente |
| ⬜ | publicar release `0.2-userspace` |  |

### Fase 3 — Dispositivos virtuais e armazenamento (anos 2–3) — 88% (17 ✅, 3 🟡, 1 ⬜)

Gate: 🟡 critérios atendidos; release pendente. arquivos criados/alterados/removidos sobre VirtIO-block persistem entre boots (NexoFS v0, `test-qemu --scenario storage`, 2026-08-30); driver de bloco morto sem afetar o kernel (`user_block_crash`); cortes de energia simulados no host em cada escrita — faltam VFS/namespace, cache de blocos, FAT do ESP, IOMMU, VirtIO input/rng/console e cortes no QEMU real

| | Item | Evidência / nota |
|---|---|---|
| ✅ | implementar enumeração ACPI mínima | RSDP/XSDT/RSDT/MADT/HPET (`nexo-acpi`, Fase 1); tabelas de PCIe (MCFG) e AML ainda não |
| ✅ | implementar enumeração PCI/PCIe | PCI convencional (config `0xCF8/0xCFC`, barramento 0, BARs por sondagem); PCIe/ECAM pendente |
| ✅ | definir protocolo driver–device manager | `devmgr` inicia cada driver com `[concessão restrita, canal]` (ADR-0015); protocolo tipado vem com a IDL |
| 🟡 | implementar binding por IDs e propriedades | binding por IDs (vendor/tipo VirtIO → driver) no `devmgr`; por propriedades (classe, ACPI) pendente |
| ✅ | implementar VirtIO transport | `libraries/virtio` (`nexo-virtio`): capabilities, negociação, MSI-X, fila dividida; usado por `blockdev` e `rngdev` |
| ✅ | implementar VirtIO block | `services/blockdev` em modo usuário; teste `user_block` + cenário `storage` |
| ✅ | implementar VirtIO input | `services/inputdev` (eventos evdev, MSI-X); cenário `input` injeta teclas por QMP e confere os códigos |
| ✅ | implementar VirtIO RNG | `services/rngdev` + `nexo.rng` v0; teste `user_devmgr` |
| ✅ | implementar VirtIO console | `services/consoledev` (porta 0, MSI-X) + `nexo.console` v0; cenário `shell` conversa por socket UNIX |
| ✅ | implementar drivers em processos isolados | driver de bloco em ring 3 com handle de dispositivo; queda do driver não afeta o kernel |
| 🟡 | restringir MMIO, IRQ e DMA por capabilities | concessões por função PCI (`device_open`): config, `pci_enum` e MMIO limitados ao BDF; IRQ/DMA por handle; falta IOMMU para conter DMA |
| 🟡 | criar abstração IOMMU e caminho sem IOMMU explicitamente inseguro | abstração criada (`kernel/src/iommu.rs`: modo `Passthrough` explícito, detecção da tabela `DMAR`, aviso `IRRESTRITO (caminho inseguro)` no boot verificado pelo cenário `boot`; ADR-0015); tradução VT-d/AMD-Vi pendente |
| ✅ | implementar cache de blocos e fila assíncrona | cache de leitura write-through (8 blocos) no `fs`; `blockdev` com até 4 pedidos em voo na virtqueue e respostas em ordem (`channel_try_recv`, syscall 25); teste de pipelining no `utest` |
| ✅ | definir VFS e namespace por sessão/processo | `services/vfs` (protocolo `nexo.fs` roteado; um vfs por cliente com máscara de montagens); teste `user_vfs` com dois namespaces isolados; protocolo tipado e montagem dinâmica pendentes |
| ✅ | implementar `ramfs` | gravável em `/tmp` do vfs (16×16 KiB, volátil, por instância) + initramfs `NEXOIRD1` só leitura |
| ✅ | implementar FAT somente para EFI | `libraries/fat` (FAT12/16/32 só leitura + GPT) e `services/espfs`; teste `user_devmgr` lê `BOOTX64.EFI` e `kernel.elf` da ESP real |
| ✅ | implementar leitura de um filesystem persistente de teste | NexoFS v0 (ADR-0016): `libraries/nexofs` + `services/fs`; `user_fs` e cenário `storage` |
| ✅ | implementar escrita, flush e sincronização | copy-on-write com commit atômico por setor, `sync`; sem cache de blocos ainda |
| ✅ | criar testes de imagem corrompida e corte de energia | host: corte em cada escrita (com escritas rasgadas) e 400 imagens corrompidas; QEMU: cenário `powercut` (SIGKILL durante escritas, boot seguinte monta e `nexo-disk check` confirma) |
| ✅ | criar ferramenta de inspeção do disco no host | `tools/nexo-disk` (info/ls/cat/check), usado pelo cenário `storage` |
| ⬜ | publicar release `0.3-storage` |  |

### Fase 4 — Rede e serviços básicos (anos 3–4) — 47% (2 ✅, 14 🟡, 3 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| ✅ | implementar VirtIO net | `services/netdev` (nexo.net tipado, MSI-X); cenário `net` troca ARP de verdade com o slirp |
| 🟡 | implementar Ethernet e ARP/NDP conforme a fase IPv6 | Ethernet e ARP (request/reply) e agora NDP (Neighbor Solicitation/Advertisement) em `nexo-netstack`, exercitados no cenário `net` |
| 🟡 | implementar IPv4 | cabeçalho + checksum com validação (`nexo-netstack`), usado no ping do cenário `net`; sem roteamento/fragmentação/pilha residente |
| 🟡 | implementar ICMP | echo request/reply completos no cenário `net` (ping real ao slirp); demais tipos pendentes |
| 🟡 | implementar UDP | montagem/leitura em `nexo-netstack` e sockets por porta no `netd` (`udp_send`/`udp_recv` com eco real no cenário `net`); entrega a múltiplos clientes pendente |
| 🟡 | implementar TCP com suíte de testes e estados documentados | máquina de estados em `nexo-netstack::tcp` (ativo e passivo) com janela deslizante de 4 segmentos, ACKs cumulativos e retransmissão go-back-1; suíte de host de 10 casos de estados; `docs/spec/tcp-states.md`; exercitada de verdade no cenário `net`; backlog de escuta e controle de congestionamento pendentes |
| 🟡 | implementar DHCP | cliente completo (DISCOVER→OFFER→REQUEST→ACK com opções) obtém lease real do slirp no cenário `net`; renovação/temporizadores e serviço residente pendentes |
| 🟡 | implementar DNS com cache e validação de entradas | consulta A + parser validado em `nexo-netstack`; `netd` resolve com cache (2ª consulta atendida do cache no cenário `net`); expiração por TTL e serviço para vários clientes pendentes |
| 🟡 | criar API de sockets nativa | `nexo.sock` v1.0 no `netd` (info/resolve com cache/UDP por porta/TCP conectar-enviar-receber-fechar-escutar/`open` p/ múltiplas sessões), multi-cliente (até 8) com transferência de handles na IDL; saída, entrada e HTTP reais no cenário `net`; isolamento por sessão (sockets globais hoje) e eventos assíncronos ao cliente pendentes |
| 🟡 | criar compatibilidade POSIX de sockets | `sdk/nexo-net`: API BSD (`socket`/`connect`/`send`/`recv`/`sendto`/`recvfrom`/`close`/`getaddrinfo`, `sockaddr_in`, `errno`) sobre `nexo.sock`, descritores→handles (ADR-0014); TCP verificado no cenário `net`; `poll`/`select`, `bind`/`accept` e integração com uma libc pendentes |
| 🟡 | implementar IPv6 | `nexo-netstack::ipv6` (endereço link-local, cabeçalho, checksum, ICMPv6 echo, NDP NS/NA); `netd` responde a NS; cenário `net` confirma um NS bem-formado no pcap; SLAAC/roteamento e sockets IPv6 no `netd` pendentes |
| 🟡 | implementar firewall por aplicativo e perfil | `nexo-netstack::firewall` (perfil negar-por-padrão: regras de sub-rede/porta/protocolo + DNS/escuta); `netd` aplica o perfil por sessão (`open{...rule_*}`), negando conexões/datagramas fora dele; cenário `net` comprova permitido/negado; perfis persistentes e por aplicativo instalado pendentes |
| 🟡 | expor permissões de rede por pacote | cada sessão do `netd` tem um perfil que autoriza destino/porta/protocolo por pacote (firewall acima); interface de usuário para conceder/revogar pendente |
| ⬜ | portar uma biblioteca TLS auditada compatível com a licença |  |
| ⬜ | criar armazenamento seguro de certificados |  |
| 🟡 | implementar cliente HTTP para atualizações | GET HTTP/1.0 com validação de status e corpo pela API `nexo.sock` (cenário `net`, servidor real no host); biblioteca de cliente, HTTP/1.1 e integração com atualizações pendentes |
| 🟡 | fuzzar pacotes, parsers e estados de protocolo | parsers de Ethernet/ARP/IPv4/ICMP/UDP/DHCP/DNS/TCP/IPv6 com fuzz-lite determinístico e a **máquina de estados TCP** submetida a sequências aleatórias de segmentos/ações (invariantes verificadas), tudo no fuzzing semanal do CI; cobertura guiada (cargo-fuzz) e fuzz de pacotes na pilha residente pendentes |
| ✅ | criar captura de rede autorizada para diagnóstico | `tools/netcap`/`make netcap` (pcap via `run-qemu --net-dump` + resumo por protocolo/fluxo; sempre local e explícita) |
| ⬜ | publicar release `0.4-network` |  |

### Fase 5 — Gráficos, entrada e shell próprio (anos 3–5) — 64% (9 ✅, 14 🟡, 2 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| 🟡 | obter framebuffer UEFI e modos de vídeo | loader captura o framebuffer via GOP (`FramebufferInfo` no BootInfo v2), kernel desenha o console sobre ele, e o layout é exposto ao userspace pela syscall `fb_info` (31) — o mapeamento é gated pela concessão do dispositivo de vídeo (o framebuffer é um BAR, `mmio_map`); enumeração e troca de modos de vídeo pendentes |
| 🟡 | criar renderer 2D por software | `libraries/gfx` (`nexo-gfx`): superfícies, retângulos, blit e clipping; auto-teste `gfx` no boot; primitivas geométricas ricas pendentes |
| 🟡 | implementar cores, composição alfa, clipping e transformações | cores RGBA, composição alfa src-over e clipping retangular em `nexo-gfx` (testes de host); transformações (escala/rotação) pendentes |
| 🟡 | implementar rasterização de texto e fallback de fontes | `nexo-gfx::text` desenha strings com a fonte bitmap 8×8 (escala, cor, fundo, quebra de linha) e cai no glifo de fallback para caracteres fora da faixa; testes de host e no auto-teste `gfx`; fontes vetoriais/hinting e Unicode amplo pendentes |
| ✅ | definir protocolo de superfícies e buffers | modelo de superfície/janela (retângulo + buffer + z + dano) em `nexo-wm`; IDL `nexo.wm` (`create_surface`/`commit`/`move`/`destroy`/`output`, handles de `MemoryObject` viajando no vetor de handles) e transporte por memória compartilhada implementados no serviço `wm` |
| ✅ | implementar compositor em espaço de usuário | serviço `wm` em ring 3 (`services/wm`): cada cliente escreve pixels num `MemoryObject` compartilhado; o serviço compõe a cena por z-order com `nexo-wm` numa saída também compartilhada. **Multi-cliente**: várias sessões coexistem (a 2ª aberta por `open`, transferindo a ponta de um canal novo), as superfícies pertencem à sessão que as criou e são liberadas ao desconectar. Auto-testes de boot `user_wm` (duas superfícies de um cliente, ordem-Z na saída) e `user_wm_multi` (duas sessões independentes compõem; uma sessão não mexe na superfície da outra), ambos sem vazamento de quadros |
| 🟡 | implementar double/triple buffering e damage tracking | rastreamento de danos (`nexo-wm::Damage`: acumula e coalesce; composição só repinta o dano) pronto e testado, e o compositor **apresenta no framebuffer real** (a saída composta é copiada para a tela a cada recomposição — auto-teste `user_wm_present`); double/triple buffering (trocar buffers em vez de copiar) pendente |
| 🟡 | integrar mouse e teclado pelo serviço de entrada | cadeia REAL de teclado fechada: `nexo.input` v1.1 tem `subscribe{chan}` (o `inputdev` **empurra** eventos evdev crus no canal, guiado por interrupção via `irq_channel`) e a outra ponta vai direto ao `set_input` do `wm` — teclas físicas injetadas por QMP chegam como eventos `key` à janela em foco (fase 2 do cenário `input`; a janela criada sem foco prévio ganha o foco). O ponteiro usa o mesmo canal/formato (e agora o clique é **entregue à janela clicada** como evento `pointer` em coordenadas locais — a base de widgets interativos e da Faixa), mas só foi testado com eventos sintéticos; falta anexar um dispositivo apontador virtio e testá-lo de verdade |
| ✅ | implementar foco, atalhos e captura segura | foco por clique e na criação (a superfície clicada/primeira criada recebe o teclado — `user_wm_input`/`user_wm_keyboard`), **atalho global** Meta+Tab ciclando o foco, interceptado antes da entrega (`user_wm_shortcut`), e **captura segura** (`grab`/`ungrab`: o teclado vai para a superfície capturada ignorando o foco e cliques são engolidos — ninguém rouba o foco durante entrada sensível; solta sozinha se a superfície some — `user_wm_grab`). Mais atalhos são adicionados conforme o shell gráfico precisar |
| ✅ | implementar janelas, redimensionamento, maximização e mosaico | no serviço `wm`: criar/mover/destruir superfícies com posição e z (múltiplas sessões), **restacking** (`raise`/`lower` — `user_wm_restack`), **redimensionamento** (`resize` via `memory_unmap`/`munmap` — `user_wm_resize`), **maximizar/restaurar** (`maximize`/`restore` — `user_wm_maximize`) e **mosaico** (`tile` organiza todas as janelas numa grade sem realocar buffers: o retângulo de exibição foi desacoplado do tamanho do buffer e a composição escala por vizinho mais próximo — `user_wm_tile`). Layouts contínuos/automáticos (re-tile ao criar/fechar janelas) ficam para o shell gráfico |
| ✅ | implementar múltiplos displays emulado | o `wm` compõe **2 displays emulados** (saídas `MemoryObject` independentes, mesmas dimensões): `create_surface.display`/`output{display}` (0 = primário, apresentado no framebuffer real) e `move_to_display{id,display}` troca a janela de tela — auto-teste `user_wm_displays`. Um segundo display físico (outra saída de vídeo real) fica para quando houver hardware/virtio-gpu multi-scanout |
| 🟡 | criar toolkit UI nativo e tokens de design | `libraries/ui` (`nexo-ui`): tokens de design (`Theme`), widgets `Label`/`Button` (com estados e hit-test) e layout `VStack`, desenhados sobre `nexo-gfx`; testes de host e auto-teste `user_wm_ui` (botão renderizado pela pilha app→ui→gfx→compositor). Faltam mais widgets (campos de texto, listas, menus), foco/tab e um sistema de layout completo |
| 🟡 | criar gerenciamento de temas claro/escuro e alto contraste | `nexo-ui::Theme` tem variantes claro, escuro e alto contraste (tokens de cor); os widgets pintam só a partir do tema. Faltam troca de tema em runtime, persistência da preferência e mais tokens (tipografia, espaçamento, raios) |
| 🟡 | criar login, bloqueio e sessão | `services/greeter`: tela de login em tela cheia (pintada com `nexo-ui`) que **captura** a entrada (`grab` — a senha não pode ser roubada por outra janela nem o foco desviado por clique); senha errada mantém o bloqueio, a certa solta a captura, remove a tela e devolve a entrada à sessão (auto-teste `user_greeter`, multi-processo: wm + greeter + driver). Pendem: credencial de verdade (armazenamento seguro — depende do modelo de usuários da Fase 6), re-bloqueio por atalho/inatividade e gestão de estado da sessão |
| 🟡 | prototipar e testar o modelo de Contextos | protótipo no compositor: 4 Contextos como grupos de janelas; só o ativo é composto e recebe cliques/atalhos, `switch_context` troca preservando o estado das ocultas (buffers intactos) e move o foco para a janela de maior z do novo Contexto; a captura (grab) sobrevive à troca (uma tela segura não é contornável trocando de Contexto) — auto-teste `user_wm_context`. Pendem as demais dimensões do modelo (documentos, permissões temporárias e notificações por Contexto), que dependem dos respectivos subsistemas |
| ✅ | implementar Central de Ações | completa no MVP: o compositor registra as 8 notificações mais recentes — **inclusive as suprimidas pelo não-perturbe** — e o shell as lista/limpa (`notification_info`/`notifications_clear`, sessões comuns negadas — `user_wm_center`); e o **painel visual** existe no `shellui`: o clique na zona direita da Faixa abre a Central (borda de acento + um marcador por aviso) e o segundo clique a fecha — auto-teste `user_shellcenter`. Pendem ações nos avisos (abrir/descartar individual) e os controles rápidos no painel (as APIs de DND/redução de movimento/temas já existem) |
| ✅ | implementar Faixa de Atividades | completa: o compositor tem o modelo de privilégio de shell (`surface_info`/`activate`, erro 7 para sessões comuns — `user_wm_shell`) e o app `services/shellui` desenha a **barra visual** no rodapé (célula por janela, tema `nexo-ui`), faz *broker* de sessões `nexo.wm` para os apps e, ao receber o **clique** na célula (evento `pointer`, entregue à janela clicada em coordenadas locais), **ativa** a janela (Contexto + frente + foco) — auto-teste `user_shellui` (wm + shellui + app: clique na barra ativa a janela, conferido pela tecla seguinte). Refinamentos (títulos nas células, atualização automática sem `sync`) ficam para depois |
| 🟡 | criar notificações e controles de atenção | `notify` (qualquer sessão, inclusive em segundo plano) desenha um banner de sobreposição no compositor (topo direito, acima de tudo); `dismiss_notification` o remove; **não-perturbe** (`set_dnd`) descarta avisos e só o dono da entrada o controla (mediação, erro 6) — auto-teste `user_wm_notify`. Pendem fila/central de avisos, ações nos avisos e notificações por Contexto |
| ✅ | implementar clipboard com mediação e histórico opt-in | área de transferência **mediada pelo compositor**: só a sessão dona da **entrada** (janela focada, ou capturada) lê/escreve — apps em segundo plano não farejam nem injetam (erro remoto 6); o conteúdo atravessa sessões pela mediação; **histórico opt-in** (anel de 4, desligado por padrão, `clipboard_enable_history`) — auto-teste `user_wm_clipboard`. Limite de 256 B nesta versão; formatos ricos/tamanhos maiores (via `MemoryObject`) e UI de histórico ficam para o shell |
| ✅ | implementar drag-and-drop por grants | a sessão dona da **entrada** inicia o arrasto (`drag_start`, erro 6 para as demais); ao soltar (BTN_LEFT release), **só a sessão dona da janela sob o ponteiro** recebe os dados (evento `drop`) — ninguém mais pode lê-los; soltar no vazio ou sob captura descarta o payload — auto-teste `user_wm_dnd`. Payloads grandes (via handle de `MemoryObject` no evento) e o feedback visual do arrasto ficam para o shell |
| 🟡 | implementar leitor de tela em arquitetura, ainda que simples | a arquitetura existe: o compositor emite **eventos semânticos** (`a11y_subscribe` + evento `a11y`: foco mudou com o **título** da janela — `set_title` —, aviso publicado, Contexto trocado) num canal que um leitor de tela assina; auto-teste `user_wm_a11y` faz o papel do leitor e confere o fluxo. Pendem a síntese de voz/braille de verdade, mais eventos (texto/valor dos widgets via `nexo-ui`) e um modelo de permissão para tecnologia assistiva |
| 🟡 | implementar navegação completa por teclado | entre janelas: atalho global Meta+Tab cicla o foco (`user_wm_shortcut`); dentro do app: `nexo-ui::Nav` cicla o foco entre widgets (`focus_next`/`focus_prev` para Tab/Shift+Tab, com wrap) e `draw_focus_ring` torna o foco visível (cor de acento) — testes de host. "Completa" pende do shell gráfico (menus, diálogos e todos os fluxos operáveis sem mouse) |
| 🟡 | implementar escala fracionária e redução de movimento | escala fracionária **por janela** (`set_scale{id,num,den}`: o retângulo de exibição vira buffer×num/den e a composição escala — 200%/150% conferidos por pixel) e a preferência **redução de movimento** (`set_reduce_motion` mediado pela entrada; `prefs` de leitura livre para apps desligarem animações) — auto-teste `user_wm_scale`. Pendem a escala global padrão do sistema (todas as janelas + DPI do texto) e filtros melhores que vizinho-mais-próximo |
| ⬜ | testar usabilidade com usuários externos |  |
| ⬜ | publicar release `0.5-desktop` |  |

### Fase 6 — Plataforma de aplicativos e desktop essencial (anos 4–6) — 30% (4 ✅, 7 🟡, 14 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| 🟡 | estabilizar ABI nativa v1 experimental | declarada: `ABI_VERSION = 1` (consultável por `abi_version`), 33 syscalls (0–32) especificadas em `docs/spec/syscall-abi.md` com **política aditiva** (números/campos novos apenas; quebras sobem a versão e vão ao CHANGELOG; protocolos IPC pelo ipc-compat §3). A promoção a estável pende do uso real por terceiros (gate F6) e do marco 0.9-beta |
| 🟡 | publicar SDK Rust | o SDK existe e está documentado (`docs/sdk.md`: crates, contrato de apps, exemplos, empacotamento); "publicar" de verdade (crates versionados fora do repositório, canal de distribuição) pende da estabilização da ABI v1 |
| ⬜ | publicar headers e toolchain C/C++ |  |
| ✅ | criar gerador de projeto | `tools/nexo-new <nome>`: gera um app funcional (janela `nexo-ui` seguindo o contrato de apps: sessão via canal do orquestrador, cordão de vida, eventos `pointer`) já **registrado** no workspace e no initrd, com `manifest.txt` NEXOPKG1; validado gerando e compilando um app do zero |
| 🟡 | criar documentação e exemplos | `docs/sdk.md` (contrato de apps, mediações, depuração) + três exemplos reais no repositório (`calc`, `greeter`, `shellui`) e os drivers de teste (`utest` 43–45); falta um tutorial passo a passo e docs de referência por API |
| ⬜ | criar depurador remoto e integração com GDB/LLDB quando viável |  |
| ⬜ | criar profiler e visualizador de traces |  |
| ✅ | definir formato de pacote e manifesto | formato `NEXOPKG1` v1 (`docs/spec/pkg.md`): manifesto textual auditável (`name`/`version`/`entry`/`perms` — chaves desconhecidas são erro) + arquivos, tudo sob CRC32; biblioteca `libraries/pkg` (`nexo-pkg`, no_std/sem alocação/`forbid(unsafe)`, validação completa no parse, fuzz-lite de truncamentos e mutações nos testes de host) e ferramenta `tools/nexo-pack` (`build`/`inspect`, validada com o binário real da calculadora). Assinatura e instalação transacional vêm por cima, em blocos próprios |
| ⬜ | implementar assinatura e verificação de pacotes |  |
| ✅ | implementar instalação transacional | `libraries/inst` (`nexo-inst`): diretório **versionado** (`/apps/<nome>.v<N>/`) + o ponteiro `/apps/<nome>.cur` gravado **por último** (o commit, no espírito do NexoFS); corte antes do commit deixa a versão anterior corrente e intacta, e a retentativa re-preenche os mesmos caminhos. Testes de host com **falha injetada em cada operação** (invariante conferida em todos os pontos de corte) e integração real no boot: `user_install` instala v1 → v2 sobre o `nexo.fs` (adaptador via protocolo tipado), v1 intacta, pacote corrompido rejeitado sem tocar nada. Pendem GC de versões antigas e o instalador como serviço com fluxo de consentimento (permissões do manifesto) |
| 🟡 | implementar permissões declarativas e consentimento | declaração **e imposição** existem: o manifesto NEXOPKG1 declara (`perms=`) e o **lançador** concede capacidades só pelo declarado — no auto-teste `user_launcher`, o app com `perms=ipc` nasce com o canal e funciona; o mesmo binário instalado **sem** a permissão nasce sem o handle (a capacidade simplesmente não existe para ele). O mapeamento permissão→capacidade já cobre `ipc` (canal) e **`janelas`** (sessão do compositor: a calculadora real, instalada no NexoFS, é lançada e cria a sua janela **só** com a permissão declarada — `user_launch_gui`); pendem `rede`/`arquivos` no lançador e o fluxo de **consentimento do usuário** (UI de prompt na primeira concessão) |
| ⬜ | criar portal de arquivos, câmera, microfone e notificações |  |
| ⬜ | criar repositório de pacotes de desenvolvimento |  |
| 🟡 | criar processo de revisão e revogação | o **mecanismo de revogação** existe: a lista `/apps/.revoked` (um nome por linha; lista corrompida = falha fechada) é consultada pelo `install` (app revogado não instala — `InstError::Revoked`) e exposta aos lançadores (`is_revoked`); `revoke()` alimenta a lista (idempotente) — testes de host + `user_install` no NexoFS real, idempotente entre boots. Pendem o **processo** de revisão em si e a distribuição da lista (dependem do repositório de pacotes) |
| ⬜ | portar toolchain e utilitários POSIX prioritários |  |
| ⬜ | criar terminal e shell |  |
| ⬜ | criar gerenciador de arquivos |  |
| ⬜ | criar editor de texto |  |
| 🟡 | criar configurações | `services/config`: janela com toggles **reais** de movimento reduzido e não-perturbe, acionados por clique — o clique dá o foco, e a posse da entrada é justamente o que as APIs mediadas exigem (a mediação trabalhando a favor do app); efeitos verificados de fora (`prefs` reflete; com DND um aviso não desenha banner) — auto-teste `user_config`. Pendem tema claro/escuro em runtime, escala e mais painéis |
| ✅ | criar monitor de sistema | `services/monitor`: janela que lê o kernel via `debug_info` (CPUs, uptime, processos, quadros livres/utilizáveis — seletores 5/6 novos, aditivos) e pinta uma célula de sanidade por estatística + um heartbeat que alterna a cada releitura; o auto-teste `user_monitor` confere de fora as células verdes e o heartbeat alternando (monitor vivo). 74 testes no boot |
| ⬜ | criar visualizador de imagens e documentos básicos |  |
| 🟡 | criar calculadora, calendário e utilitários | a **calculadora** existe (`services/calc`): o primeiro aplicativo real da plataforma — janela com visor e botões `nexo-ui` acionados pelos eventos `pointer` do compositor; "1 + 2 =" clicado de verdade e o resultado "3" lido pelo **clipboard mediado** (o clique dá o foco, o foco dá o clipboard) — auto-teste `user_calc` (wm + calc + driver). Lição de ciclo de vida: o canal com o orquestrador é o cordão de vida do app (senão app e compositor esperam um pelo outro no encerramento). Dígitos completos, mais operações, calendário e utilitários pendem |
| ⬜ | portar um motor web existente com sandbox, se recursos permitirem |  |
| ⬜ | criar APIs de compartilhamento entre aplicativos |  |
| ⬜ | publicar release `0.6-sdk` |  |

### Fase 7 — Áudio, mídia, USB e hardware real (anos 5–7) — 0% (0 ✅, 0 🟡, 20 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | implementar USB host controller escolhido para o hardware de referência |  |
| ⬜ | implementar HID USB |  |
| ⬜ | implementar armazenamento USB |  |
| ⬜ | criar enumeração e autorização de dispositivos USB |  |
| ⬜ | implementar NVMe ou AHCI conforme o computador de referência |  |
| ⬜ | implementar teclado, touchpad e mouse reais |  |
| ⬜ | implementar relógio, RTC e fusos horários |  |
| ⬜ | implementar áudio no hardware de referência |  |
| ⬜ | criar servidor de áudio, mixagem e controle de volume |  |
| ⬜ | implementar permissão e indicador de microfone |  |
| ⬜ | implementar câmera somente após isolamento e indicador confiável |  |
| ⬜ | implementar Bluetooth em fase posterior e limitada |  |
| ⬜ | implementar Ethernet real |  |
| ⬜ | escolher um único chipset Wi-Fi inicial e documentá-lo |  |
| ⬜ | implementar GPU/display mínimo do hardware de referência ou usar framebuffer compatível |  |
| ⬜ | implementar suspensão, retomada e tampa do notebook |  |
| ⬜ | implementar bateria, temperatura e política térmica |  |
| ⬜ | criar daemon de firmware e política de blobs externos |  |
| ⬜ | criar laboratório com inventário e testes repetíveis |  |
| ⬜ | publicar release `0.7-hardware-alpha` |  |

### Fase 8 — Segurança, instalação, atualização e recuperação (anos 5–8) — 0% (0 ✅, 0 🟡, 22 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | revisar threat model por subsistema |  |
| ⬜ | auditar syscalls e `unsafe` crítico |  |
| ⬜ | implementar usuários, credenciais e bloqueio seguro |  |
| ⬜ | implementar criptografia de disco |  |
| ⬜ | integrar TPM quando disponível |  |
| ⬜ | criar trust root offline e cerimônia de chaves |  |
| ⬜ | implementar pacotes e imagens assinadas |  |
| ⬜ | implementar proteção contra rollback |  |
| ⬜ | implementar layout A/B |  |
| ⬜ | implementar atualização atômica e health check pós-boot |  |
| ⬜ | implementar rollback automático |  |
| ⬜ | criar ambiente de recuperação independente |  |
| ⬜ | criar instalador gráfico e particionamento protegido |  |
| ⬜ | criar instalação em máquina vazia e dual boot documentado |  |
| ⬜ | implementar Secure Boot depois da cadeia assinada interna |  |
| ⬜ | criar backup e restauração de dados de usuário |  |
| ⬜ | criar reset preservando arquivos quando possível |  |
| ⬜ | implementar crash dumps protegidos e consentimento de envio |  |
| ⬜ | montar fuzzing contínuo |  |
| ⬜ | executar revisão independente de segurança |  |
| ⬜ | realizar exercícios de chave comprometida e repositório malicioso |  |
| ⬜ | publicar release `0.8-distributable-alpha` |  |

### Fase 9 — Beta público controlado (anos 7–10) — 0% (0 ✅, 0 🟡, 20 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | definir lista fechada de hardware beta |  |
| ⬜ | criar imagem de instalação assinada |  |
| ⬜ | criar canais nightly, alpha, beta e stable |  |
| ⬜ | implantar servidor de símbolos e bugs |  |
| ⬜ | implantar crash reporting opt-in |  |
| ⬜ | criar triagem de segurança e SLA interno |  |
| ⬜ | manter matriz de regressão por hardware |  |
| ⬜ | executar testes de atualização desde versões anteriores |  |
| ⬜ | testar internacionalização em português e inglês |  |
| ⬜ | concluir acessibilidade AA nas interfaces essenciais |  |
| ⬜ | criar documentação para usuários |  |
| ⬜ | criar documentação para fabricantes e drivers |  |
| ⬜ | criar portal para desenvolvedores |  |
| ⬜ | distribuir SDK versionado |  |
| ⬜ | medir crash-free sessions, boot, RAM e bateria |  |
| ⬜ | formar grupo de 20 testadores |  |
| ⬜ | ampliar para 100 testadores após gates de estabilidade |  |
| ⬜ | corrigir bloqueadores de uso diário |  |
| ⬜ | congelar ABI candidata a 1.0 |  |
| ⬜ | publicar release `0.9-beta` |  |

### Fase 10 — Versão 1.0 e expansão (anos 8–12+) — 0% (0 ✅, 0 🟡, 18 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | publicar contrato de compatibilidade da ABI 1.x |  |
| ⬜ | publicar política de suporte e fim de vida |  |
| ⬜ | concluir auditoria externa prioritária |  |
| ⬜ | garantir atualização e rollback desde a última beta |  |
| ⬜ | garantir instalação limpa e recuperação |  |
| ⬜ | certificar 1 a 3 modelos de computador |  |
| ⬜ | publicar SDK, documentação e exemplos finais |  |
| ⬜ | manter aplicativos essenciais atualizáveis |  |
| ⬜ | publicar repositório stable assinado |  |
| ⬜ | publicar SBOM das imagens e pacotes próprios |  |
| ⬜ | publicar notas de segurança e limitações conhecidas |  |
| ⬜ | estabelecer governança de releases |  |
| ⬜ | publicar `[NOME] OS 1.0` |  |
| ⬜ | iniciar porta `aarch64` somente com abstrações maduras |  |
| ⬜ | expandir drivers por prioridade e dados de usuários |  |
| ⬜ | pesquisar aceleração GPU e compatibilidade Vulkan plena |  |
| ⬜ | pesquisar VM Linux integrada |  |
| ⬜ | fomentar ecossistema de aplicativos e fabricantes |  |

## 4. Frentes permanentes (Plano §6)

| Frente | ✅ | 🟡 | ⬜ | % |
|---|---|---|---|---|
| 6.1 Kernel e baixo nível | 4 | 6 | 4 | 50% |
| 6.2 Drivers | 0 | 1 | 15 | 3% |
| 6.3 Armazenamento | 0 | 0 | 14 | 0% |
| 6.4 Rede | 0 | 0 | 14 | 0% |
| 6.5 Desktop e experiência | 0 | 0 | 16 | 0% |
| 6.6 Aplicativos e SDK | 0 | 0 | 18 | 0% |
| 6.7 Segurança e privacidade | 0 | 3 | 14 | 9% |
| 6.8 Qualidade e confiabilidade | 3 | 2 | 11 | 25% |
| 6.9 Acessibilidade e internacionalização | 0 | 0 | 14 | 0% |
| 6.10 Distribuição e operação | 0 | 1 | 13 | 4% |

### 6.1 Kernel e baixo nível

| | Item | Evidência / nota |
|---|---|---|
| ✅ | especificação da ABI de boot | docs/spec/boot-abi.md |
| 🟡 | memória física e virtual | 0.0.1-boot: bitmap + paginação 4 níveis; falta SMP/afinidade |
| ✅ | SMP e afinidade | 4 CPUs no QEMU; afinidade por máscara (`spawn_on`, `set_affinity`) |
| 🟡 | preempção e prioridades | preempção sim; prioridades não |
| 🟡 | temporizadores de alta resolução | TSC em ns; timers despachados com resolução de 1 ms (tick) |
| ✅ | isolamento usuário/kernel | ring 3 com espaços próprios; faltas de usuário não afetam o kernel |
| 🟡 | syscalls versionadas | v0 instável com `SYS_ABI_VERSION` |
| ✅ | IPC com transferência de capabilities | handles transferidos por canal com direito TRANSFER |
| ⬜ | contabilidade e limites de recursos |  |
| 🟡 | panic, dump e symbolication | panic/backtrace/símbolos prontos; dump completo pendente |
| ⬜ | mitigação de classes de exploração |  |
| ⬜ | benchmarks de contexto, syscall e IPC |  |
| 🟡 | stress de 24h e posteriormente 7 dias | `make stress DURATION=86400` disponível; execução ainda não realizada |
| ⬜ | documentação de todas as invariantes `unsafe` |  |

### 6.2 Drivers

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | modelo e lifecycle de driver |  |
| ⬜ | descoberta e binding |  |
| ⬜ | isolamento por host de driver |  |
| ⬜ | PCI/PCIe |  |
| 🟡 | ACPI | RSDP/XSDT/MADT/HPET; sem AML |
| ⬜ | VirtIO block, net, input, console e RNG |  |
| ⬜ | USB e HID |  |
| ⬜ | NVMe/AHCI |  |
| ⬜ | display/GPU |  |
| ⬜ | áudio |  |
| ⬜ | Ethernet e Wi-Fi limitado |  |
| ⬜ | energia e bateria |  |
| ⬜ | DMA/IOMMU |  |
| ⬜ | hotplug |  |
| ⬜ | assinatura e distribuição de drivers |  |
| ⬜ | suíte de conformidade por classe de dispositivo |  |

### 6.3 Armazenamento

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | VFS e namespaces |  |
| ⬜ | cache e writeback |  |
| ⬜ | permissões e ACLs/capabilities |  |
| ⬜ | arquivos mapeados em memória |  |
| ⬜ | mounts e mídia removível |  |
| ⬜ | filesystem persistente |  |
| ⬜ | ferramenta de verificação e reparo |  |
| ⬜ | snapshots |  |
| ⬜ | criptografia |  |
| ⬜ | quotas |  |
| ⬜ | testes de corrupção |  |
| ⬜ | testes de queda de energia |  |
| ⬜ | migração de formato |  |
| ⬜ | backup e restauração |  |

### 6.4 Rede

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | Ethernet |  |
| ⬜ | IPv4 e IPv6 |  |
| ⬜ | ICMP, UDP e TCP |  |
| ⬜ | DHCP e DNS |  |
| ⬜ | sockets nativos e POSIX |  |
| ⬜ | TLS |  |
| ⬜ | certificados e relógio confiável |  |
| ⬜ | firewall |  |
| ⬜ | permissões por aplicativo |  |
| ⬜ | VPN em fase posterior |  |
| ⬜ | Wi-Fi e gerenciamento de redes |  |
| ⬜ | captive portal |  |
| ⬜ | diagnósticos |  |
| ⬜ | fuzzing contínuo |  |

### 6.5 Desktop e experiência

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | linguagem visual original |  |
| ⬜ | tokens de cor, tipografia, espaçamento e movimento |  |
| ⬜ | compositor |  |
| ⬜ | janelas flutuantes e mosaico |  |
| ⬜ | Contextos persistentes |  |
| ⬜ | Central de Ações |  |
| ⬜ | Faixa de Atividades |  |
| ⬜ | login e bloqueio |  |
| ⬜ | notificações |  |
| ⬜ | multi-monitor |  |
| ⬜ | escala e alta densidade |  |
| ⬜ | clipboard e drag-and-drop seguros |  |
| ⬜ | temas e personalização |  |
| ⬜ | atalhos consistentes |  |
| ⬜ | onboarding e recuperação de erro |  |
| ⬜ | testes de usabilidade desktop e notebook |  |

### 6.6 Aplicativos e SDK

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | ABI C |  |
| ⬜ | SDK Rust |  |
| ⬜ | toolkit UI |  |
| ⬜ | runtime e biblioteca padrão |  |
| ⬜ | CLI de build, run, test, debug e package |  |
| ⬜ | templates e exemplos |  |
| ⬜ | documentação gerada |  |
| ⬜ | pacotes e manifests |  |
| ⬜ | capabilities/portals |  |
| ⬜ | repositório e atualização |  |
| ⬜ | terminal |  |
| ⬜ | arquivos |  |
| ⬜ | configurações |  |
| ⬜ | monitor do sistema |  |
| ⬜ | editor |  |
| ⬜ | visualizadores |  |
| ⬜ | motor web portado |  |
| ⬜ | compatibilidade POSIX progressiva |  |

### 6.7 Segurança e privacidade

| | Item | Evidência / nota |
|---|---|---|
| 🟡 | threat model atualizado | v0 em SECURITY.md |
| ⬜ | privilégio mínimo |  |
| ⬜ | isolamento de drivers e serviços |  |
| 🟡 | W^X, NX, ASLR e guard pages | W^X, NX e guard pages ativos; ASLR pendente |
| ⬜ | IOMMU |  |
| ⬜ | consentimento de câmera/microfone/rede/arquivos |  |
| ⬜ | indicadores de privacidade resistentes a falsificação |  |
| ⬜ | cofre de credenciais |  |
| ⬜ | criptografia de disco |  |
| ⬜ | Secure/Measured Boot |  |
| ⬜ | pacotes e updates assinados |  |
| ⬜ | proteção contra rollback |  |
| ⬜ | rotação e revogação de chaves |  |
| 🟡 | fuzzing e sanitizers onde aplicável | fuzz-lite nos testes de host; sanitizers/cargo-fuzz pendentes |
| ⬜ | SBOM e análise de dependências |  |
| ⬜ | política de vulnerabilidades |  |
| ⬜ | auditoria externa |  |

### 6.8 Qualidade e confiabilidade

| | Item | Evidência / nota |
|---|---|---|
| ✅ | build reproduzível |  |
| ✅ | testes unitários no host |  |
| ✅ | testes kernel/QEMU |  |
| ⬜ | testes de integração de serviços |  |
| ⬜ | testes end-to-end de boot/login/app/update |  |
| ⬜ | property tests |  |
| 🟡 | fuzzing | fuzz-lite determinístico + fuzz de syscalls semanal no CI (`make fuzz`); cobertura guiada e fuzz de rede/FS reais pendentes |
| ⬜ | fault injection |  |
| ⬜ | testes de corte de energia |  |
| 🟡 | testes SMP e race conditions | stress multi-CPU básico |
| ⬜ | testes de longa duração |  |
| ⬜ | matriz de hardware |  |
| ⬜ | performance regression gates |  |
| ⬜ | crash dumps e símbolos |  |
| ⬜ | métricas respeitando privacidade |  |
| ⬜ | processo de triagem e regressão |  |

### 6.9 Acessibilidade e internacionalização

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | toda ação essencial acessível por teclado |  |
| ⬜ | árvore semântica de acessibilidade |  |
| ⬜ | leitor de tela |  |
| ⬜ | ampliação e escala |  |
| ⬜ | alto contraste |  |
| ⬜ | redução de movimento |  |
| ⬜ | legendas e indicadores visuais para áudio |  |
| ⬜ | tamanhos de texto ajustáveis |  |
| ⬜ | métodos de entrada |  |
| ⬜ | Unicode completo nas camadas fundamentais |  |
| ⬜ | localização pt-BR e en-US |  |
| ⬜ | formatos de data, hora, número e moeda |  |
| ⬜ | layouts da direita para a esquerda em fase posterior |  |
| ⬜ | testes com pessoas e tecnologias assistivas |  |

### 6.10 Distribuição e operação

| | Item | Evidência / nota |
|---|---|---|
| 🟡 | image builder | tools/build-image (GPT + ESP); instalador/assinatura pendentes |
| ⬜ | instalador |  |
| ⬜ | recovery |  |
| ⬜ | canais de release |  |
| ⬜ | A/B e rollback |  |
| ⬜ | repositório assinado |  |
| ⬜ | servidor de símbolos |  |
| ⬜ | espelhos e CDN quando necessário |  |
| ⬜ | status público |  |
| ⬜ | política de suporte |  |
| ⬜ | compatibilidade de upgrades |  |
| ⬜ | documentação de dual boot |  |
| ⬜ | política de coleta opt-in |  |
| ⬜ | plano de resposta a incidentes |  |

## 5. Governança: documentos obrigatórios e ADRs (Plano §4.3–4.4)

### 4.3 Documentos obrigatórios — 12/12

| | Item | Evidência / nota |
|---|---|---|
| ✅ | `PROJECT_CHARTER.md`: visão, usuário-alvo e não objetivos |  |
| ✅ | `ARCHITECTURE.md`: arquitetura vigente |  |
| ✅ | `SECURITY.md`: contato, política e threat model |  |
| ✅ | `CONTRIBUTING.md`: build, testes e revisão |  |
| ✅ | `CODE_OF_CONDUCT.md` |  |
| ✅ | `LICENSES.md`: licenças do projeto e dependências |  |
| ✅ | `SUPPORTED_HARDWARE.md` |  |
| ✅ | `COMPATIBILITY.md`: ABI, API e formatos |  |
| ✅ | `RELEASE.md`: versionamento e canais |  |
| ✅ | `RECOVERY.md`: backup, rollback e recuperação |  |
| ✅ | diretório `docs/adr/` para decisões arquiteturais |  |
| ✅ | diretório `docs/rfc/` para propostas grandes |  |

### 4.4 ADRs iniciais — 14/14

| | Item | Evidência / nota |
|---|---|---|
| ✅ | ADR-0001 — linguagem principal e política de `unsafe` |  |
| ✅ | ADR-0002 — microkernel híbrido e limites privilegiados |  |
| ✅ | ADR-0003 — x86_64, UEFI e política de arquiteturas |  |
| ✅ | ADR-0004 — modelo de objetos, handles e capabilities |  |
| ✅ | ADR-0005 — formato e versionamento de IPC |  |
| ✅ | ADR-0006 — ABI nativa e política de estabilidade |  |
| ✅ | ADR-0007 — modelo de drivers |  |
| ✅ | ADR-0008 — VFS e namespace |  |
| ✅ | ADR-0009 — pacotes, manifests e identidade de aplicativos |  |
| ✅ | ADR-0010 — atualizações A/B e trust root |  |
| ✅ | ADR-0011 — telemetria opt-in e privacidade |  |
| ✅ | ADR-0012 — licença do projeto e política de dependências |  |
| ✅ | ADR-0013 — shell gráfico e conceito de Contextos |  |
| ✅ | ADR-0014 — estratégia de compatibilidade POSIX/Linux/Web |  |

## 6. Plano dos 90 dias (Plano §8) e checklist de início imediato (§15)

### Semanas 1–2 — Contrato do projeto — 8/8 ✅

| | Item | Evidência / nota |
|---|---|---|
| ✅ | preencher nome provisório, público e promessa de 1.0 |  |
| ✅ | escolher licença |  |
| ✅ | escolher x86_64 + UEFI como plataforma 1 |  |
| ✅ | registrar ADR-0001 a ADR-0004 |  |
| ✅ | criar repositório e estrutura mínima |  |
| ✅ | configurar board de tarefas e milestones |  |
| ✅ | instalar Rust, LLVM/binutils, QEMU, GDB/LLDB e firmware UEFI |  |
| ✅ | registrar versões exatas e comando de setup |  |

### Semanas 3–4 — Imagem inicializável — 7/7 ✅

| | Item | Evidência / nota |
|---|---|---|
| ✅ | compilar um binário UEFI |  |
| ✅ | criar imagem GPT com partição EFI |  |
| ✅ | inicializar no QEMU |  |
| ✅ | escrever no console/framebuffer |  |
| ✅ | escrever no serial |  |
| ✅ | criar comando único `build-image` |  |
| ✅ | criar comando único `run-qemu` |  |

### Semanas 5–6 — Kernel e erros — 7/7 ✅

| | Item | Evidência / nota |
|---|---|---|
| ✅ | separar loader e kernel |  |
| ✅ | transferir mapa de memória e framebuffer |  |
| ✅ | configurar entry point de 64 bits |  |
| ✅ | implementar logger estruturado mínimo |  |
| ✅ | implementar panic |  |
| ✅ | causar e tratar exceção de teste |  |
| ✅ | gerar símbolos e localizar endereço de falha |  |

### Semanas 7–8 — Memória física — 6/6 ✅

| | Item | Evidência / nota |
|---|---|---|
| ✅ | normalizar mapa de memória |  |
| ✅ | marcar regiões reservadas |  |
| ✅ | implementar frame allocator |  |
| ✅ | testar alocação, liberação e exaustão |  |
| ✅ | criar invariantes e testes no host |  |
| ✅ | mapear framebuffer e regiões necessárias |  |

### Semanas 9–10 — Memória virtual e heap — 7/7 ✅

| | Item | Evidência / nota |
|---|---|---|
| ✅ | criar abstração de page tables |  |
| ✅ | mapear/desmapear páginas |  |
| ✅ | aplicar permissões RW/NX |  |
| ✅ | criar guard page |  |
| ✅ | implementar heap do kernel |  |
| ✅ | testar page fault intencional |  |
| ✅ | medir e registrar alocações |  |

### Semanas 11–12 — Interrupções e release inicial — 7/7 ✅

| | Item | Evidência / nota |
|---|---|---|
| ✅ | configurar IDT completa para exceções relevantes |  |
| ✅ | configurar timer |  |
| ✅ | contar ticks e tempo monotônico inicial |  |
| ✅ | executar duas tarefas cooperativas simples |  |
| ✅ | automatizar boot no CI |  |
| ✅ | automatizar timeout e resultado via serial |  |
| ✅ | revisar documentação e publicar `0.0.1-boot` | documentação revisada; tag publicada no GitHub |

### 15. Checklist de início imediato — 14/18 ✅

| | Item | Evidência / nota |
|---|---|---|
| ✅ | escolher nome provisório | Nexo OS |
| ✅ | escrever em uma frase quem usará a versão 1.0 |  |
| 🟡 | escolher dedicação semanal sustentável | meta provisória 10–15 h/semana no charter; confirmar |
| ✅ | definir QEMU `x86_64/q35/UEFI` como alvo inicial |  |
| 🟡 | escolher um único computador de referência futuro | regra definida; modelo a escolher |
| ✅ | escolher Rust `no_std` + Assembly mínimo |  |
| ✅ | escolher licença | MIT OR Apache-2.0 |
| ✅ | criar repositório | github.com/LucasBatista37/nexo-os |
| ✅ | criar board com Fase 0 e primeiros 90 dias | docs/board.md |
| ✅ | criar `PROJECT_CHARTER.md` |  |
| ✅ | escrever ADR-0001 a ADR-0004 |  |
| ✅ | instalar e fixar toolchain |  |
| ✅ | gerar o primeiro binário UEFI |  |
| ⬜ | inicializar no QEMU |  |
| ✅ | obter log serial no CI | tools/test-qemu; make ci |
| ✅ | publicar `0.0.1-boot` | tag no GitHub |
| ✅ | não começar GUI antes de memória, erros e testes básicos | respeitado: só console de diagnóstico |
| ⬜ | revisar este plano no final de cada trimestre |  |

## 7. Currículo de estudo (Plano §9) — atividade pessoal, não implementável pelo repositório

### Nível A — Antes e durante o primeiro boot — 0/9

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | binário, hexadecimal, endianness e complemento de dois |  |
| ⬜ | CPU, registradores, pilha, chamadas e ABI |  |
| ⬜ | memória virtual, páginas e TLB |  |
| ⬜ | Rust ownership, lifetimes, atomics, `no_std` e `unsafe` |  |
| ⬜ | Assembly x86_64 básico |  |
| ⬜ | linker, seções, símbolos, relocation e ELF |  |
| ⬜ | UEFI e mapa de memória |  |
| ⬜ | GDB/LLDB e leitura de disassembly |  |
| ⬜ | Git, CI e builds reproduzíveis |  |

### Nível B — Kernel e concorrência — 0/8

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | interrupções, exceções, APIC e temporizadores |  |
| ⬜ | processos, threads e troca de contexto |  |
| ⬜ | schedulers |  |
| ⬜ | locks, atomics, memory ordering e race conditions |  |
| ⬜ | IPC e passagem de mensagens |  |
| ⬜ | capabilities e modelos de acesso |  |
| ⬜ | DMA, MMIO e IOMMU |  |
| ⬜ | property testing e fuzzing |  |

### Nível C — Sistema completo — 0/10

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | VFS e filesystems |  |
| ⬜ | Ethernet, IP, TCP, DNS e TLS |  |
| ⬜ | PCIe, USB, NVMe e classes de dispositivos |  |
| ⬜ | composição, rasterização, fontes e color management |  |
| ⬜ | áudio digital e sincronização |  |
| ⬜ | energia, ACPI e suspensão |  |
| ⬜ | criptografia aplicada e gestão de chaves |  |
| ⬜ | atualização segura e recuperação |  |
| ⬜ | ABI/API design e compatibilidade |  |
| ⬜ | acessibilidade, internacionalização e UX research |  |

### Projetos de estudo auxiliares — 0/8

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | escrever um alocador em user space |  |
| ⬜ | criar um executor de threads simples |  |
| ⬜ | criar um filesystem em um arquivo de imagem |  |
| ⬜ | criar um protocolo RPC tipado entre processos normais |  |
| ⬜ | criar um renderer 2D por software |  |
| ⬜ | implementar cliente TCP/HTTP educacional em user space |  |
| ⬜ | fuzzar um parser binário próprio |  |
| ⬜ | analisar a arquitetura de Redox, seL4, Fuchsia e Linux sem copiá-las cegamente |  |

## 8. Próximas ações recomendadas (ordem)

1. Rodar `make stress DURATION=86400 SMP=4` (de preferência também em host x86_64 com KVM) e cortar `0.1-kernel` com notas em `docs/releases/`.
2. Fase 2: IDL de protocolos + gerador de código (ADR-0005), fuzzing contínuo de syscalls/IPC/parsers no CI, shell de diagnóstico, múltiplas threads por processo, memória compartilhada, eventos/espera múltipla → `0.2-userspace`.
3. Fase 3: enumeração PCI/ACPI, VirtIO block/input/rng/console em processos isolados, VFS + ramfs + FAT (só EFI), testes de corte de energia → `0.3-storage`.
4. Escolher o computador de referência (regra: um único modelo) antes da Fase 7 e confirmar nome/licença/horas (decisões hoje provisórias).
5. Revisar este documento e o Plano Mestre a cada trimestre (próxima revisão: 2026-11-29).


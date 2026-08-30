# Checklist consolidada do projeto — estado e caminho até a 1.0

Gerado por `tools/roadmap-status` a partir de `PLANO_MESTRE_SISTEMA_OPERACIONAL.md` em 2026-08-30 (commit `597053e`). Legenda: ✅ concluído · 🟡 parcial · ⬜ pendente · ⛔ bloqueado. Percentual = (concluídos + ½ parciais) / total.

**Total de itens do plano:** 535 — ✅ 134 · 🟡 31 · ⬜ 370 · ⛔ 0 → **28% do caminho até a 1.0** (ponderado por item, não por esforço: as fases restantes são muito maiores).

## 1. Visão por fase

| Fase | Gate | ✅ | 🟡 | ⬜ | % | Esforço restante estimado | O que falta (resumo) |
|---|---|---|---|---|---|---|---|
| Fase 0 — Fundação e preparação (meses 0–6) | ✅ atendido | 13 | 2 | 0 | 93% | 0 (concluída) | gate F0 atendido em clone limpo; release 0.0.1-boot publicada |
| Fase 1 — Kernel mínimo confiável (meses 6–18) | 🟡 quase | 17 | 2 | 0 | 95% | 1–3 semanas | só falta executar o stress de 24 h e cortar 0.1-kernel; melhorias: fila por CPU, shootdown com confirmação |
| Fase 2 — Modo usuário, IPC e capabilities (ano 2) | 🟡 quase | 7 | 7 | 4 | 58% | 2–4 meses | IDL + gerador de código, fuzzing sistemático, shell de diagnóstico, jobs/domínios, múltiplas threads por processo, memória compartilhada, eventos/espera múltipla, release 0.2-userspace |
| Fase 3 — Dispositivos virtuais e armazenamento (anos 2–3) | 🟡 em andamento | 8 | 5 | 8 | 50% | 6–12 meses | PCI/ACPI, VirtIO (block/input/rng/console), drivers isolados com DMA/MMIO por capability, cache de blocos, VFS, ramfs, FAT, escrita persistente, testes de corte de energia |
| Fase 4 — Rede e serviços básicos (anos 3–4) | ⬜ não iniciado | 0 | 0 | 19 | 0% | 6–12 meses | VirtIO net, Ethernet/ARP/IPv4/ICMP/UDP/TCP/DHCP/DNS, sockets, IPv6, firewall, TLS portado, HTTP, fuzzing de rede |
| Fase 5 — Gráficos, entrada e shell próprio (anos 3–5) | ⬜ não iniciado | 0 | 0 | 25 | 0% | 12–18 meses | renderer 2D, compositor, entrada, janelas, toolkit, temas, login/sessão, Contextos, Central de Ações, Faixa de Atividades, acessibilidade, testes de usabilidade |
| Fase 6 — Plataforma de aplicativos e desktop essencial (anos 4–6) | ⬜ não iniciado | 0 | 0 | 25 | 0% | 12–18 meses | ABI v1, SDK Rust/C, pacotes assinados, permissões/portais, apps essenciais, terminal, gerenciador de arquivos, editor, configurações, motor web portado |
| Fase 7 — Áudio, mídia, USB e hardware real (anos 5–7) | ⬜ não iniciado | 0 | 0 | 20 | 0% | 12–24 meses | USB/HID/armazenamento, NVMe/AHCI, áudio, Ethernet real, Wi-Fi, GPU/display, energia/suspensão, laboratório de hardware |
| Fase 8 — Segurança, instalação, atualização e recuperação (anos 5–8) | ⬜ não iniciado | 0 | 0 | 22 | 0% | 9–15 meses | criptografia de disco, TPM, trust root, A/B, rollback, recovery, instalador, Secure Boot, backup, crash dumps, fuzzing contínuo, revisão externa |
| Fase 9 — Beta público controlado (anos 7–10) | ⬜ não iniciado | 0 | 0 | 20 | 0% | 12–24 meses | hardware certificado, canais de release, servidores de símbolos/bugs, i18n, acessibilidade AA, documentação, 20→100 testadores, ABI candidata |
| Fase 10 — Versão 1.0 e expansão (anos 8–12+) | ⬜ não iniciado | 0 | 0 | 18 | 0% | contínuo (12+ meses até 1.0) | contrato de ABI 1.x, auditoria, certificação de modelos, repositório stable, SBOM, governança, 1.0; depois aarch64, GPU, VM Linux |

Estimativas assumem uma pessoa com 15–25 h/semana (Plano §13). Somando as faixas, o caminho até a 1.0 fica em **~6 a 10 anos** de trabalho de uma pessoa; com uma equipe pequena (Plano §13.1) cai para 3–5 anos. As fases 0–2 avançaram em dias, não meses, por terem sido feitas com apoio de IA em sessões intensivas — o ritmo real das fases seguintes depende de hardware, drivers e testes com pessoas, que não se comprimem da mesma forma.

## 2. Gates (Plano §5 e §7)

| Gate | Critério (resumo) | Estado | Evidência |
|---|---|---|---|
| Fase 0 | clone limpo → um comando gera imagem que inicia em QEMU; CI comprova a mensagem do kernel | ✅ atendido | clone limpo + `make ci` verde (2026-08-29); `docs/releases/0.0.1-boot.md` |
| Fase 1 | 24 h de stress em QEMU, múltiplas CPUs, memória isolada, exceções tratadas, zero falha inexplicada | 🟡 quase | 30 min de stress com 4 CPUs sem erros; faltam as 24 h (`make stress DURATION=86400`) |
| Fase 2 | 3 processos isolados simultâneos; servidor reinicia sem reiniciar o kernel; acesso sem capability falha de forma testada | 🟡 quase | 4 processos isolados simultâneos e serviço reiniciado sem reiniciar o kernel (`user_services`); negação por direitos testada; falta fuzzing sistemático e protocolo tipado |
| Fase 3 | arquivos sobre VirtIO block persistem; driver de armazenamento pode falhar sem corromper o kernel; cortes de energia simulados | 🟡 em andamento | arquivos criados/alterados/removidos sobre VirtIO-block persistem entre boots (NexoFS v0, `test-qemu --scenario storage`, 2026-08-30); driver de bloco morto sem afetar o kernel (`user_block_crash`); cortes de energia simulados no host em cada escrita — faltam VFS/namespace, cache de blocos, FAT do ESP, IOMMU, VirtIO input/rng/console e cortes no QEMU real |
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

Gate: 🟡 quase. 30 min de stress com 4 CPUs sem erros; faltam as 24 h (`make stress DURATION=86400`)

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

### Fase 2 — Modo usuário, IPC e capabilities (ano 2) — 58% (7 ✅, 7 🟡, 4 ⬜)

Gate: 🟡 quase. 4 processos isolados simultâneos e serviço reiniciado sem reiniciar o kernel (`user_services`); negação por direitos testada; falta fuzzing sistemático e protocolo tipado

| | Item | Evidência / nota |
|---|---|---|
| ✅ | entrar em ring 3 e retornar por syscall | GDT de usuário, `syscall`/`sysret`, `swapgs`; `services/init` roda em ring 3 |
| ✅ | definir ABI de syscall e convenções de erro | ABI v0 em `abi/syscall` + `docs/spec/syscall-abi.md` |
| 🟡 | implementar processos, threads de usuário e jobs/domínios | processos como objetos (spawn/wait/info), uma thread cada; múltiplas threads de usuário e jobs/domínios pendentes |
| ✅ | implementar handles e tabela por processo | `kernel/src/ipc.rs`, syscalls 8–13 |
| 🟡 | implementar direitos: ler, escrever, sinalizar, mapear, transferir e administrar | ler/escrever/transferir/duplicar aplicados; sinalizar/mapear/administrar definidos, sem objetos que os usem |
| ✅ | implementar canais IPC e transferência de handles | canais com filas por extremidade, bloqueio em recv, transferência testada entre processos |
| ⬜ | implementar espera múltipla, eventos e timers |  |
| 🟡 | validar cópias entre usuário e kernel | ponteiros validados por faixa e bit USER antes de copiar (`copy_from_user`); cópia para o usuário ainda não existe |
| ⬜ | criar formato de protocolo tipado e gerador de código |  |
| ✅ | definir regras de compatibilidade do IPC | docs/spec/ipc-compat.md |
| ✅ | criar `init` e `service-manager` | `services/init` e `services/svcmgr` em ring 3 |
| 🟡 | criar políticas de reinício e dependências | reinício com limite implementado e testado (`echo` cai e volta); dependências declarativas pendentes |
| ✅ | criar loader ELF de usuário | `process::spawn_elf` (W^X, USER, pilha com guarda) |
| 🟡 | criar runtime mínimo Rust e ABI C | `sdk/nexo-sys` + `sdk/nexo-rt` (Rust, sem alocação); ABI C pendente |
| ⬜ | criar shell de diagnóstico no espaço de usuário |  |
| 🟡 | testar isolamento e negação de capabilities | isolamento de memória/instruções e negação por direitos (Denied) testados; fuzzing pendente |
| 🟡 | fuzzar decodificador de IPC e syscalls | fuzz-lite determinístico dos parsers no host e fuzz de syscalls de um processo de usuário; fuzzing contínuo (cargo-fuzz/CI) pendente |
| ⬜ | publicar release `0.2-userspace` |  |

### Fase 3 — Dispositivos virtuais e armazenamento (anos 2–3) — 50% (8 ✅, 5 🟡, 8 ⬜)

Gate: 🟡 em andamento. arquivos criados/alterados/removidos sobre VirtIO-block persistem entre boots (NexoFS v0, `test-qemu --scenario storage`, 2026-08-30); driver de bloco morto sem afetar o kernel (`user_block_crash`); cortes de energia simulados no host em cada escrita — faltam VFS/namespace, cache de blocos, FAT do ESP, IOMMU, VirtIO input/rng/console e cortes no QEMU real

| | Item | Evidência / nota |
|---|---|---|
| ✅ | implementar enumeração ACPI mínima | RSDP/XSDT/RSDT/MADT/HPET (`nexo-acpi`, Fase 1); tabelas de PCIe (MCFG) e AML ainda não |
| ✅ | implementar enumeração PCI/PCIe | PCI convencional (config `0xCF8/0xCFC`, barramento 0, BARs por sondagem); PCIe/ECAM pendente |
| 🟡 | definir protocolo driver–device manager | concessão de dispositivo como objeto transferível (ADR-0015); device manager e binding ainda não existem |
| ⬜ | implementar binding por IDs e propriedades |  |
| 🟡 | implementar VirtIO transport | transporte PCI moderno (capabilities, fila dividida, MSI-X) implementado dentro do `blockdev`; ainda não é biblioteca compartilhada |
| ✅ | implementar VirtIO block | `services/blockdev` em modo usuário; teste `user_block` + cenário `storage` |
| ⬜ | implementar VirtIO input |  |
| ⬜ | implementar VirtIO RNG |  |
| ⬜ | implementar VirtIO console |  |
| ✅ | implementar drivers em processos isolados | driver de bloco em ring 3 com handle de dispositivo; queda do driver não afeta o kernel |
| 🟡 | restringir MMIO, IRQ e DMA por capabilities | syscalls 17–23 exigem handle de dispositivo com direitos; MMIO limitado a BARs; falta concessão por dispositivo (BDF) e IOMMU |
| 🟡 | criar abstração IOMMU e caminho sem IOMMU explicitamente inseguro | caminho sem IOMMU documentado como inseguro (ADR-0015); abstração IOMMU pendente |
| ⬜ | implementar cache de blocos e fila assíncrona |  |
| ⬜ | definir VFS e namespace por sessão/processo |  |
| 🟡 | implementar `ramfs` | initramfs somente leitura (`NEXOIRD1`) com membros nomeados; `ramfs` gravável pendente |
| ⬜ | implementar FAT somente para EFI |  |
| ✅ | implementar leitura de um filesystem persistente de teste | NexoFS v0 (ADR-0016): `libraries/nexofs` + `services/fs`; `user_fs` e cenário `storage` |
| ✅ | implementar escrita, flush e sincronização | copy-on-write com commit atômico por setor, `sync`; sem cache de blocos ainda |
| ✅ | criar testes de imagem corrompida e corte de energia | host: corte em cada escrita (com escritas rasgadas) e 400 imagens corrompidas; falta simular cortes no QEMU real |
| ✅ | criar ferramenta de inspeção do disco no host | `tools/nexo-disk` (info/ls/cat/check), usado pelo cenário `storage` |
| ⬜ | publicar release `0.3-storage` |  |

### Fase 4 — Rede e serviços básicos (anos 3–4) — 0% (0 ✅, 0 🟡, 19 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | implementar VirtIO net |  |
| ⬜ | implementar Ethernet e ARP/NDP conforme a fase IPv6 |  |
| ⬜ | implementar IPv4 |  |
| ⬜ | implementar ICMP |  |
| ⬜ | implementar UDP |  |
| ⬜ | implementar TCP com suíte de testes e estados documentados |  |
| ⬜ | implementar DHCP |  |
| ⬜ | implementar DNS com cache e validação de entradas |  |
| ⬜ | criar API de sockets nativa |  |
| ⬜ | criar compatibilidade POSIX de sockets |  |
| ⬜ | implementar IPv6 |  |
| ⬜ | implementar firewall por aplicativo e perfil |  |
| ⬜ | expor permissões de rede por pacote |  |
| ⬜ | portar uma biblioteca TLS auditada compatível com a licença |  |
| ⬜ | criar armazenamento seguro de certificados |  |
| ⬜ | implementar cliente HTTP para atualizações |  |
| ⬜ | fuzzar pacotes, parsers e estados de protocolo |  |
| ⬜ | criar captura de rede autorizada para diagnóstico |  |
| ⬜ | publicar release `0.4-network` |  |

### Fase 5 — Gráficos, entrada e shell próprio (anos 3–5) — 0% (0 ✅, 0 🟡, 25 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | obter framebuffer UEFI e modos de vídeo |  |
| ⬜ | criar renderer 2D por software |  |
| ⬜ | implementar cores, composição alfa, clipping e transformações |  |
| ⬜ | implementar rasterização de texto e fallback de fontes |  |
| ⬜ | definir protocolo de superfícies e buffers |  |
| ⬜ | implementar compositor em espaço de usuário |  |
| ⬜ | implementar double/triple buffering e damage tracking |  |
| ⬜ | integrar mouse e teclado pelo serviço de entrada |  |
| ⬜ | implementar foco, atalhos e captura segura |  |
| ⬜ | implementar janelas, redimensionamento, maximização e mosaico |  |
| ⬜ | implementar múltiplos displays emulado |  |
| ⬜ | criar toolkit UI nativo e tokens de design |  |
| ⬜ | criar gerenciamento de temas claro/escuro e alto contraste |  |
| ⬜ | criar login, bloqueio e sessão |  |
| ⬜ | prototipar e testar o modelo de Contextos |  |
| ⬜ | implementar Central de Ações |  |
| ⬜ | implementar Faixa de Atividades |  |
| ⬜ | criar notificações e controles de atenção |  |
| ⬜ | implementar clipboard com mediação e histórico opt-in |  |
| ⬜ | implementar drag-and-drop por grants |  |
| ⬜ | implementar leitor de tela em arquitetura, ainda que simples |  |
| ⬜ | implementar navegação completa por teclado |  |
| ⬜ | implementar escala fracionária e redução de movimento |  |
| ⬜ | testar usabilidade com usuários externos |  |
| ⬜ | publicar release `0.5-desktop` |  |

### Fase 6 — Plataforma de aplicativos e desktop essencial (anos 4–6) — 0% (0 ✅, 0 🟡, 25 ⬜)

Gate: ⬜ não iniciado. 

| | Item | Evidência / nota |
|---|---|---|
| ⬜ | estabilizar ABI nativa v1 experimental |  |
| ⬜ | publicar SDK Rust |  |
| ⬜ | publicar headers e toolchain C/C++ |  |
| ⬜ | criar gerador de projeto |  |
| ⬜ | criar documentação e exemplos |  |
| ⬜ | criar depurador remoto e integração com GDB/LLDB quando viável |  |
| ⬜ | criar profiler e visualizador de traces |  |
| ⬜ | definir formato de pacote e manifesto |  |
| ⬜ | implementar assinatura e verificação de pacotes |  |
| ⬜ | implementar instalação transacional |  |
| ⬜ | implementar permissões declarativas e consentimento |  |
| ⬜ | criar portal de arquivos, câmera, microfone e notificações |  |
| ⬜ | criar repositório de pacotes de desenvolvimento |  |
| ⬜ | criar processo de revisão e revogação |  |
| ⬜ | portar toolchain e utilitários POSIX prioritários |  |
| ⬜ | criar terminal e shell |  |
| ⬜ | criar gerenciador de arquivos |  |
| ⬜ | criar editor de texto |  |
| ⬜ | criar configurações |  |
| ⬜ | criar monitor de sistema |  |
| ⬜ | criar visualizador de imagens e documentos básicos |  |
| ⬜ | criar calculadora, calendário e utilitários |  |
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
| 🟡 | fuzzing | fuzz-lite determinístico (parsers e syscalls); fuzzing contínuo pendente |
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


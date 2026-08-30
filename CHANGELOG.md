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

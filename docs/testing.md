# Testes

## Camadas

| Camada | Comando | O que cobre |
|---|---|---|
| Host (`cargo test`) | `cargo test --workspace` | ABI de boot (layout/validação/cmdline), endereços, normalização do mapa, bitmap de quadros, paginação (arena), heap (inclui aleatório com verificação de corrupção), spinlock/once, símbolos/demangler, fonte |
| Kernel em QEMU (`selftest`) | `tools/test-qemu --scenario boot` (4 CPUs, disco virtio-blk de 16 MiB) | 40 testes no kernel real: boot info, segmentos, `#BP`, quadros, map/unmap, permissões de seção, recuperação de `#PF`, guard pages, CR0.WP, NX, heap, crescimento do heap, timer, ACPI, timer do LAPIC, relógio TSC, I/O APIC, IPI, SMP (4/4 online, broadcast, shootdown), threads (yield, preempção, sleep/join, churn, multi-CPU, afinidade), timers de kernel, processos de usuário (syscalls, isolamento, erro de syscall, IPC, serviços, fuzz), PCI (`pci`), driver de bloco em modo usuário com MSI-X (`user_block`, `user_block_crash`), sistema de arquivos (`user_fs`), gerenciador de dispositivos com concessões por função + RNG + leitura da ESP (`user_devmgr`), VFS com namespaces por processo (`user_vfs`), espera múltipla de canais (`user_wait_any`), símbolos |
| Persistência | `tools/test-qemu --scenario storage` | dois boots com o mesmo disco recém-criado: o 1º grava um marcador cru via `blockdev` e formata/escreve o NexoFS (`boot.count`=1), o 2º encontra o marcador, monta sem reparos e lê `boot.count`=2; depois `tools/nexo-disk ls/cat/check` confirma no host |
| Shell interativo | `tools/test-qemu --scenario shell` | QEMU com `--console-socket`: o teste conecta no socket UNIX, espera o prompt e executa 13 comandos (`info`, `ls /boot/nexo`, `escreve`/`cat`/`remove` em `/tmp` e `/disk`, `eco`, `sair`) verificando cada resposta; kernel encerra limpo |
| Firewall (host) | `cargo test -p nexo-netstack` (módulo `firewall`) | política por perfil (negar por padrão), regras de sub-rede/porta/protocolo, `allow_dns`/`allow_listen`, distinção de motivos de negação |
| IPv6/NDP (host) | `cargo test -p nexo-netstack` (módulo `ipv6`) | link-local/solicited-node, checksum com pseudo-cabeçalho, NS→NA (resolução de vizinho) e ICMPv6 echo, com fuzz-lite |
| Rede (QEMU) | `tools/test-qemu --scenario net` | `run-qemu --net` (slirp), duas fases num boot: (1) caminho cru — DHCP, ARP, ICMP echo, DNS e handshake TCP com RST; (2) `netd` — API de sockets (`nexo.sock`): info do lease, DNS com cache (2ª consulta `cached=1`), eco UDP, TCP conectar/enviar/receber/fechar contra servidores reais no host uma **conexão de entrada** aceita pelo `netd` (`tcp_listen` + `--net-hostfwd`) e um GET HTTP/1.0 com resposta 200 validada; captura pcap em `build/logs/net.pcap` (`--net-dump`); parsers e máquina de estados com suíte no host (`cargo test -p nexo-netstack`) |
| Entrada (QEMU) | `tools/test-qemu --scenario input` | teclas injetadas pelo host via QMP chegam ao driver `inputdev` em ring 3 e ao cliente com os códigos evdev corretos |
| Corte de energia (QEMU) | `tools/test-qemu --scenario powercut` | 1º boot com `fs-churn=1`: `utest` (modo 10) cria/sobrescreve/estende/trunca/remove arquivos sem parar; o host mata o QEMU com SIGKILL (`run-qemu --kill-on … --kill-delay-ms 150`) no meio das escritas; 2º boot normal com o mesmo disco: o `fs` monta (reparando órfãos se houver, sem reformatar), `user_fs` passa e `tools/nexo-disk check` confirma o volume consistente |
| Protocolos tipados (host) | `cargo test -p nexo-proto` | cabeçalho NXIP (layout, flags reservadas, `payload_len`), ida-e-volta do `nexo.rng`, compatibilidade com payload estendido, erro remoto, fuzz-lite de 20 000 mutações |
| FAT/GPT (host) | `cargo test -p nexo-fat` | FAT12 construída à mão (8.3, caixa NT, nomes longos, subdiretório, leitura com offset), 500 imagens corrompidas sem pânico, GPT sintética; com mtools instalado, uma FAT32 real (`mformat`/`mcopy`) igual ao ESP |
| Sistema de arquivos (host) | `cargo test -p nexofs` | ciclo de arquivos, arquivos grandes (bloco indireto), 100 entradas, disco cheio, corte de energia em cada escrita (incl. escritas rasgadas) com verificação de versões permitidas e de ausência de vazamento, 400 imagens corrompidas sem pânico |
| Queda de driver | teste `user_block_crash` | `blockdev` morre no 2º pedido; cliente recebe `PeerClosed`, kernel segue, quadros/canais/vetor de IRQ liberados |
| Cenários de falha | `tools/test-qemu --scenario panic|fault|overflow` | panic com backtrace simbolizado; `#PF` fatal com CR2/RIP/backtrace; estouro de pilha → `#DF` em IST1 |
| Stress (SMP) | `tools/test-qemu --scenario stress` (15 s no CI) · `make stress DURATION=86400 SMP=4` (gate F1) | threads de lock/atomics/heap/sleep/spawn-join/map-unmap em todas as CPUs; a cada segundo `[STRESS] t=…` com invariantes (contador com lock exato, heap e quadros sem vazamento, ≥ 2 CPUs); fim com `[STRESS] PASS` |
| Fuzz-lite (host) | `cargo test --workspace` (testes `fuzz_lite_*`) | mutação determinística de entradas válidas nos parsers de ELF, initrd, ACPI, símbolos e ABI de boot: nunca podem entrar em pânico |
| Fuzz de syscalls (kernel) | teste `user_syscall_fuzz` (`utest` modo 7) | 20 000 syscalls aleatórias de um processo de usuário; o processo sobrevive e o kernel não vaza quadros nem canais |
| Fuzzing contínuo | `make fuzz DURATION=1800` · workflow `fuzz` (semanal/cron + manual) | rodadas de 20 000 syscalls com sementes aleatórias do TSC (logadas para reprodução) até esgotar o tempo, checando vazamentos por rodada; host: todos os testes `*fuzz*` |
| Reprodutibilidade | `make reproducible` | duas builds → mesma imagem |
| Lint | `make lint` | fmt + clippy `-D warnings` nos três workspaces |

## Protocolo serial (o que o CI verifica)

- Sucesso: `[RESULT] PASS n/n` e `NEXO: boot completo`, QEMU sai com **33** (`isa-debug-exit`, valor `0x10`).
- Falha: qualquer `FAIL`, `KERNEL PANIC` ou `EXCEPTION` no cenário `boot`; código **35** nos cenários de falha esperada.
- Timeout (`--timeout`, padrão 120 s) → código 124.
- Marcadores estruturados: `[TEST] nome ... ok|FAIL: motivo`, `[MEMORY] …`, `[HEAP] …`, `[TIME] …`.

## Escrevendo um teste de kernel

1. Adicione `("nome", fn)` em `kernel/src/selftest.rs::TESTS`; use `check!(cond, "motivo {x}")`.
2. Para acessos que devem falhar, use `x86::traps::probe(ProbeKind::Read|Write|Exec, addr)` — a falta é capturada e o teste continua.
3. Acrescente o marcador `\[TEST\] nome \.\.\. ok` em `tools/test-qemu`.
4. Logs ficam em `build/logs/<cenário>.log`; `tools/symbolize` resolve endereços.

## Depuração

`tools/run-qemu --gdb` inicia parado com gdbstub em `:1234`. No `lldb`: `target create build/kernel.elf`, `gdb-remote 1234`, `b nexo_kernel::kmain`, `c`. Os símbolos do kernel estão em `build/kernel.sym` (`llvm-nm -n`).

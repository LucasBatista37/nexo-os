# Inventário de `unsafe`

**Gerado por `tools/nexo-unsafe-audit --inventario` (`make unsafe-inventory`). Não editar à
mão** — o preâmbulo vive no próprio gerador. Uma versão anterior deste arquivo era escrita à
mão e ficou defasada em 119 usos antes de alguém reparar; um inventário que envelhece é pior
que nenhum, porque dá a impressão de que se sabe o que há na árvore.

Regra (ADR-0001): **todo** uso de `unsafe` tem uma justificativa `SAFETY:` adjacente, e
`make lint` recusa código que a esqueça. Este arquivo é o resultado: cada uso da árvore, o
ficheiro e linha onde vive, e a invariante que o autor afirmou. É a resposta ao item "documentar
todas as invariantes `unsafe`" do Plano §6.1 — e, por ser gerado, continua a ser a resposta
amanhã.

Como ler: uma justificativa que não diga *por que a invariante vale* é uma dívida, não uma
justificativa. Revisar este arquivo a cada release é a forma barata de as encontrar.


**476 usos de `unsafe`, 476 com justificativa registrada.**

| Crate | Usos |
| --- | ---: |
| `nexo-kernel` | 146 |
| `nexo-arch-x86_64` | 126 |
| `nexo-sys` | 56 |
| `nexo-heap` | 30 |
| `nexo-utest` | 26 |
| `nexo-loader` | 15 |
| `nexo-nvmedev` | 12 |
| `nexo-virtio` | 10 |
| `nexo-sync` | 9 |
| `nexo-blockdev` | 8 |
| `nexo-ahcidev` | 6 |
| `nexo-wmd` | 3 |
| `nexo-consoledev` | 2 |
| `nexo-editor` | 2 |
| `nexo-lanc` | 2 |
| `nexo-netdev` | 2 |
| `nexo-shellui` | 2 |
| `nexo-vfs` | 2 |
| `nexo-visor` | 2 |
| `nexo-agenda` | 1 |
| `nexo-arquivos` | 1 |
| `nexo-backup` | 1 |
| `nexo-boot-abi` | 1 |
| `nexo-calc` | 1 |
| `nexo-config` | 1 |
| `nexo-echo` | 1 |
| `nexo-greeter` | 1 |
| `nexo-inputdev` | 1 |
| `nexo-monitor` | 1 |
| `nexo-netd` | 1 |
| `nexo-portal` | 1 |
| `nexo-rngdev` | 1 |
| `nexo-term` | 1 |
| `nexo-upd` | 1 |

Crates sem nenhum `unsafe` não aparecem aqui: os puros declaram `forbid(unsafe_code)`.

## `nexo-kernel` — 146 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `kernel/src/acpi.rs:26` | bloco | intervalo dentro do physmap; tabelas ACPI são somente leitura. |
| `kernel/src/aslr.rs:17` | bloco | a instrução existe (bit 30 de CPUID.1:ECX conferido acima); só escreve em `v`. |
| `kernel/src/boot.rs:69` | bloco | o loader gravou `memory_map_len` regiões nesse endereço, em páginas reservadas (BootInfo) que nunca são reutilizadas. |
| `kernel/src/boot.rs:80` | bloco | páginas do tipo Initrd, reservadas e imutáveis. |
| `kernel/src/cell.rs:13` | impl | a disciplina de acesso é responsabilidade dos chamadores (`as_ptr` é usado apenas em inicialização single-core ou em leitura após publicação). |
| `kernel/src/console.rs:36` | impl | o ponteiro do framebuffer é acessado apenas sob o lock. |
| `kernel/src/console.rs:56` | bloco | (xx, yy) dentro da resolução; stride*height*4 <= tamanho do fb. |
| `kernel/src/console.rs:239` | fn | # Safety Somente quando o detentor não voltará a executar. |
| `kernel/src/console.rs:241` | bloco | contrato da função. |
| `kernel/src/crashdump.rs:120` | bloco | init roda uma vez, single-core, antes de qualquer pânico possível usar TARGET. |
| `kernel/src/crashdump.rs:172` | bloco | página pré-alocada exclusiva deste caminho; único uso (guarda ONCE). |
| `kernel/src/crashdump.rs:200` | bloco | `rbp` validado dentro de uma pilha mapeada, alinhado a 8. |
| `kernel/src/crashdump.rs:217` | bloco | TARGET publicado no init (single-core) e lido uma vez (guarda ONCE). |
| `kernel/src/crashdump.rs:245` | bloco | página pré-alocada exclusiva; layout do pedido virtio-blk. |
| `kernel/src/crashdump.rs:264` | bloco | mesma página; status escrito pelo dispositivo. |
| `kernel/src/klog.rs:74` | bloco | COM1 é a UART padrão do PC; a inicialização é idempotente. |
| `kernel/src/klog.rs:152` | fn | # Safety Somente quando o detentor não voltará a executar (panic/exceção fatal). |
| `kernel/src/klog.rs:154` | bloco | contrato da função. |
| `kernel/src/main.rs:61` | extern | # Safety Só pode ser chamada pelo loader, com o estado de máquina descrito em `docs/spec/boot-abi.md` §2 e `boot_info` apontando para um `BootInfo` válido. |
| `kernel/src/main.rs:63` | bloco | contrato de boot (docs/spec/boot-abi.md): ponteiro válido e imutável. |
| `kernel/src/mm/heap.rs:24` | impl | todas as operações são serializadas pelo spinlock com interrupções desabilitadas; o crescimento só toca páginas exclusivas do heap. |
| `kernel/src/mm/heap.rs:26` | fn | SAFETY (contrato GlobalAlloc): layout válido; devolve nulo em falta de memória. |
| `kernel/src/mm/heap.rs:48` | fn | SAFETY (contrato GlobalAlloc): `ptr` veio de `alloc` com o mesmo layout. |
| `kernel/src/mm/heap.rs:51` | bloco | `ptr` veio de `alloc` deste heap (contrato de GlobalAlloc). |
| `kernel/src/mm/heap.rs:79` | bloco | páginas recém-mapeadas, exclusivas do heap. |
| `kernel/src/mm/heap.rs:118` | bloco | região recém-mapeada, exclusiva do heap. |
| `kernel/src/mm/phys.rs:54` | bloco | inicialização única em uma CPU; depois só há leituras. |
| `kernel/src/mm/phys.rs:99` | bloco | região utilizável, dentro do physmap, exclusiva do bitmap a partir daqui. |
| `kernel/src/mm/phys.rs:125` | bloco | após `init`, o array é imutável. |
| `kernel/src/mm/phys.rs:144` | bloco | quadro recém-alocado, dentro do physmap. |
| `kernel/src/mm/virt.rs:32` | bloco | CR3 aponta para a PML4 construída pelo loader, coberta pelo physmap. |
| `kernel/src/mm/virt.rs:36` | extern | símbolos do script de linker; só os ENDEREÇOS são usados (nunca os valores). |
| `kernel/src/mm/virt.rs:66` | bloco | o kernel executa e usa pilha apenas na metade superior. |
| `kernel/src/mm/virt.rs:72` | bloco | seções somente-leitura do kernel foram mapeadas sem WRITABLE. |
| `kernel/src/mm/virt.rs:77` | bloco | todo acesso do kernel a memória de usuário passa por `x86::usercopy`, que liga EFLAGS.AC durante a cópia; o resto do kernel escreve em páginas de usuário pelo alias do physmap (páginas do kernel), que SMAP não alcança. |
| `kernel/src/mm/virt.rs:154` | bloco | tabela de páginas dentro do physmap; leitura de uma entrada alinhada. |
| `kernel/src/panic.rs:21` | bloco | o detentor de qualquer lock de saída não voltará a executar. |
| `kernel/src/panic.rs:93` | bloco | `rbp` está dentro de uma pilha mapeada e alinhado a 8. |
| `kernel/src/pci.rs:20` | bloco | acesso serializado pelo lock. |
| `kernel/src/pci.rs:27` | bloco | acesso serializado pelo lock. |
| `kernel/src/pci.rs:284` | bloco | página mapeada por `ecam_init` como MMIO sem cache; leitura alinhada de 32 bits dentro da janela de 4 KiB daquela função. |
| `kernel/src/process.rs:75` | bloco | ambas as tabelas estão no physmap; copia as 256 entradas da metade alta. |
| `kernel/src/process.rs:95` | bloco | PML4 válida construída em `new`. |
| `kernel/src/process.rs:174` | bloco | página mapeada e exclusiva do processo; `n` respeita o limite da página. |
| `kernel/src/process.rs:190` | bloco | a PML4 do kernel mapeia tudo que o kernel usa. |
| `kernel/src/process.rs:532` | bloco | ponteiro de um `Box<UserStart>` vazado por `spawn_elf`. |
| `kernel/src/process.rs:537` | bloco | entrada e pilha mapeadas com USER; CR3 do processo já ativo; gs:[8] apontando para a pilha de kernel desta thread (set_current). |
| `kernel/src/sched.rs:93` | impl | `inner`/`entry` só são acessados com o lock do escalonador detido (ou pela própria thread ao iniciar); o restante é atômico/imutável. |
| `kernel/src/sched.rs:95` | impl | idem. |
| `kernel/src/sched.rs:103` | fn | # Safety O chamador detém `SCHED` (exclusão mútua externa ao empréstimo). |
| `kernel/src/sched.rs:105` | bloco | contrato da função. |
| `kernel/src/sched.rs:113` | bloco | lock detido. |
| `kernel/src/sched.rs:285` | bloco | a thread atual está viva (mantida em `all`) enquanto ocupa a CPU. |
| `kernel/src/sched.rs:301` | bloco | ponteiro para thread viva enquanto ocupa a CPU. |
| `kernel/src/sched.rs:318` | bloco | pilha recém-alocada e exclusiva. |
| `kernel/src/sched.rs:361` | bloco | registro inicial, sem concorrência sobre esta thread. |
| `kernel/src/sched.rs:388` | bloco | `arg` é o ponteiro de um `Arc<Thread>` mantido vivo em `all`. |
| `kernel/src/sched.rs:391` | bloco | IDT e LAPIC prontos; estamos fora de qualquer lock. |
| `kernel/src/sched.rs:393` | bloco | `entry` só é lido aqui, uma única vez, pela própria thread. |
| `kernel/src/sched.rs:440` | bloco | pilha recém-mapeada e exclusiva. |
| `kernel/src/sched.rs:487` | bloco | lock detido. |
| `kernel/src/sched.rs:499` | bloco | lock detido; `cur` e `next` são distintas. |
| `kernel/src/sched.rs:543` | bloco | a metade do kernel é idêntica em todas as PML4s; pilhas e código continuam mapeados. Escrever em CR3 esvazia TLB e caches de estrutura de paginação. |
| `kernel/src/sched.rs:549` | bloco | lock detido; `sp` só é tocado aqui e na troca. |
| `kernel/src/sched.rs:551` | bloco | lock detido; `next` não executa em nenhuma CPU neste instante. |
| `kernel/src/sched.rs:556` | bloco | lock detido; `fx` só é tocado aqui, pela CPU que executa a troca. |
| `kernel/src/sched.rs:566` | bloco | `prev_sp` aponta para o campo de uma thread viva; `next_sp` foi preparado por `prepare_stack` ou salvo por uma troca anterior. |
| `kernel/src/sched.rs:575` | bloco | o lock está detido por construção (esquecido antes da troca). |
| `kernel/src/sched.rs:578` | bloco | lock detido. |
| `kernel/src/sched.rs:581` | bloco | lock detido. |
| `kernel/src/sched.rs:592` | bloco | lock detido. |
| `kernel/src/sched.rs:595` | bloco | lock detido. |
| `kernel/src/sched.rs:605` | bloco | fim da seção crítica iniciada por quem esqueceu o guard. |
| `kernel/src/sched.rs:642` | bloco | lock detido. |
| `kernel/src/sched.rs:664` | bloco | lock detido. |
| `kernel/src/sched.rs:677` | bloco | lock detido. |
| `kernel/src/sched.rs:712` | bloco | lock detido. |
| `kernel/src/sched.rs:717` | bloco | lock detido. |
| `kernel/src/sched.rs:751` | bloco | estado anterior ao bloqueio. |
| `kernel/src/sched.rs:763` | bloco | lock detido. |
| `kernel/src/sched.rs:766` | bloco | lock detido. |
| `kernel/src/sched.rs:820` | bloco | lock detido. |
| `kernel/src/sched.rs:824` | bloco | lock detido. |
| `kernel/src/sched.rs:837` | bloco | lock detido. |
| `kernel/src/sched.rs:856` | bloco | lock detido. |
| `kernel/src/selftest.rs:298` | bloco | #BP é tratado pelo handler, que apenas conta e retorna. |
| `kernel/src/selftest.rs:321` | bloco | quadro alocado, dentro do physmap. |
| `kernel/src/selftest.rs:371` | bloco | página recém-mapeada RW; mesmo quadro visto pelo physmap. |
| `kernel/src/selftest.rs:674` | bloco | o canal 0 do PIT esta livre (o tick do sistema vem do LAPIC). |
| `kernel/src/selftest.rs:678` | bloco | encerra o canal 0. |
| `kernel/src/selftest.rs:1222` | bloco | leitura de configuração PCI de uma função já enumerada; sem efeitos. |
| `kernel/src/selftest.rs:1338` | bloco | quadro recém-mapeado, escrito pelo alias do physmap (página do kernel). |
| `kernel/src/selftest.rs:3033` | bloco | px_addr está dentro do framebuffer (validado contra o BAR); physmap o cobre. |
| `kernel/src/selftest.rs:4681` | bloco | quadro recém-alocado, exclusivo, mapeado no physmap. |
| `kernel/src/selftest.rs:4729` | bloco | quadro recém-alocado, exclusivo, mapeado no physmap. |
| `kernel/src/selftest.rs:4792` | bloco | quadro recém-alocado, exclusivo, mapeado no physmap. |
| `kernel/src/selftest.rs:4861` | bloco | quadro recém-alocado, exclusivo, mapeado no physmap. |
| `kernel/src/selftest.rs:5381` | bloco | deliberadamente inválido — o objetivo é exercitar o caminho fatal de #PF. |
| `kernel/src/stress.rs:142` | bloco | página recém-mapeada RW e exclusiva desta thread. |
| `kernel/src/symbols.rs:20` | bloco | páginas do tipo KernelFile, reservadas e imutáveis. |
| `kernel/src/sync.rs:53` | bloco | estavam habilitadas antes. |
| `kernel/src/sync.rs:64` | fn | # Safety Ver [`SpinLock::force_unlock`]. |
| `kernel/src/sync.rs:66` | bloco | contrato da função. |
| `kernel/src/sync.rs:75` | bloco | solta o guard interno exatamente uma vez. |
| `kernel/src/sync.rs:98` | bloco | o guard interno é solto exatamente uma vez, antes de reabilitar interrupções. |
| `kernel/src/sync.rs:101` | bloco | estavam habilitadas quando o lock foi adquirido. |
| `kernel/src/time.rs:34` | bloco | PIT canal 2 é dedicado à calibração; IRQs estão desabilitadas. |
| `kernel/src/time.rs:57` | bloco | encerra a contagem do canal 2. |
| `kernel/src/time.rs:81` | bloco | IDT instalada com handler para o vetor do timer. |
| `kernel/src/x86/apic.rs:67` | bloco | habilitar o APIC global é pré-requisito do resto da inicialização. |
| `kernel/src/x86/apic.rs:75` | bloco | página do LAPIC mapeada sem cache. |
| `kernel/src/x86/apic.rs:81` | bloco | IDT instalada; PIC não será mais usado. |
| `kernel/src/x86/apic.rs:96` | bloco | página mapeada sem cache. |
| `kernel/src/x86/gdt.rs:28` | bloco | executado uma única vez, em uma única CPU, antes de qualquer outro acesso a GDT/TSS; depois de carregadas só há leituras pela CPU. |
| `kernel/src/x86/gdt.rs:39` | bloco | GDT contém código em 0x08, dados em 0x10 e o TSS em `tss_sel`. |
| `kernel/src/x86/percpu.rs:67` | impl | cada CPU só acessa a própria estrutura de forma mutável durante a inicialização; depois disso apenas campos atômicos são alterados. |
| `kernel/src/x86/percpu.rs:69` | impl | a estrutura é vazada para 'static e nunca movida após `allocate`. |
| `kernel/src/x86/percpu.rs:112` | bloco | `tss` vive dentro da estrutura vazada ('static) e não se move. |
| `kernel/src/x86/percpu.rs:129` | fn | # Safety Uma única vez por CPU, durante a inicialização dela. |
| `kernel/src/x86/percpu.rs:131` | bloco | GDT com código 0x08, dados 0x10 e TSS 0x18; estrutura é 'static. |
| `kernel/src/x86/percpu.rs:143` | bloco | campos lidos apenas pela CPU dona (hardware) em transições de privilégio. |
| `kernel/src/x86/percpu.rs:168` | bloco | primeira ativação na BSP. |
| `kernel/src/x86/percpu.rs:189` | bloco | `gs` aponta para uma estrutura 'static configurada por `activate`. |
| `kernel/src/x86/smp.rs:24` | bloco | a BSP gravou o ponteiro de uma estrutura 'static no trampolim. |
| `kernel/src/x86/smp.rs:27` | bloco | primeira ativação nesta CPU. |
| `kernel/src/x86/smp.rs:47` | bloco | IDT carregada e LAPIC configurado nesta CPU. |
| `kernel/src/x86/smp.rs:110` | bloco | página 0x8000 reservada (primeiro MiB), acessível pelo physmap. |
| `kernel/src/x86/syscall.rs:316` | bloco | FramebufferInfo é repr(C) com 40 bytes sem padding (8+8+4×6); lemos seus bytes. |
| `kernel/src/x86/syscall.rs:688` | bloco | PciInfo é repr(C) sem padding interno relevante para leitura como bytes. |
| `kernel/src/x86/syscall.rs:771` | bloco | quadro recem-alocado, visivel pelo physmap, ainda nao entregue ao usuario. |
| `kernel/src/x86/syscall.rs:784` | bloco | DmaBuffer é repr(C) de inteiros. |
| `kernel/src/x86/syscall.rs:813` | bloco | IrqInfo é repr(C) de inteiros. |
| `kernel/src/x86/syscall.rs:1036` | bloco | Event e repr(C) de 16 bytes sem padding invalido; got <= cap. |
| `kernel/src/x86/syscall.rs:1133` | bloco | `lista` é um `Vec<ProcInfo>` (repr(C), sem padding indefinido) vivo aqui. |
| `kernel/src/x86/syscall.rs:1290` | bloco | frame empilhado por `nexo_syscall_entry` na pilha de kernel desta thread. |
| `kernel/src/x86/syscall.rs:1292` | bloco | estamos na pilha de kernel com gs configurado; a syscall pode bloquear. |
| `kernel/src/x86/traps.rs:35` | bloco | inicialização única em uma CPU; a tabela nunca mais é escrita. |
| `kernel/src/x86/traps.rs:42` | bloco | todos os handlers apontam para stubs válidos gerados em assembly. |
| `kernel/src/x86/traps.rs:56` | fn | # Safety `init` deve ter sido executado pela BSP. |
| `kernel/src/x86/traps.rs:58` | bloco | tabela preenchida por `init`, nunca mais escrita. |
| `kernel/src/x86/traps.rs:272` | bloco | caminho fatal; ninguém mais usará os locks de saída. |
| `kernel/src/x86/traps.rs:305` | bloco | caminho fatal. |
| `kernel/src/x86/traps.rs:328` | bloco | caminho fatal. |
| `kernel/src/x86/traps.rs:465` | bloco | o handler de #PF redireciona RIP para o rótulo `2:` quando a falta ocorre na página sondada; nos demais casos o acesso é válido (páginas mapeadas do kernel). Para `Exec`, a página-alvo contém `ret`. |
| `kernel/src/x86/usercopy.rs:60` | extern | símbolos definidos pelo `global_asm!` logo acima, nesta mesma unidade de compilação; as assinaturas correspondem ao que o código em assembly faz (System V: rdi, rsi, rdx → rax). |
| `kernel/src/x86/usercopy.rs:118` | bloco | a rotina só executa `rep movsb` entre dois ponteiros fornecidos por quem chama; uma falta de página dentro dela é desviada pelo handler de `#PF` para a retomada, que devolve 1 sem tocar em mais nada. Não há outra memória envolvida. |

## `nexo-arch-x86_64` — 126 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `arch/x86_64/src/apic.rs:80` | impl | registradores MMIO por CPU; cada CPU acessa o próprio LAPIC pelo mesmo endereço virtual. |
| `arch/x86_64/src/apic.rs:82` | impl | idem; escritas são operações atômicas de 32 bits no MMIO local. |
| `arch/x86_64/src/apic.rs:89` | fn | # Safety `virt_base` deve mapear a página do LAPIC sem cache. |
| `arch/x86_64/src/apic.rs:96` | bloco | registrador dentro da página mapeada do LAPIC. |
| `arch/x86_64/src/apic.rs:102` | bloco | idem. |
| `arch/x86_64/src/apic.rs:216` | bloco | MSR existe em toda CPU com APIC. |
| `arch/x86_64/src/apic.rs:228` | fn | # Safety Altera o roteamento de interrupções da CPU. |
| `arch/x86_64/src/apic.rs:230` | bloco | contrato da função. |
| `arch/x86_64/src/context.rs:40` | extern | implementada no global_asm! acima com esta exata assinatura. |
| `arch/x86_64/src/context.rs:54` | fn | # Safety `[stack_top - 64, stack_top)` deve ser memória gravável exclusiva da tarefa. |
| `arch/x86_64/src/context.rs:59` | bloco | contrato da função; layout casado com os `pop`s do assembly. |
| `arch/x86_64/src/cpu.rs:28` | fn | # Safety E/S de porta pode ter qualquer efeito colateral no hardware. |
| `arch/x86_64/src/cpu.rs:30` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:40` | fn | # Safety Ver [`outb`]. |
| `arch/x86_64/src/cpu.rs:43` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:54` | fn | # Safety Ver [`outb`]. |
| `arch/x86_64/src/cpu.rs:56` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:66` | fn | # Safety Ver [`outb`]. |
| `arch/x86_64/src/cpu.rs:69` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:80` | fn | # Safety MSR inexistente causa #GP. |
| `arch/x86_64/src/cpu.rs:83` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:94` | fn | # Safety Pode alterar o modo de operação da CPU. |
| `arch/x86_64/src/cpu.rs:98` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:108` | bloco | leitura de registrador de controle é livre de efeitos colaterais. |
| `arch/x86_64/src/cpu.rs:117` | fn | # Safety Altera paginação/proteção. |
| `arch/x86_64/src/cpu.rs:119` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:127` | bloco | leitura sem efeitos colaterais. |
| `arch/x86_64/src/cpu.rs:136` | bloco | leitura sem efeitos colaterais. |
| `arch/x86_64/src/cpu.rs:145` | fn | # Safety As novas tabelas devem mapear o código em execução e a pilha. |
| `arch/x86_64/src/cpu.rs:147` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:167` | fn | # Safety Depois disto, qualquer acesso do kernel a uma página de usuário **fora** de uma janela com `EFLAGS.AC` ligado vira falta de página, e executar página de usuário em ring 0 idem. Só é seguro chamar quando todos esses acessos passam por um caminho que liga AC (em Nexo, a cópia protegida de `x86::usercopy`). |
| `arch/x86_64/src/cpu.rs:179` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:189` | bloco | leitura sem efeitos colaterais. |
| `arch/x86_64/src/cpu.rs:198` | fn | # Safety Altera recursos da CPU. |
| `arch/x86_64/src/cpu.rs:200` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:207` | bloco | invalidar TLB é sempre seguro (apenas custo). |
| `arch/x86_64/src/cpu.rs:214` | bloco | reescrever o mesmo CR3 é seguro. |
| `arch/x86_64/src/cpu.rs:221` | bloco | instrução sem efeitos além de parar até uma interrupção. |
| `arch/x86_64/src/cpu.rs:236` | bloco | apenas mascara interrupções. |
| `arch/x86_64/src/cpu.rs:244` | fn | # Safety Uma IDT válida deve estar carregada. |
| `arch/x86_64/src/cpu.rs:246` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:254` | bloco | pushfq/pop usa a pilha de forma balanceada. |
| `arch/x86_64/src/cpu.rs:271` | bloco | estavam habilitadas antes, logo a IDT é válida. |
| `arch/x86_64/src/cpu.rs:281` | bloco | leitura de registrador. |
| `arch/x86_64/src/cpu.rs:290` | bloco | leitura de registrador. |
| `arch/x86_64/src/cpu.rs:299` | fn | # Safety Código que usa `gs:` passa a ler a partir de `base`. |
| `arch/x86_64/src/cpu.rs:301` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:308` | bloco | leitura de MSR sempre válida em modo longo. |
| `arch/x86_64/src/cpu.rs:316` | bloco | leitura relativa a GS; o chamador garante base configurada. |
| `arch/x86_64/src/cpu.rs:325` | bloco | leitura de registrador de segmento. |
| `arch/x86_64/src/cpu.rs:334` | bloco | rdtsc não tem efeitos colaterais. |
| `arch/x86_64/src/cpu.rs:388` | fn | # Safety Deve ser chamado antes de instalar tabelas com o bit NX. |
| `arch/x86_64/src/cpu.rs:393` | bloco | EFER existe em qualquer CPU de 64 bits; só ligamos NXE. |
| `arch/x86_64/src/cpu.rs:405` | bloco | leitura de EFER. |
| `arch/x86_64/src/cpu.rs:412` | fn | # Safety Escritas em páginas somente-leitura passam a falhar em ring 0. |
| `arch/x86_64/src/cpu.rs:414` | bloco | contrato da função. |
| `arch/x86_64/src/cpu.rs:422` | bloco | bits arquiteturais padrão do x86_64; idempotente. |
| `arch/x86_64/src/cpu.rs:454` | bloco | área de 512 B alinhada a 16 por construção; CR4.OSFXSR ligado no boot. |
| `arch/x86_64/src/cpu.rs:462` | bloco | área válida escrita por `fxsave` ou `FxArea::new`; CR4.OSFXSR ligado. |
| `arch/x86_64/src/gdt.rs:145` | fn | # Safety A tabela deve conter código em 0x08 e dados em 0x10 e viver para sempre. |
| `arch/x86_64/src/gdt.rs:148` | bloco | contrato da função; `retfq` recarrega CS com o seletor 0x08. |
| `arch/x86_64/src/gdt.rs:182` | fn | # Safety `selector` deve apontar para um descritor de TSS válido na GDT ativa. |
| `arch/x86_64/src/gdt.rs:184` | bloco | contrato da função. |
| `arch/x86_64/src/idt.rs:84` | fn | # Safety Todos os handlers presentes devem ser stubs válidos. |
| `arch/x86_64/src/idt.rs:87` | bloco | contrato da função. |
| `arch/x86_64/src/ioapic.rs:18` | impl | acesso serializado pelo chamador (lock no kernel). |
| `arch/x86_64/src/ioapic.rs:20` | impl | idem. |
| `arch/x86_64/src/ioapic.rs:42` | fn | # Safety `virt_base` deve mapear os registradores do I/O APIC sem cache. |
| `arch/x86_64/src/ioapic.rs:51` | bloco | registradores dentro da página mapeada. |
| `arch/x86_64/src/ioapic.rs:59` | bloco | idem. |
| `arch/x86_64/src/paging.rs:240` | fn | # Safety `root` deve apontar para uma PML4 válida (ou zerada) e `translate` deve dar acesso a toda memória física usada pelas tabelas. |
| `arch/x86_64/src/paging.rs:253` | bloco | `index < 512` mantém o ponteiro dentro da tabela de 4 KiB. |
| `arch/x86_64/src/paging.rs:259` | bloco | tabela válida por invariante do Mapper; leitura volátil evita que o compilador fund leituras de memória que a CPU também altera (A/D). |
| `arch/x86_64/src/paging.rs:264` | bloco | idem; escrita alinhada em entrada da tabela. |
| `arch/x86_64/src/paging.rs:276` | fn | # Safety Alterar entradas de topo pode desmapear código em execução. |
| `arch/x86_64/src/paging.rs:307` | bloco | quadro recém-alocado, exclusivo, de 4 KiB. |
| `arch/x86_64/src/paging.rs:514` | bloco | quadro zerado pela arena. |
| `arch/x86_64/src/paging.rs:585` | bloco | teste em arena. |
| `arch/x86_64/src/paging.rs:591` | bloco | idem. |
| `arch/x86_64/src/pci.rs:25` | fn | # Safety Acesso a portas de E/S; deve ser serializado pelo chamador. |
| `arch/x86_64/src/pci.rs:27` | bloco | contrato da função. |
| `arch/x86_64/src/pci.rs:37` | fn | # Safety Pode reprogramar o dispositivo; deve ser serializado pelo chamador. |
| `arch/x86_64/src/pci.rs:39` | bloco | contrato da função. |
| `arch/x86_64/src/pci.rs:49` | fn | # Safety Ver [`config_read32`]. |
| `arch/x86_64/src/pci.rs:51` | bloco | contrato da função. |
| `arch/x86_64/src/pci.rs:59` | fn | # Safety Ver [`config_read32`]. |
| `arch/x86_64/src/pci.rs:61` | bloco | contrato da função. |
| `arch/x86_64/src/pic.rs:18` | fn | # Safety Escreve na porta 0x80 (no-op de temporização): sem outros efeitos. |
| `arch/x86_64/src/pic.rs:20` | bloco | escrever na porta 0x80 é um no-op de temporização. |
| `arch/x86_64/src/pic.rs:27` | fn | # Safety Reprograma o controlador de interrupções. |
| `arch/x86_64/src/pic.rs:29` | bloco | sequência ICW1..ICW4 documentada. |
| `arch/x86_64/src/pic.rs:55` | fn | # Safety Habilitar IRQs exige handlers instalados. |
| `arch/x86_64/src/pic.rs:57` | bloco | contrato da função. |
| `arch/x86_64/src/pic.rs:66` | bloco | leitura das portas de dados. |
| `arch/x86_64/src/pic.rs:73` | fn | # Safety Deve ser chamado exatamente uma vez por interrupção atendida. |
| `arch/x86_64/src/pic.rs:75` | bloco | comando EOI não específico. |
| `arch/x86_64/src/pic.rs:87` | fn | # Safety Desliga entrega de IRQs legadas. |
| `arch/x86_64/src/pic.rs:89` | bloco | contrato da função. |
| `arch/x86_64/src/pit.rs:28` | fn | # Safety Altera o hardware de temporização da plataforma. |
| `arch/x86_64/src/pit.rs:32` | bloco | sequência documentada do PIT: comando, byte baixo, byte alto. |
| `arch/x86_64/src/pit.rs:45` | fn | # Safety Programa o PIT e a porta 0x61 (também controla o alto-falante). |
| `arch/x86_64/src/pit.rs:48` | bloco | sequência documentada; alto-falante fica desligado (bit 1 = 0). |
| `arch/x86_64/src/pit.rs:63` | bloco | latch + duas leituras do canal 2. |
| `arch/x86_64/src/pit.rs:76` | fn | # Safety Altera a porta 0x61. |
| `arch/x86_64/src/pit.rs:79` | bloco | apenas limpa os bits de gate/alto-falante. |
| `arch/x86_64/src/pit.rs:87` | fn | # Safety Reprograma o PIT. |
| `arch/x86_64/src/pit.rs:90` | bloco | sequência documentada do PIT. |
| `arch/x86_64/src/qemu.rs:22` | bloco | a porta 0xf4 só tem efeito se o dispositivo de debug existir. |
| `arch/x86_64/src/rtc.rs:13` | bloco | portas CMOS padrão do PC; leitura sem efeitos além da seleção de índice. |
| `arch/x86_64/src/serial.rs:28` | fn | # Safety Programa o hardware serial da plataforma. |
| `arch/x86_64/src/serial.rs:31` | bloco | sequência documentada do 16550. |
| `arch/x86_64/src/serial.rs:50` | bloco | leitura do LSR. |
| `arch/x86_64/src/serial.rs:64` | bloco | escrita no THR. |
| `arch/x86_64/src/serial.rs:80` | bloco | leitura do LSR/RBR. |
| `arch/x86_64/src/smp.rs:117` | extern | símbolos do global_asm! do trampolim; só endereços/tamanhos são usados. |
| `arch/x86_64/src/smp.rs:142` | bloco | símbolos delimitam um bloco contíguo e somente leitura definido acima. |
| `arch/x86_64/src/smp.rs:166` | fn | # Safety `dest` deve apontar para uma página gravável exclusiva do trampolim. |
| `arch/x86_64/src/smp.rs:170` | bloco | contrato da função; a imagem cabe em uma página (verificado pelo chamador). |
| `arch/x86_64/src/syscall.rs:87` | extern | implementada no global_asm! acima; só o ENDEREÇO é usado (LSTAR). |
| `arch/x86_64/src/syscall.rs:95` | fn | # Safety Os dados por CPU devem estar em `GS_BASE` com o layout descrito no módulo. |
| `arch/x86_64/src/syscall.rs:98` | bloco | contrato da função; MSRs existem em qualquer CPU x86_64. |
| `arch/x86_64/src/syscall.rs:114` | fn | # Safety `entry`/`user_sp` devem estar mapeados com `USER` no espaço atual; `gs:[8]` deve conter o topo da pilha de kernel desta thread. |
| `arch/x86_64/src/syscall.rs:117` | bloco | contrato da função; `swapgs` deixa GS_BASE = 0 para o usuário e KERNEL_GS_BASE = dados por CPU. |
| `arch/x86_64/src/trap.rs:145` | extern | tabela gerada pelo global_asm! acima com exatamente 256 entradas. |
| `arch/x86_64/src/trap.rs:152` | bloco | tabela definida no assembly acima, somente leitura. |
| `arch/x86_64/src/trap.rs:173` | bloco | `h` foi armazenado a partir de um `fn(&mut TrapFrame)` válido; `frame` aponta para o frame empilhado pelo stub e vive até o `iretq`. |

## `nexo-sys` — 56 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `sdk/nexo-sys/src/lib.rs:14` | fn | # Safety O kernel valida ponteiros, mas argumentos incoerentes podem encerrar o processo. |
| `sdk/nexo-sys/src/lib.rs:18` | bloco | convenção da ABI v0; `rcx`/`r11` são destruídos por `syscall`. |
| `sdk/nexo-sys/src/lib.rs:39` | fn | # Safety Ver [`raw`]. |
| `sdk/nexo-sys/src/lib.rs:43` | bloco | convenção da ABI v0 (a3 em r10, a4 em r8). |
| `sdk/nexo-sys/src/lib.rs:66` | fn | # Safety Nunca é perigosa: não invoca nada. |
| `sdk/nexo-sys/src/lib.rs:76` | fn | # Safety Nunca é perigosa: não invoca nada. |
| `sdk/nexo-sys/src/lib.rs:83` | bloco | syscall sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:94` | bloco | ponteiro e tamanho vêm de um `&str` válido. |
| `sdk/nexo-sys/src/lib.rs:100` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:106` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:114` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:122` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:128` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:136` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:156` | bloco | sem ponteiros de usuário além do handle (validado pelo kernel). |
| `sdk/nexo-sys/src/lib.rs:162` | bloco | `out` é memória nossa; o kernel copia no máximo out.len() entradas de 16 B. |
| `sdk/nexo-sys/src/lib.rs:178` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:187` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:193` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:204` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:213` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:219` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:225` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:236` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:247` | bloco | ponteiros e tamanhos vêm de slices válidas. |
| `sdk/nexo-sys/src/lib.rs:267` | bloco | ponteiros e capacidades vêm de slices válidas e mutáveis. |
| `sdk/nexo-sys/src/lib.rs:288` | bloco | ponteiros e tamanhos vêm de slices válidas. |
| `sdk/nexo-sys/src/lib.rs:305` | bloco | ponteiros e tamanhos vêm de slices válidas. |
| `sdk/nexo-sys/src/lib.rs:321` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:328` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:347` | bloco | buffer válido e mutável; o kernel escreve no máximo `out.len()` entradas. |
| `sdk/nexo-sys/src/lib.rs:361` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:368` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:384` | bloco | sem ponteiros de usuário. |
| `sdk/nexo-sys/src/lib.rs:392` | bloco | `b` é uma estrutura válida e mutável. |
| `sdk/nexo-sys/src/lib.rs:407` | bloco | `i` é uma estrutura válida e mutável. |
| `sdk/nexo-sys/src/lib.rs:425` | bloco | ponteiros e capacidades vêm de slices válidas e mutáveis. |
| `sdk/nexo-sys/src/lib.rs:445` | bloco | ponteiro e tamanho vêm de uma slice válida. |
| `sdk/nexo-sys/src/lib.rs:461` | bloco | ponteiro e tamanho vêm de uma slice válida. |
| `sdk/nexo-sys/src/lib.rs:477` | bloco | syscall sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:483` | bloco | syscall sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:490` | bloco | syscall sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:496` | bloco | syscall sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:502` | bloco | syscall sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:509` | bloco | ponteiro e capacidade vêm de uma slice válida. |
| `sdk/nexo-sys/src/lib.rs:524` | bloco | syscall sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:531` | bloco | syscall sem ponteiros de dados; a entrada é um endereço de código válido. |
| `sdk/nexo-sys/src/lib.rs:545` | bloco | syscall sem ponteiros; nunca retorna. |
| `sdk/nexo-sys/src/lib.rs:556` | bloco | syscall sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:562` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:569` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:577` | bloco | sem ponteiros; o kernel valida a faixa. |
| `sdk/nexo-sys/src/lib.rs:586` | bloco | ponteiro para uma struct local válida do tamanho que o kernel escreve. |
| `sdk/nexo-sys/src/lib.rs:594` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:601` | bloco | sem ponteiros. |
| `sdk/nexo-sys/src/lib.rs:608` | bloco | sem ponteiros. |

## `nexo-heap` — 30 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `kernel/lib/heap/src/lib.rs:65` | impl | o Heap só é usado atrás de um lock; os ponteiros internos apontam para memória de sua propriedade exclusiva. |
| `kernel/lib/heap/src/lib.rs:94` | fn | # Safety A região deve ser válida, exclusiva do heap e permanecer mapeada. |
| `kernel/lib/heap/src/lib.rs:102` | bloco | região exclusiva; inserimos como bloco livre. |
| `kernel/lib/heap/src/lib.rs:119` | bloco | nós da lista são blocos livres válidos de nossa propriedade. |
| `kernel/lib/heap/src/lib.rs:147` | bloco | `alloc_end..block_end` está dentro do bloco livre. |
| `kernel/lib/heap/src/lib.rs:155` | bloco | o nó atual continua no mesmo endereço, só encolhe. |
| `kernel/lib/heap/src/lib.rs:165` | bloco | `p` é um nó válido da lista. |
| `kernel/lib/heap/src/lib.rs:173` | bloco | cabeçalho fica dentro do bloco alocado. |
| `kernel/lib/heap/src/lib.rs:185` | bloco | ptr != 0 (dentro de um bloco válido). |
| `kernel/lib/heap/src/lib.rs:198` | fn | # Safety `ptr` deve ter sido devolvido por este heap e não ter sido liberado. |
| `kernel/lib/heap/src/lib.rs:201` | bloco | por contrato, o cabeçalho existe antes de `ptr`. |
| `kernel/lib/heap/src/lib.rs:215` | bloco | bloco era nosso e está fora de uso. |
| `kernel/lib/heap/src/lib.rs:222` | fn | # Safety `start..start+size` deve ser memória do heap fora de uso (sem aliases vivos). |
| `kernel/lib/heap/src/lib.rs:231` | bloco | nó válido. |
| `kernel/lib/heap/src/lib.rs:239` | bloco | nó válido. |
| `kernel/lib/heap/src/lib.rs:249` | bloco | nó válido. |
| `kernel/lib/heap/src/lib.rs:253` | bloco | reescreve o nó anterior no lugar. |
| `kernel/lib/heap/src/lib.rs:258` | bloco | `start` é memória livre de nossa propriedade. |
| `kernel/lib/heap/src/lib.rs:265` | bloco | nó válido. |
| `kernel/lib/heap/src/lib.rs:277` | bloco | nó válido. |
| `kernel/lib/heap/src/lib.rs:307` | bloco | buffer vivo enquanto Arena existir. |
| `kernel/lib/heap/src/lib.rs:326` | bloco | ponteiros válidos deste heap. |
| `kernel/lib/heap/src/lib.rs:344` | bloco | ponteiro válido. |
| `kernel/lib/heap/src/lib.rs:361` | bloco | ponteiros válidos. |
| `kernel/lib/heap/src/lib.rs:376` | bloco | `extra` vive até o fim do teste. |
| `kernel/lib/heap/src/lib.rs:388` | bloco | primeira liberação válida; a segunda deve ser detectada. |
| `kernel/lib/heap/src/lib.rs:413` | bloco | bloco recém-alocado. |
| `kernel/lib/heap/src/lib.rs:421` | bloco | bloco vivo. |
| `kernel/lib/heap/src/lib.rs:424` | bloco | bloco vivo. |
| `kernel/lib/heap/src/lib.rs:429` | bloco | bloco vivo. |

## `nexo-utest` — 26 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/utest/src/main.rs:30` | bloco | deliberadamente inválido: o kernel deve encerrar este processo. |
| `services/utest/src/main.rs:36` | bloco | número inválido; o kernel responde com NotSupported. |
| `services/utest/src/main.rs:42` | bloco | `cli` em ring 3 gera #GP; o kernel deve encerrar este processo. |
| `services/utest/src/main.rs:48` | bloco | deliberadamente inválido: página somente leitura. |
| `services/utest/src/main.rs:253` | bloco | o kernel valida o intervalo antes de ler. |
| `services/utest/src/main.rs:381` | bloco | o kernel deve validar tudo; e o objetivo do teste. |
| `services/utest/src/main.rs:1797` | bloco | unico acesso, processo de uma so thread; buffer estatico (64 KiB nao cabem na pilha). |
| `services/utest/src/main.rs:2070` | bloco | leitura dentro do buffer f1 da saida mapeada (w*h*4 bytes). |
| `services/utest/src/main.rs:2658` | bloco | base .. base+elf_len esta dentro do MemoryObject mapeado. |
| `services/utest/src/main.rs:2661` | bloco | utest tem uma unica thread; buffer estatico evita estourar a pilha. |
| `services/utest/src/main.rs:2808` | bloco | base .. base+elf_len esta dentro do MemoryObject mapeado. |
| `services/utest/src/main.rs:2818` | bloco | utest tem uma unica thread; buffers estaticos evitam estourar a pilha. |
| `services/utest/src/main.rs:3000` | bloco | base .. base+elf_len esta dentro do MemoryObject mapeado. |
| `services/utest/src/main.rs:3011` | bloco | utest tem uma unica thread; os buffers estaticos evitam estourar a pilha. |
| `services/utest/src/main.rs:3059` | bloco | base .. base+elf_len esta dentro do MemoryObject mapeado (USER\|RW). |
| `services/utest/src/main.rs:4531` | bloco | utest tem uma unica thread; buffer estatico evita estourar a pilha. |
| `services/utest/src/main.rs:4533` | bloco | idem — unico acesso a IDXNET neste processo de uma so thread. |
| `services/utest/src/main.rs:5798` | bloco | a regiao acabou de ser mapeada; se o desmapeador chegar antes da syscall, ela falha com BadAddress — que e justamente um dos desfechos que o teste exercita. |
| `services/utest/src/main.rs:6057` | bloco | base .. base+4096 foi mapeada por memory_map (USER\|RW) neste processo. |
| `services/utest/src/main.rs:6069` | bloco | leitura da mesma pagina mapeada. |
| `services/utest/src/main.rs:8667` | bloco | base .. base + w*h*4 foi mapeada por memory_map (USER\|RW) neste processo. |
| `services/utest/src/main.rs:9042` | bloco | base .. base+w*h*4 foi mapeada por memory_map (USER\|RW) neste processo. |
| `services/utest/src/main.rs:9057` | bloco | `base` e o inicio do mapeamento da saida (pagina de cabecalho) e `off` e um dos offsets `frame::OFF_*`, alinhado a 4 e dentro da pagina. |
| `services/utest/src/main.rs:9074` | bloco | leitura dentro do buffer da frente da saida mapeada (w*h*4 bytes). |
| `services/utest/src/main.rs:9101` | bloco | base foi mapeada por memory_map; confere o marcador do produtor. |
| `services/utest/src/main.rs:9109` | bloco | mesma pagina compartilhada. |

## `nexo-loader` — 15 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `boot/loader/src/main.rs:100` | bloco | páginas recém-alocadas, identidade-mapeadas, de nossa propriedade. |
| `boot/loader/src/main.rs:222` | bloco | GetProtocol não toma posse exclusiva; o console do firmware pode continuar desenhando até ExitBootServices — aceitável para o loader. |
| `boot/loader/src/main.rs:268` | bloco | dentro do framebuffer linear (stride * height * 4 <= size). |
| `boot/loader/src/main.rs:349` | bloco | destino tem `pages` páginas zeradas; deslocamento dentro do segmento. |
| `boot/loader/src/main.rs:393` | bloco | destino tem `pages` páginas zeradas. |
| `boot/loader/src/main.rs:401` | fn | # Safety As tabelas devem mapear este código (alias identidade), a pilha e o kernel. |
| `boot/loader/src/main.rs:403` | bloco | contrato da função; registradores explícitos evitam conflitos. |
| `boot/loader/src/main.rs:421` | bloco | COM1 é a porta serial padrão do PC; inicializar é idempotente. |
| `boot/loader/src/main.rs:482` | bloco | `root` é uma página zerada; antes de ExitBootServices o mapeamento é identidade. |
| `boot/loader/src/main.rs:484` | bloco | idem. |
| `boot/loader/src/main.rs:583` | bloco | nenhum handle/protocolo de boot services é usado depois deste ponto. |
| `boot/loader/src/main.rs:593` | bloco | `regions` tem capacidade MAX_MEMORY_REGIONS. |
| `boot/loader/src/main.rs:602` | bloco | página de BootInfo alocada e zerada acima. |
| `boot/loader/src/main.rs:605` | bloco | CPU suporta NX (verificado); tabelas usam o bit NX. |
| `boot/loader/src/main.rs:615` | bloco | PML4 mapeia physmap (+ alias identidade), kernel e pilha. |

## `nexo-nvmedev` — 12 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/nvmedev/src/main.rs:43` | bloco | off fica dentro do BAR0 mapeado por mmio_map; acesso volátil alinhado. |
| `services/nvmedev/src/main.rs:47` | bloco | idem; escrita volátil alinhada. |
| `services/nvmedev/src/main.rs:97` | bloco | página de DMA exclusiva da SQ; slot < QD. |
| `services/nvmedev/src/main.rs:112` | bloco | página de DMA exclusiva da CQ; leitura volátil da entrada corrente. |
| `services/nvmedev/src/main.rs:134` | bloco | página de DMA exclusiva da SQ; slot < QD. |
| `services/nvmedev/src/main.rs:148` | bloco | página de DMA exclusiva da CQ; leitura volátil da entrada corrente. |
| `services/nvmedev/src/main.rs:151` | bloco | mesma entrada; o dispositivo já a preencheu (fase confere). |
| `services/nvmedev/src/main.rs:299` | bloco | página de DMA exclusiva preenchida pelo identify. |
| `services/nvmedev/src/main.rs:311` | bloco | página de DMA exclusiva preenchida pelo identify namespace. |
| `services/nvmedev/src/main.rs:320` | bloco | tabela LBAF dentro da mesma página do identify. |
| `services/nvmedev/src/main.rs:431` | bloco | pagina de DMA exclusiva do slot; bytes <= 3584. |
| `services/nvmedev/src/main.rs:597` | bloco | pagina de DMA exclusiva do slot; bytes <= 3584. |

## `nexo-virtio` — 10 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `libraries/virtio/src/lib.rs:141` | bloco | `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia BARs enumerados); acesso volátil e alinhado. |
| `libraries/virtio/src/lib.rs:147` | bloco | `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia BARs enumerados); acesso volátil e alinhado. |
| `libraries/virtio/src/lib.rs:153` | bloco | `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia BARs enumerados); acesso volátil e alinhado. |
| `libraries/virtio/src/lib.rs:159` | bloco | `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia BARs enumerados); acesso volátil e alinhado. |
| `libraries/virtio/src/lib.rs:165` | bloco | `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia BARs enumerados); acesso volátil e alinhado. |
| `libraries/virtio/src/lib.rs:171` | bloco | `self.0 + off` fica dentro do BAR mapeado com `mmio_map` (o kernel só mapeia BARs enumerados); acesso volátil e alinhado. |
| `libraries/virtio/src/lib.rs:425` | bloco | página de DMA exclusiva da fila; `i < size ≤ 256` → dentro dos 4 KiB. |
| `libraries/virtio/src/lib.rs:437` | bloco | página de DMA exclusiva; slot dentro de `4 + 2*size ≤ 516` bytes. |
| `libraries/virtio/src/lib.rs:450` | bloco | página de DMA exclusiva; offset 2 dentro da página. |
| `libraries/virtio/src/lib.rs:462` | bloco | página de DMA exclusiva; elemento dentro de `4 + 8*size ≤ 2052` bytes. |

## `nexo-sync` — 9 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `kernel/lib/sync/src/lib.rs:19` | impl | o acesso a `data` é serializado pelo flag `locked`. |
| `kernel/lib/sync/src/lib.rs:21` | impl | mover o lock move o dado junto; nada é compartilhado sem o lock. |
| `kernel/lib/sync/src/lib.rs:79` | fn | # Safety O chamador garante que nenhum guard vivo continuará a usar o dado. |
| `kernel/lib/sync/src/lib.rs:99` | bloco | o guard existe apenas enquanto o lock está adquirido. |
| `kernel/lib/sync/src/lib.rs:106` | bloco | idem; acesso exclusivo garantido pelo lock. |
| `kernel/lib/sync/src/lib.rs:128` | impl | escrita ocorre uma única vez, protegida pelo estado atômico; leituras só acontecem após `ONCE_READY` (Acquire). |
| `kernel/lib/sync/src/lib.rs:130` | impl | mover a célula move o valor junto. |
| `kernel/lib/sync/src/lib.rs:151` | bloco | somos o único escritor (estado BUSY) e ninguém lê antes de READY. |
| `kernel/lib/sync/src/lib.rs:174` | bloco | após READY o valor nunca mais é escrito. |

## `nexo-blockdev` — 8 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/blockdev/src/main.rs:177` | bloco | página de DMA exclusiva; 20 bytes. |
| `services/blockdev/src/main.rs:202` | bloco | páginas de DMA exclusivas do slot. |
| `services/blockdev/src/main.rs:227` | bloco | página de DMA exclusiva do slot. |
| `services/blockdev/src/main.rs:234` | bloco | páginas de DMA exclusivas deste driver. |
| `services/blockdev/src/main.rs:268` | bloco | página de DMA exclusiva. |
| `services/blockdev/src/main.rs:341` | bloco | página de DMA exclusiva do slot; `bytes <= 3584`. |
| `services/blockdev/src/main.rs:417` | bloco | deliberadamente inválido — o kernel encerra este processo. |
| `services/blockdev/src/main.rs:504` | bloco | página de DMA exclusiva do slot; `bytes <= 3584`. |

## `nexo-ahcidev` — 6 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/ahcidev/src/main.rs:49` | bloco | off dentro do ABAR mapeado por mmio_map; acesso volátil alinhado. |
| `services/ahcidev/src/main.rs:53` | bloco | idem; escrita volátil alinhada. |
| `services/ahcidev/src/main.rs:102` | bloco | página de DMA exclusiva; layout do FIS de 20 bytes + zeros. |
| `services/ahcidev/src/main.rs:233` | bloco | página de dados preenchida pelo IDENTIFY. |
| `services/ahcidev/src/main.rs:293` | bloco | página de DMA exclusiva; bytes <= 3584. |
| `services/ahcidev/src/main.rs:314` | bloco | página de DMA exclusiva; bytes <= 3584. |

## `nexo-wmd` — 3 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/wm/src/main.rs:65` | bloco | `base` é o início do mapeamento (página de cabeçalho, USER\|RW) e `off` é um dos offsets de `frame::OFF_*`, alinhado a 4 e dentro da página. |
| `services/wm/src/main.rs:211` | bloco | `base..base+len` foi mapeado por `memory_map` neste processo (USER\|RW). |
| `services/wm/src/main.rs:215` | bloco | idem; único mapeamento mutável no wm para a saída. |

## `nexo-consoledev` — 2 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/consoledev/src/main.rs:162` | bloco | pagina de DMA exclusiva; len <= 4096 e cabe em resp.data. |
| `services/consoledev/src/main.rs:185` | bloco | pagina de DMA exclusiva; len <= 4096. |

## `nexo-editor` — 2 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/editor/src/main.rs:185` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |
| `services/editor/src/main.rs:237` | bloco | unico acesso, processo de uma so thread. |

## `nexo-lanc` — 2 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/lanc/src/main.rs:183` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |
| `services/lanc/src/main.rs:252` | bloco | unico acesso, processo de uma so thread. |

## `nexo-netdev` — 2 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/netdev/src/main.rs:175` | bloco | pagina de DMA exclusiva; flen <= FRAME_MAX. |
| `services/netdev/src/main.rs:264` | bloco | pagina de DMA exclusiva; NET_HDR + frame.len() <= 4096. |

## `nexo-shellui` — 2 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/shellui/src/main.rs:118` | bloco | base .. base+BAR_W*BAR_H*4 foi mapeada por memory_map (USER\|RW) neste processo. |
| `services/shellui/src/main.rs:237` | bloco | base .. base+PANEL_W*PANEL_H*4 foi mapeada por memory_map neste processo. |

## `nexo-vfs` — 2 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/vfs/src/main.rs:76` | bloco | processo com uma única thread; nenhuma reentrância. |
| `services/vfs/src/main.rs:93` | bloco | processo com uma única thread. |

## `nexo-visor` — 2 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/visor/src/main.rs:116` | bloco | unico acesso a IMG_BUF neste processo de uma so thread. |
| `services/visor/src/main.rs:187` | bloco | base .. base+w*h*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-agenda` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/agenda/src/main.rs:94` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-arquivos` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/arquivos/src/main.rs:97` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-backup` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/backup/src/main.rs:243` | bloco | unico acesso; processo de uma so thread (buffer estatico para arquivos maiores; a recursao nao o usa em dois niveis ao mesmo tempo — copia um arquivo por vez, sempre por inteiro, antes de descer ou seguir adiante). |

## `nexo-boot-abi` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `abi/boot/src/lib.rs:486` | extern | SAFETY (contrato): chamar exatamente uma vez, com um `BootInfo` válido e mapeado, pilha própria e paginação do kernel ativa; nunca retorna. |

## `nexo-calc` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/calc/src/main.rs:37` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-config` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/config/src/main.rs:48` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-echo` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/echo/src/main.rs:63` | bloco | deliberadamente inválido — o kernel encerra este processo. |

## `nexo-greeter` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/greeter/src/main.rs:88` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-inputdev` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/inputdev/src/main.rs:147` | bloco | página de DMA exclusiva; o evento tem 8 bytes dentro da página. |

## `nexo-monitor` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/monitor/src/main.rs:99` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-netd` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/netd/src/main.rs:89` | bloco | processo com uma única thread; nenhuma reentrância. |

## `nexo-portal` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/portal/src/main.rs:122` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-rngdev` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/rngdev/src/main.rs:174` | bloco | página de DMA exclusiva; `len ≤ 1024`. |

## `nexo-term` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/term/src/main.rs:36` | bloco | base .. base+W*H*4 foi mapeada por memory_map (USER\|RW) neste processo. |

## `nexo-upd` — 1 usos

| Local | Forma | Invariante afirmada |
| --- | --- | --- |
| `services/upd/src/main.rs:168` | bloco | unico acesso; processo de uma so thread (palco estatico para os artefatos). |


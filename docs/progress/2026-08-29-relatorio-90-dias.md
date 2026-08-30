# Relatório — Fase 0 e primeiros 90 dias (`0.0.1-boot`)

**Data:** 2026-08-29 · **Host:** macOS 26 / Apple Silicon (QEMU TCG) · **Toolchain:** Rust 1.98.0, QEMU 11.1.1, mtools 4.0.49

## O que foi entregue

- Loader UEFI e kernel próprios em Rust estável; contrato de boot versionado.
- Memória física/virtual, heap, IDT/exceções, timer, tarefas cooperativas, console.
- 15 auto-testes no kernel + 41 testes de host; 4 cenários de QEMU no CI; imagem reproduzível.
- 12 documentos obrigatórios, 14 ADRs, spec de boot, toolchain, testes, inventário de `unsafe`.

## Relatório de memória (QEMU q35, 512 MiB, `-cpu qemu64`)

| Métrica | Valor |
|---|---|
| Regiões do firmware → normalizadas | 113 → 44 |
| RAM utilizável após o boot | 503 MiB (515 580 KiB) |
| Ocupação do kernel (imagem, tabelas, pilha, BootInfo, cópia do ELF) | 1 680 KiB |
| Reservado (firmware, ACPI, MMIO, runtime) | 12 554 MiB (inclui janela MMIO de 48 GiB em `0xfd00000000`) |
| Quadros gerenciados / livres no fim dos testes | 130 804 / 127 857 |
| Bitmap do alocador | 32 KiB em `0x100000` (2 bitmaps: ocupado + utilizável) |
| Heap mapeado após testes | 4 100 KiB (1 MiB inicial + crescimento sob demanda), 0 OOM |
| Pico de uso do heap | 3 145 920 bytes (teste `heap_grow`) |
| Tabelas de página do loader | 14 quadros (physmap de 4 GiB em 2 MiB + kernel + pilha + identidade) |
| Tempo até `NEXO: boot completo` | 74 ms de tick (após `sti`; a inicialização anterior não é cronometrada — TSC calibrado fica para a Fase 1) |
| Exceções tratadas durante os testes | 8 (1 `#BP` + 7 `#PF` sondados) |
| Trocas de contexto | 12 |

Testes de exaustão: no host (`bitmap::tests`, `heap::tests::exhaustion_and_recovery`); no kernel, `frame_alloc` aloca/libera 1000 quadros sem vazamento e rejeita double free e liberação de quadro reservado.

## Como uma falha é diagnosticada hoje

1. `test=fault` → `PAGE FAULT em 0xffffffffd0007000: nao-presente leitura kernel`, RIP simbolizado (`nexo_kernel::selftest::deliberate_fault+0x8`), registradores, CR2/CR3/CR0, backtrace até `kmain` e `_start`.
2. `test=panic` → mensagem, `src/main.rs:linha:coluna`, uptime, tarefa, backtrace.
3. `test=overflow` → `DOUBLE FAULT (rsp=0xffffffff7fdfff08) — provavel estouro de pilha (guard page atingida)` com frames repetidos agrupados.
4. `tools/symbolize build/logs/*.log` adiciona arquivo:linha usando `llvm-symbolizer`.

## O que foi aprendido (registro para o próximo marco)

- **NX no physmap quebra o salto do loader** se o código que executa `mov cr3` estiver mapeado só por um alias do physmap; a solução foi mapear a imagem do loader em identidade (RX, 4 KiB) e o kernel remover `PML4[0]`.
- **lld funde `.text` e `.rodata` num único segmento RX** sem `PHDRS` explícitos; W^X exige declarar os segmentos no linker script.
- **Rust 1.98 usa mangling v0 por padrão** e `legacy` não é aceito no canal estável — foi preciso um demangler v0 próprio para backtraces legíveis.
- **`cargo` descobre `.cargo/config.toml` pelo diretório atual**, não pelo `--manifest-path`; flags por alvo ficaram no config raiz e as ferramentas sempre executam dentro de cada workspace.
- **`mcopy -m` copia o mtime da fonte** e destrói a reprodutibilidade; timestamps fixos + `SOURCE_DATE_EPOCH` resolvem.
- **Janelas MMIO de 64 bits** (PCIe em `0xfd00000000`) faziam o physmap crescer para 1 TiB; só RAM/ACPI dimensionam o physmap.
- Sondas de falta esperada (`probe`) com retomada por RIP tornaram testáveis guard pages, CR0.WP e NX sem crashar o kernel — vale generalizar para *exception tables* na Fase 2.

## Riscos e próximos passos

- Maior risco continua sendo escopo/continuidade (Plano §11). Limitar WIP a `0.1-kernel`.
- Fase 1: APIC/IOAPIC, ACPI (MADT), SMP, threads preemptivas, TSC/HPET, locks IRQ-safe, stress de 24 h em QEMU, testes de concorrência.
- Dívidas registradas: PIC/PIT temporários; console sem rolagem; frames de `LoaderReclaimable` reaproveitados sem distinguir runtime services (ok enquanto não há chamadas UEFI em runtime); 3 quadros de tabela da identidade do loader não são liberados.

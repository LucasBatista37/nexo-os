# Especificação — ABI de boot (versão 2)

**Crate de referência:** `abi/boot` (`nexo-boot-abi`). Testes de layout: `cargo test -p nexo-boot-abi`.
**Produtor:** `boot/loader`. **Consumidor:** `kernel`.

## 1. Arquivos na partição EFI

| Caminho | Conteúdo |
|---|---|
| `\EFI\BOOT\BOOTX64.EFI` | loader (aplicação UEFI PE32+) |
| `\nexo\kernel.elf` | kernel ELF64 estático (`ET_EXEC`, `EM_X86_64`), segmentos `PT_LOAD` alinhados a 4 KiB, sem segmento W+X, `p_vaddr ≥ 0xffff_ffff_8000_0000` |
| `\nexo\boot.cfg` | texto UTF-8; a primeira linha não vazia e não iniciada por `#` é a linha de comando (≤ 256 bytes) |
| `\nexo\init.elf` | (opcional, v2) initrd: por ora um único ELF64 estático de usuário (`services/init`) |

## 2. Estado da máquina na entrada do kernel

- Modo longo de 64 bits, ring 0, **interrupções desabilitadas**, paginação ativa com a PML4 descrita em `BootInfo::page_table_root`.
- `RDI` = endereço **virtual** (no physmap) de `BootInfo`. `RSP` = `KERNEL_STACK_TOP` (alinhado a 16). `RBP` = 0.
- `EFER.NXE = 1`. `CR0.WP` indefinido (o kernel liga). GDT/IDT/TSS são os do firmware — o kernel deve instalar os seus antes de `sti`.
- Boot services UEFI **encerrados**; runtime services não são chamados pelo loader e o kernel não deve usá-los nesta versão.
- Todos os registradores de propósito geral além de `RDI/RSP/RBP` são indefinidos.

## 3. Espaço de endereçamento construído pelo loader

| Região | Virtual | Físico | Páginas | Flags |
|---|---|---|---|---|
| physmap | `PHYS_MAP_OFFSET + p` para `p ∈ [0, phys_map_size)` | identidade deslocada | 2 MiB | P, W, NX |
| kernel `.text` | conforme ELF | quadros `KernelImage` | 4 KiB | P |
| kernel `.rodata` | conforme ELF | idem | 4 KiB | P, NX |
| kernel `.data/.bss` | conforme ELF | idem | 4 KiB | P, W, NX |
| pilha inicial | `[KERNEL_STACK_BASE, KERNEL_STACK_TOP)` | quadros `KernelStack` | 4 KiB | P, W, NX |
| guard page | `KERNEL_STACK_BASE − 4 KiB` | — | — | não mapeada |
| identidade temporária | imagem do loader | identidade | 4 KiB | P (RX) — **o kernel deve remover `PML4[0]`** |

`phys_map_size` ≥ 4 GiB e cobre toda a RAM reportada, tabelas ACPI e o framebuffer. Janelas MMIO acima disso **não** estão mapeadas.

## 4. `BootInfo` (`#[repr(C)]`, 440 bytes, alinhamento 8)

| Offset | Campo | Tipo | Significado |
|---|---|---|---|
| 0 | `magic` | u64 | `0x544f_4f42_4f58_454e` (`"NEXOBOOT"`) |
| 8 | `version` | u32 | `2` |
| 12 | `size` | u32 | `size_of::<BootInfo>()` do produtor |
| 16 | `memory_map_addr` | u64 | físico do vetor de `MemoryRegion` |
| 24 | `memory_map_len` | u32 | regiões válidas |
| 28 | `memory_map_capacity` | u32 | capacidade (512) |
| 32 | `phys_map_offset` | u64 | = `PHYS_MAP_OFFSET` |
| 40 | `phys_map_size` | u64 | bytes cobertos pelo physmap |
| 48 | `kernel_phys_base` | u64 | menor físico entre os segmentos |
| 56 | `kernel_virt_base` | u64 | = `KERNEL_VIRT_BASE` |
| 64 | `kernel_size` | u64 | soma dos segmentos (arredondados) |
| 72 | `kernel_file_addr` | u64 | físico da cópia do ELF (símbolos) |
| 80 | `kernel_file_len` | u64 | bytes do ELF |
| 88 | `initrd_addr` | u64 | físico do initrd (0 = ausente) — v2 |
| 96 | `initrd_len` | u64 | bytes do initrd — v2 |
| 104 | `stack_base` | u64 | = `KERNEL_STACK_BASE` |
| 112 | `stack_size` | u64 | = `KERNEL_STACK_SIZE` |
| 120 | `page_table_root` | u64 | físico da PML4 ativa |
| 128 | `rsdp_addr` | u64 | físico do RSDP (ACPI 2.0 preferido) ou 0 |
| 136 | `framebuffer` | `FramebufferInfo` (40 B) | ver §5 |
| 176 | `cmdline_len` | u32 | bytes válidos |
| 180 | `reserved` | u32 | 0 |
| 184 | `cmdline` | `[u8; 256]` | UTF-8 sem terminador |

Validação obrigatória no consumidor (`BootInfo::validate`): magic, versão, tamanho, mapa não vazio ≤ capacidade, physmap ≥ 4 GiB em `PHYS_MAP_OFFSET`, cmdline UTF-8.

## 5. `FramebufferInfo` (40 bytes)

`base` (físico, 0 = ausente), `size`, `width`, `height`, `stride` (pixels por linha), `format` (`1` = RGBX8888, `2` = BGRX8888, `0` = desconhecido), `bytes_per_pixel` (4), `reserved`.

## 6. `MemoryRegion` (24 bytes) e `MemoryKind`

`start`, `end` (exclusivo), `kind` (u32), `reserved`. Tipos: 1 Usable, 2 Reserved, 3 AcpiReclaimable, 4 AcpiNvs, 5 Mmio, 6 UefiRuntime, 7 LoaderReclaimable, 8 KernelImage, 9 KernelPageTables, 10 KernelStack, 11 BootInfo, 12 KernelFile, 13 Framebuffer, 14 Initrd. O loader entrega o mapa **cru** (possivelmente desordenado/sobreposto); o kernel normaliza (`nexo_mm::normalize`). Usáveis após o boot: `Usable` e `LoaderReclaimable`. Prioridade em sobreposição: valor de `MemoryKind::priority()`.

## 7. Linha de comando reconhecida pelo kernel `0.0.1-boot`

| Chave | Efeito |
|---|---|
| `loglevel=error|warn|info|debug|trace` | nível do logger serial |
| `selftest=0` | não executa os auto-testes |
| `test=panic|fault|overflow` | dispara o cenário após os testes; implica modo de teste |
| `exit` | encerra o QEMU via `isa-debug-exit` ao final (33 sucesso, 35 falha) |

## 8. Compatibilidade

Mudanças de layout ou semântica exigem `BOOT_ABI_VERSION += 1`, atualização desta página e do teste `layout_is_stable`. O kernel rejeita versões diferentes da sua.

Histórico: v1 (`0.0.1-boot`); v2 (Fase 2) acrescenta `initrd_addr`/`initrd_len` e o tipo `Initrd`.

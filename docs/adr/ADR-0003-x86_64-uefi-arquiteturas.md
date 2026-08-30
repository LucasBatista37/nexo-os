# ADR-0003 — x86_64, UEFI e política de arquiteturas

- **Status:** aceita
- **Data:** 2026-08-29
- **Relacionados:** ADR-0002, `docs/spec/boot-abi.md`

## Contexto

§3.3 escolhe a plataforma inicial. É preciso fixar CPU, firmware, máquina virtual e a regra para novas arquiteturas.

## Decisão

1. **Plataforma 1:** `x86_64` com firmware **UEFI 64 bits**; máquina de desenvolvimento **QEMU `q35`** com edk2/OVMF; CPU modelo `qemu64` (baseline conservadora; exige NX).
2. **Boot:** aplicação UEFI própria (`boot/loader`, target `x86_64-unknown-uefi`) carrega o kernel ELF64 estático (`x86_64-unknown-none`) de `\nexo\kernel.elf`, constrói as tabelas de página iniciais e entrega um `BootInfo` versionado (`abi/boot`, `docs/spec/boot-abi.md`). Não há suporte a BIOS legado nem a bootloaders de terceiros (GRUB/Limine) — a cadeia de boot é parte da arquitetura própria.
3. **Layout de endereços:** kernel em `0xffff_ffff_8000_0000` (code model `kernel`), physmap em `0xffff_8000_0000_0000`, pilha e heap com guard pages (constantes em `abi/boot`).
4. **Novas arquiteturas:** `aarch64` somente após `arch/` ter abstraído tudo que `kernel/` usa e após `0.8`. `riscv64` é opcional e posterior. Nenhuma decisão de kernel pode assumir x86_64 fora de `arch/`.
5. **Hardware real:** um único computador de referência, definido antes da Fase 7; até lá QEMU é a única plataforma suportada.

## Consequências

- O loader precisa lidar com GOP, mapa de memória UEFI e `ExitBootServices` — implementado e testado.
- ACPI/APIC/SMP entram na Fase 1; PIC/PIT são temporários e documentados como tal.

## Alternativas consideradas

- **BIOS/Multiboot**: rejeitado — legado, sem framebuffer moderno nem caminho para Secure Boot.
- **aarch64 primeiro**: rejeitado — menos documentação/emulação uniforme e hardware de teste menos acessível.

## Evidência / verificação

`tools/test-qemu` (boot UEFI em QEMU no CI); `docs/spec/boot-abi.md`; testes de layout em `abi/boot`.

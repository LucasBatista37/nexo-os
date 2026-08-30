# Hardware suportado

## Suporte oficial (release `0.0.1-boot`)

| Plataforma | Estado | Notas |
|---|---|---|
| QEMU `q35`, `-cpu qemu64`, 512 MiB, firmware edk2/OVMF | **suportado e testado no CI** | serial COM1, `isa-debug-exit`, framebuffer GOP 1280×800 BGRX (padrão do OVMF) |
| QEMU com `-cpu host`/KVM (Linux) | esperado funcionar | não testado no CI |
| Hardware real x86_64 UEFI | **não suportado** | exige CPU com NX; PIC/PIT legados; sem drivers de armazenamento/rede |

## Requisitos mínimos do kernel atual

- CPU x86_64 com `NX` (o loader recusa CPUs sem NX).
- Firmware UEFI 64 bits com GOP (opcional) e mapa de memória padrão.
- UART 16550 em `0x3F8` para diagnóstico (opcional; sem ela o log é descartado).
- 64 MiB de RAM (o CI usa 512 MiB).

## Computador de referência

Ainda não escolhido (regra: um único modelo, ver `PROJECT_CHARTER.md`). A escolha e a matriz de regressão serão registradas aqui e em `docs/lab/` antes da Fase 7.

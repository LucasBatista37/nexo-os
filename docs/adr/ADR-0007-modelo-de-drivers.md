# ADR-0007 — Modelo de drivers

- **Status:** aceita (direção); protocolo driver–device manager via RFC na Fase 3
- **Data:** 2026-08-29
- **Relacionados:** ADR-0002, ADR-0004

## Decisão

1. Drivers são **processos de usuário** (hosts de driver) que recebem, via capabilities, apenas `Interrupt`, `DeviceMemory` (MMIO) e buffers DMA autorizados pelo `device-manager`.
2. Descoberta e *binding* por IDs/propriedades (PCI vendor/device/class, ACPI HID, VirtIO device id); um driver declara compatibilidade em manifesto.
3. Ordem de implementação: VirtIO (block, input, RNG, console, net) em QEMU antes de qualquer hardware real; PCI/PCIe e ACPI mínimos no kernel apenas para enumeração.
4. Sem IOMMU o caminho é marcado **explicitamente inseguro** (log e política); com IOMMU, DMA é restrito ao domínio do driver.
5. Um driver que falha é reiniciado pelo `service-manager` sem reiniciar o kernel (gate F3).
6. Nesta release existem apenas "drivers" de plataforma dentro de `arch/x86_64` (UART 16550, PIC, PIT) — temporários e sem estado compartilhado; serão substituídos/isolados nas Fases 1–3.

## Alternativas

Drivers no kernel (rejeitado como padrão: ADR-0002); reutilizar drivers Linux (rejeitado: licença GPL e acoplamento).

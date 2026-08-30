# drivers/

Reservado para drivers isolados (ADR-0007): VirtIO (block, net, input, console, RNG), PCI/PCIe, ACPI, USB, NVMe/AHCI… A partir da Fase 3. Os únicos "drivers" atuais (UART 16550, PIC, PIT) vivem em `arch/x86_64` como código de plataforma temporário.

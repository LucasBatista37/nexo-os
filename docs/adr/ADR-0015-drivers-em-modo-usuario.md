# ADR-0015 — Drivers em processos de usuário com concessões de dispositivo

- **Status:** aceita
- **Data:** 2026-08-30
- **Relacionados:** ADR-0002 (microkernel), ADR-0004 (handles), ADR-0005 (canais)

## Decisão

Drivers de dispositivo rodam como processos de usuário comuns (ELF no initrd, sem privilégio de ring 0) e acessam hardware **apenas** por um handle de *concessão de dispositivo* (`kind` 3) que autoriza: enumeração e espaço de configuração PCI (`pci_enum`, `pci_cfg_read/write`), mapeamento de MMIO restrito a BARs enumerados (`mmio_map`, páginas `USER|RW|NO_CACHE`), páginas de DMA físicas contíguas de 4 KiB zeradas e pertencentes ao processo (`dma_alloc`) e vetores de interrupção MSI/MSI-X (`irq_alloc`/`irq_wait`; vetores 0x50–0x6f, contadores por vetor, o kernel só faz EOI e acorda quem espera). O kernel não contém drivers de dispositivo além de serial, PIC/APIC, PIT/TSC e leitura PCI.

O primeiro driver é `services/blockdev` (VirtIO-block 1.x sobre PCI, capabilities modernas, fila dividida, MSI-X). Seu contrato com clientes é um protocolo cru provisório (`nexo.block` v0, `docs/spec/ipc-compat.md` §5) até a IDL existir.

## Consequências

- Um driver que cai é só um processo morto: o kernel continua; o `svcmgr` pode reiniciá-lo (a fila do dispositivo é reinicializada no reset VirtIO).
- **Caminho sem IOMMU é explicitamente inseguro:** o dispositivo faz DMA para qualquer endereço físico que o driver lhe programar. A concessão total equivale, portanto, a confiança no driver. A abstração de IOMMU (VT-d/AMD-Vi) e concessões por dispositivo (BDF + BARs específicos) são itens abertos da Fase 3.
- Cada concessão é um objeto com handle: pode ser transferida por canal e ter direitos reduzidos (`handle_duplicate`). O gerenciador de dispositivos (`services/devmgr`) recebe a concessão raiz (`ADMIN`), enumera PCI, deriva com `device_open` uma concessão **restrita a cada função** (config, `pci_enum` e BARs limitados àquele BDF) e inicia o driver correspondente por tabela de IDs (`vendor`/tipo VirtIO → programa do initrd), entregando-lhe `[concessão, canal]`. Um driver só enxerga o seu dispositivo; DMA continua sem IOMMU.
- O transporte VirtIO (capabilities, negociação, MSI-X, fila dividida) é a biblioteca `nexo-virtio` (`libraries/virtio`, `no_std`, parser testado no host), usada por `blockdev` e `rngdev`.

## Alternativas

Drivers no kernel (rejeitado: contraria ADR-0002 e o gate F3 — "um driver de armazenamento pode falhar sem corromper o kernel"); acesso a portas de E/S por usuário via IOPL (rejeitado por ora: só MMIO/config PCI são expostos; portas serão um direito separado se necessário).

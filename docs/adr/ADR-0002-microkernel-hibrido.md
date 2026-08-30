# ADR-0002 — Núcleo híbrido orientado a microkernel e limites privilegiados

- **Status:** aceita
- **Data:** 2026-08-29
- **Relacionados:** ADR-0004, ADR-0005, ADR-0007

## Contexto

§3.1 do Plano Mestre define um núcleo mínimo com capacidades e serviços isolados, permitindo *fast paths* privilegiados apenas com medição e threat model.

## Decisão

1. Em modo privilegiado (ring 0) ficam somente: boot/abstração de CPU, exceções e interrupções, memória física/virtual e espaços de endereçamento, threads/escalonador/timers, IPC e objetos do kernel, capabilities, primitivas de DMA/IOMMU e depuração.
2. Em espaço de usuário ficam VFS, rede, áudio, compositor, device manager, drivers isoláveis, sessão/login, instalador/atualizador/pacotes.
3. Um componente só entra no kernel se: (a) uma medição mostrar custo inaceitável no caminho isolado, (b) houver threat model específico e (c) uma ADR registrar a exceção com critérios de saída.
4. O código do kernel é organizado em `kernel/` (núcleo) e `arch/` (HAL); nada em `kernel/` acessa hardware diretamente sem passar por `arch/`.
5. Nesta release (`0.0.1-boot`) o kernel ainda não possui modo usuário; a fronteira é preparada por GDT/TSS, IST, W^X e guard pages já ativos.

## Consequências

- IPC e cópias entre domínios são a principal fonte de custo; batching e memória compartilhada controlada são obrigatórios na Fase 2.
- Cada driver terá processo/domínio próprio; falha de driver não derruba o kernel (gate F3).
- Interfaces internas nascem como protocolos IPC tipados (ADR-0005), não como chamadas de função.

## Alternativas consideradas

- **Monolítico**: mais simples inicialmente; rejeitado por isolamento fraco e superfície privilegiada grande.
- **Microkernel puro (seL4-like)**: máximo isolamento; rejeitado como requisito rígido por custo de desempenho não medido e tempo de uma pessoa — mantido como direção, com exceções controladas.

## Evidência / verificação

Revisão trimestral de ADRs; lista de componentes privilegiados em `ARCHITECTURE.md`; testes de isolamento a partir de `0.2-userspace`.

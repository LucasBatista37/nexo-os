# Painel mensal (Plano Mestre §10)

Preencher a cada 4 semanas. Primeira medição: 2026-08-29.

```text
Release atual:                 0.0.1-boot (main: Fases 1-3 completas; Fase 4 quase completa, so TLS pendente)
Marco ativo:                   0.1-kernel (gate: 24 h de stress — EM EXECUCAO) → 0.2 → 0.3 → 0.4-network
Horas disponiveis/semana:      meta provisoria 10-15 (confirmar)
Itens concluidos:              ~150 [x]/[-] no Plano Mestre; 40 testes de kernel no boot; 109 testes de host; 10 cenarios de QEMU
Itens bloqueados:              0 (TLS adiado por decisao; 4 pendentes de decisao: nome, licenca, horas, PC de referencia)
Cobertura de testes relevante: crates de logica pura com testes + fuzz-lite (parsers de rede + maquina de estados TCP); cortes de energia simulados
Tempo de boot:                 ~2,5 s ate "boot completo" com 40 testes (QEMU TCG); 8+ drivers/servicos em ring 3
RAM ociosa:                    ~500 MiB livres de 512 MiB no fim do boot
Crash mais recente:            nenhum nao intencional; stress de 2 h sem erros; 24 h em execucao
Maior risco atual:             escopo e continuidade (§11); DMA sem IOMMU (ADR-0015); TLS ainda ausente
Decisao necessaria:            confirmar nome/licenca/horas; PC de referencia; retomar TLS quando priorizado
Proxima demonstracao publica:  pilha de rede completa num boot (cenario `net`: DHCP->ARP->ICMP->DNS->TCP/HTTP, IPv6, firewall, POSIX)
```

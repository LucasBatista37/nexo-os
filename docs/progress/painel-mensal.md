# Painel mensal (Plano Mestre §10)

Preencher a cada 4 semanas. Primeira medição: 2026-08-29.

```text
Release atual:                 0.0.1-boot (main: Fases 1–2 completas + Fase 3 com gate F3 atendido)
Marco ativo:                   0.1-kernel (gate: 24 h de stress — EM EXECUÇÃO) → 0.2-userspace → 0.3-storage
Horas disponíveis/semana:      meta provisória 10–15 (confirmar)
Itens concluídos:              145+ [x] no Plano Mestre; 39 testes de kernel no boot; 79 testes de host; 9 cenários de QEMU
Itens bloqueados:              0 (4 pendentes de decisão: nome, licença, horas, PC de referência)
Cobertura de testes relevante: crates de lógica pura com testes + fuzz-lite; kernel: 39 auto-testes; cortes de energia simulados (host e QEMU)
Tempo de boot:                 ~2,5 s até "boot completo" com 39 testes (QEMU TCG); serviços: 7 drivers/servidores em ring 3
RAM ociosa:                    ~500 MiB livres de 512 MiB no fim do boot
Crash mais recente:            nenhum não intencional; stress de 2 h sem erros (2026-08-30); 24 h em execução
Maior risco atual:             escopo e continuidade (§11); DMA sem IOMMU (ADR-0015)
Decisão necessária:            confirmar nome/licença/horas; PC de referência
Próxima demonstração pública:  shell de diagnóstico interativo na console VirtIO (cenário `shell`)
```

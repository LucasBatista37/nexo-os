# Painel mensal (Plano Mestre §10)

Preencher a cada 4 semanas. Primeira medição: 2026-08-29.

```text
Release atual:                 0.0.1-boot
Marco ativo:                   0.1-kernel (Fase 1)
Horas disponíveis/semana:      meta provisória 10–15 (confirmar)
Itens concluídos:              94 [x] no Plano Mestre; 15/15 testes de kernel; 41 testes de host
Itens bloqueados:              0 (4 pendentes de decisão: nome, licença, horas, hospedagem)
Cobertura de testes relevante: crates de lógica pura 100% com testes; kernel: 15 auto-testes + 4 cenários
Tempo de boot:                 74 ms de tick até "boot completo" (QEMU TCG, sem calibração de TSC)
RAM ociosa:                    503 MiB livres de 512 MiB; kernel 1.7 MiB; heap 4 MiB mapeado
Crash mais recente:            nenhum não intencional (cenários panic/fault/overflow são deliberados)
Maior risco atual:             escopo e continuidade (§11)
Decisão necessária:            confirmar nome/licença/horas; escolher hospedagem para CI e release
Próxima demonstração pública:  boot 0.0.1-boot em QEMU (docs/releases/0.0.1-boot.md)
```

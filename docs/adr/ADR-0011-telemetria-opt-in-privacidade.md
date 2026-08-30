# ADR-0011 — Telemetria opt-in e privacidade

- **Status:** aceita
- **Data:** 2026-08-29

## Decisão

1. Nenhum dado sai do computador sem consentimento explícito, por categoria (crash reports, métricas de uso, diagnósticos de rede) e revogável.
2. Crash dumps são protegidos localmente; o envio mostra exatamente o conteúdo e remove dados de usuário por padrão.
3. Métricas do projeto (boot, RAM, crash-free) são calculadas localmente e agregadas somente se enviadas.
4. Identificadores são aleatórios por instalação e rotacionáveis; nunca hardware IDs.
5. Toda coleta tem documentação pública e teste automatizado que garante o "desligado por padrão".

## Alternativas

Telemetria opt-out (rejeitada: contraria §2.1 "o usuário entende o que está acontecendo").

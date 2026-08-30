# Nexo OS — Project Charter

**Nome provisório:** Nexo OS (`nexo-*` nas crates). Substituível a qualquer momento antes de `0.9-beta`; a escolha final passará por verificação de marca e domínio (ADR-0015, futura).
**Status:** Fase 0 concluída — release `0.0.1-boot`.
**Documento de referência:** [PLANO_MESTRE_SISTEMA_OPERACIONAL.md](PLANO_MESTRE_SISTEMA_OPERACIONAL.md).

## Visão

Um sistema operacional de uso geral, construído do zero (kernel, IPC, serviços, compositor, shell e SDK próprios), rápido, compreensível e seguro por padrão: aplicativos recebem apenas as capacidades explicitamente concedidas, serviços são atualizáveis de forma modular e o usuário entende o que acontece com seus dados, dispositivos e tarefas.

## Quem usará a versão 1.0 (uma frase)

Desenvolvedores e entusiastas técnicos que querem um desktop seguro, auditável e previsível em um pequeno conjunto de computadores certificados — não o público geral, não servidores, não dispositivos móveis.

## Promessa da 1.0

- funciona de forma confiável em QEMU e em 1–3 modelos de computador certificados;
- atualizações A/B assinadas com rollback automático;
- aplicativos essenciais (terminal, arquivos, editor, configurações, monitor) e SDK versionado;
- contrato de compatibilidade de ABI 1.x publicado e testado;
- acessibilidade AA nos fluxos essenciais; pt-BR e en-US.

## Não objetivos (até 1.0)

Compatibilidade ampla com Windows/macOS/Linux; suporte a todo hardware; navegador próprio; API de GPU própria; loja de aplicativos competitiva; certificações médicas/automotivas; ABI estável antes de governança; versões móvel/servidor/embarcada simultâneas.

## Plataforma inicial (decidida)

| Item | Escolha |
|---|---|
| Alvo 1 | QEMU `x86_64` / `q35` / UEFI (edk2) |
| Computador de referência futuro | **a definir** — regra: um único modelo com NVMe, Ethernet Intel e GPU com framebuffer UEFI (candidato: mini-PC x86_64 com CPU Intel de 12ª geração ou superior); decisão registrada quando a Fase 7 iniciar |
| Linguagem | Rust `no_std` estável + Assembly mínimo (ADR-0001) |
| Licença | MIT OR Apache-2.0 (ADR-0012) |
| Host de desenvolvimento | macOS 26 (Apple Silicon) + Linux no CI |

## Dedicação semanal sustentável

Registrar em `docs/progress/` o valor real. Meta inicial: **10–15 h/semana**, uma frente principal e uma de manutenção/documentação por vez.

## Regra de avanço

Nenhuma fase começa antes do gate técnico da anterior (Plano Mestre §7.1). Estado dos gates: [docs/CHECKLIST_STATUS.md](docs/CHECKLIST_STATUS.md).

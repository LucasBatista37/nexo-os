# ADR-0009 — Pacotes, manifests e identidade de aplicativos

- **Status:** aceita (direção)
- **Data:** 2026-08-29
- **Relacionados:** ADR-0004, ADR-0010

## Decisão

1. Aplicativo = **pacote imutável e assinado** + manifesto (ID reverso-DNS, versão semântica, entry point, tipos de arquivo, capabilities solicitadas, compatibilidade mínima/máxima).
2. Identidade = ID do manifesto + chave do publicador; a primeira instalação fixa a chave (TOFU) e atualizações exigem a mesma chave ou rotação assinada.
3. Cada app tem diretório privado; acesso externo apenas por seleção do usuário (portais) ou grant persistente revogável.
4. Instalação e remoção são transacionais (estado anterior recuperável).
5. Permissões são explicadas no momento de uso e inspecionáveis por app (§2.3).

## Alternativas

Modelo de pacotes de distribuição Linux (rejeitado: sem isolamento por padrão); apenas web apps (rejeitado: ABI nativa é prioridade, §3.8).

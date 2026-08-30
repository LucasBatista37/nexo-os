# ADR-0013 — Shell gráfico e conceito de Contextos

- **Status:** aceita como direção de produto; validação por protótipos na Fase 5
- **Data:** 2026-08-29

## Decisão

1. O diferencial de experiência é o modelo de **Contextos** persistentes (janelas, documentos, permissões temporárias, notificações e estado), a **Central de Ações** e a **Faixa de Atividades** (§2.3).
2. Nada disso vive no kernel ou no compositor: é um cliente do protocolo de superfícies e pode ser substituído.
3. Compositor e toolkit são próprios, com renderer 2D por software inicialmente; GPU via camadas compatíveis depois.
4. Acessibilidade (teclado completo, árvore semântica, contraste, escala, redução de movimento) é requisito de desenho do toolkit, não adaptação.
5. Nenhuma decisão de UI é congelada sem testes com usuários externos (§11 "Falta de usuários reais").

## Alternativas

Portar Wayland/X11 e um desktop existente (rejeitado: identidade e controle da experiência, §3.3).

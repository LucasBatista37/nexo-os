# Threat model por subsistema (revisão 2026-09-01)

Plano §Fase 8 ("revisar threat model por subsistema"). O formato de cada seção: **ativos** (o
que se protege), **adversário** (de quem), **superfície**, **mitigações existentes** (com os
testes que as provam) e **lacunas conhecidas** (honestas, com o porquê). Convenções: "app" =
processo em ring 3 sem privilégios; "malicioso" = controlado pelo adversário, inclusive com
syscalls arbitrárias (o fuzz de syscalls modela exatamente isso).

Princípio geral do sistema: **negação por omissão via capabilities** — um processo só alcança
o que os handles na sua tabela alcançam; não existem nomes globais ambientais. Todas as
travessias de fronteira (syscall, IPC, protocolos, formatos de arquivo) tratam a entrada como
hostil e validam no parse.

## 1. Kernel / syscalls

- **Ativos**: isolamento entre processos; integridade do próprio kernel.
- **Adversário**: app malicioso com syscalls arbitrárias.
- **Superfície**: 34 syscalls (0–33; `docs/spec/syscall-abi.md`), ponteiros e handles vindos
  do usuário.
- **Mitigações**: ponteiros validados por faixa + bit USER antes de qualquer cópia
  (`copy_from_user`/`copy_to_user`); handles verificados por tipo e **direitos**
  (read/write/transfer/duplicate — `user_syscall_error`, `user_isolation`); limites
  explícitos em toda entrada (`MSG_MAX`, `MEMORY_MAX_PAGES`, `SPAWN_MEM_MAX`…); fuzz de
  syscalls (20 000/rodada no boot + `make fuzz` semanal) sem pânico nem vazamento; pânico
  com backtrace simbolizado para diagnóstico. Instruções privilegiadas em ring 3 →
  exceção → processo morto (`user_isolation`).
- **Lacunas**: sem mitigação de canais laterais (o TSC é legível por qualquer app — inclusive
  via trace, ver §9); auditoria externa nunca feita; uma thread por processo limita a
  superfície hoje, mas a análise terá de ser refeita com threads.

## 2. IPC (canais, handles, coletor)

- **Ativos**: confidencialidade e integridade das mensagens; a própria noção de capability.
- **Adversário**: qualquer par de processos tentando forjar/duplicar/reter capacidades.
- **Superfície**: send/recv/transferência de handles; o coletor de pontas inalcançáveis.
- **Mitigações**: transferir MOVE o handle (sem `RIGHT_TRANSFER` não sai; duplicar exige
  `RIGHT_DUPLICATE` e nunca amplia direitos); mensagens têm dono único; janelas em-trânsito
  protegidas do coletor por `ipc::InFlight` com validação por geração — **estudo de caso**: o
  bug em que o coletor fechava handles em trânsito foi achado pelo CI em SMP carregado e está
  coberto por regressão (`ipc_handoff`); contagem de pontas conferida em todos os testes
  (vazamento = falha).
- **Lacunas**: mensagens/canais sem quota por processo, mas com teto composto finito
  (`HANDLES_MAX` × `CHANNEL_QUEUE_MAX` × `MSG_MAX`); **memória compartilhável tem quota por
  criador desde 2026-09-01** (bloco 37): 4096 páginas (16 MiB) por processo, devolvida quando
  o objeto morre (`shm_quota`, teste com estouro e devolução).

## 3. Memória compartilhada e DMA

- **Ativos**: isolamento físico entre processos e entre dispositivos e o sistema.
- **Adversário**: app com `MemoryObject` compartilhado; driver comprometido com acesso a DMA.
- **Superfície**: `memory_create/map/unmap`; `dma_alloc`; BARs mapeados por concessão.
- **Mitigações**: objetos de memória são capabilities (mapeia quem tem o handle); W^X no
  carregador de ELF; `memory_unmap` limpa PTEs com shootdown de TLB em todas as CPUs.
  Leitores de memória compartilhada tratam o conteúdo como dado hostil e **esperam
  convergência** (lição codificada em `docs/testing.md`).
- **Lacunas (por construção, documentadas)**: **sem IOMMU o DMA de drivers é irrestrito**
  (ADR-0015) — um driver malicioso com concessão de dispositivo lê/escreve física arbitrária;
  a concessão de UMA função PCI limita *quais* dispositivos, não o alcance do DMA. Mitigação
  planejada: IOMMU quando presente. Este é o maior gap aberto do modelo.

## 4. Drivers em modo usuário (blockdev, nvmedev, inputdev, netdev, consoledev, rngdev)

- **Ativos**: o resto do sistema, contra um driver comprometido.
- **Adversário**: driver explorado via dispositivo hostil ou bug próprio.
- **Superfície**: MMIO/filas dos dispositivos; os protocolos que o driver serve.
- **Mitigações**: ring 3 + capabilities (a queda de um driver não derruba o kernel — cenários
  `fault`; o svcmgr reinicia serviços); concessão por função PCI; protocolos tipados com
  validação no parse dos DOIS lados; respostas do dispositivo tratadas como não confiáveis
  (fase de CQE, limites de tamanho).
- **Lacunas**: o DMA do §3; sem rate-limit de interrupções (coalescing existe nos avisos).

## 5. Compositor (wm) e entrada

- **Ativos**: o que está na tela de cada app; o teclado (senhas!); a decisão do usuário.
- **Adversário**: app malicioso com sessão do compositor tentando ler/injetar noutras janelas.
- **Superfície**: protocolo `nexo.wm` (30+ métodos), canal de entrada, saídas compartilhadas.
- **Mitigações**: cada superfície pertence à sessão que a criou (erro 3 para as demais);
  teclado vai SÓ à janela em foco (ou captura — `grab` para entrada sensível, greeter);
  clipboard mediado pela posse da entrada; DnD entrega só à janela sob o cursor (grants);
  `surface_info`/notificações são privilégio do shell (erro 7); o **consentimento** é um
  clique numa janela que o app não controla (`services/lanc`); o portal de arquivos entrega
  conteúdo, nunca capacidade. Tudo coberto por autotestes (`user_wm_*`, `user_consent`,
  `user_portal`).
- **Lacunas**: ~~a saída composta legível por qualquer sessão~~ **fechada em 2026-09-01**
  (bloco 34): `output` é privilégio do shell (erro 7; teste negativo em `user_wm_shell`) e o
  shell exporta a tela ao SEU orquestrador pelo pipe (`"saida"`) quando preciso. Resta: sem
  indicador visual de captura de tela/entrada.

## 6. Armazenamento (NexoFS, fs, vfs)

- **Ativos**: integridade e durabilidade dos dados; isolamento entre consumidores.
- **Adversário**: corte de energia a qualquer momento; imagem de disco corrompida/hostil;
  cliente malicioso do protocolo `nexo.fs`.
- **Superfície**: blocos vindos do disco; pedidos `nexo.fs`.
- **Mitigações**: commits atômicos por setor com testes de corte em cada escrita (host) e
  cenário `powercut` (SIGKILL no QEMU durante escritas, repetido); imagens corrompidas nos
  testes de host do parser; caminhos validados; quem tem o canal do `fs` tem o volume — a
  partição de confiança é o CANAL (o vfs dá namespaces por instância).
- **Lacunas**: **sem criptografia de disco** (Fase 8, depende da decisão de cripto); sem
  quotas (ENOSPC é gracioso — provado em campo — mas um app com canal de fs pode encher o
  volume); um cliente por servidor `fs` (multiplexação é papel do vfs).

## 7. Pacotes, instalação e repositório

- **Ativos**: só rodar o que o usuário decidiu instalar; permissões fiéis ao declarado.
- **Adversário**: pacote malicioso; repositório adulterado.
- **Superfície**: NEXOPKG1 (parse completo), manifesto, `/repo`, lista de revogação.
- **Mitigações**: validação total no parse (CRC, tabela, limites; fuzz-lite host); chaves de
  manifesto desconhecidas são ERRO (nada de campos "extras" passando batido); instalação
  transacional (corte em qualquer ponto preserva a versão corrente — falha injetada em cada
  operação); permissões por negação-por-omissão + consentimento por clique; revogação com
  falha fechada (lista ilegível nega tudo).
- **Lacunas**: **sem assinatura de pacotes** (bloqueada pela decisão de cripto adiada) — o
  repositório local é confiável por localização, não por criptografia; o CRC é integridade
  contra acidente, não contra adversário. Esta é a lacuna nº 1 a fechar quando a decisão de
  TLS/cripto for tomada.

## 8. Rede (netdev, netd, sockets, firewall)

- **Ativos**: o sistema contra a rede; sessões de app contra apps sem permissão de rede.
- **Adversário**: qualquer pacote da rede; app sem a capability tentando falar com a rede.
- **Superfície**: quadros virtio-net; API de sockets.
- **Mitigações**: parsers de pacote com validação hostil; capability de rede por sessão com
  firewall por sessão (testado: TCP permitido/UDP negado na mesma sessão); slirp isola o host
  nos testes.
- **Lacunas**: **sem TLS** (decisão adiada pelo usuário) — nada que fale com a internet real
  deve ser exposto até lá; sem rate-limiting/SYN-flood hardening (o alvo atual é QEMU).

## 9. Observabilidade (trace, debug_info, logs)

- **Ativos**: não vazar segredos de um processo para outro pelos canais de diagnóstico.
- **Adversário**: app curioso.
- **Superfície**: `debug_info` (contadores globais), `SYS_TRACE` (anel global!), logs.
- **Mitigações**: o trace grava só `{tsc, pid, nr}` — sem argumentos, sem dados; logs de app
  são prefixados por pid.
- **Lacunas**: ~~anel de trace legível por qualquer app~~ **fechada em 2026-09-01** (bloco
  35): ligar e ler exigem a capability de **depuração** (`Object::Debug`, kind 5 — o mesmo
  desenho de posse-explícita das concessões de dispositivo), com testes negativos em
  `user_trace`. O TSC em si segue legível (canais laterais, §1).

## 10. Cadeia de boot e atualização

- **Ativos**: integridade do que roda antes do kernel; recuperabilidade.
- **Adversário**: adulteração offline da imagem; atualização interrompida.
- **Mitigações**: imagem reproduzível (verificada no CI); artefatos de release com SHA256SUMS
  gerado pelo build (lição de 2026-09-01); instalação de APPS transacional.
- **Lacunas**: sem Secure Boot / trust root / A/B / rollback (Fase 8 inteira pendente); o
  SHA256SUMS autentica contra acidente, não contra adversário (sem assinatura).

## Política de `unsafe` (auditoria mecânica)

`tools/nexo-unsafe-audit` roda no `make lint`: **todo** uso de `unsafe` (bloco, fn, impl,
extern) exige justificativa `SAFETY:`/`# Safety` adjacente. Estado da revisão de 2026-09-01:
**424 usos, 0 sem justificativa** (inventário por crate em `docs/unsafe-inventory.md`).
`services/` de apps não usam `unsafe` além de mapeamentos de memória compartilhada e buffers
estáticos; `nexo-pkg`/`nexo-inst`/`nexo-img`/`nexo-cal`/`nexo-textgrid` são
`forbid(unsafe_code)` ou zero-unsafe por construção.

## Prioridades decorrentes desta revisão

1. IOMMU quando disponível (§3 — maior gap estrutural; documentado desde ADR-0015).
2. Assinatura de pacotes assim que a decisão de cripto sair (§7).
3. Quota de disco (§6) e refinamento das de IPC (§2 — a de memória compartilhável existe).

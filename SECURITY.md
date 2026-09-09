# Segurança — Nexo OS

## Contato e divulgação

- Relate vulnerabilidades por uma *security advisory* privada em https://github.com/LucasBatista37/nexo-os/security/advisories/new (não abra issue pública). Um endereço `security@` será publicado quando houver domínio.
- Prazo de resposta inicial: 7 dias. Divulgação coordenada: até 90 dias após a confirmação, ou antes se houver correção publicada.
- Enquanto o projeto está antes de `0.9-beta`, não há canal de atualização; correções entram na próxima release.

## Política

- Toda correção de segurança gera nota em `docs/releases/` e, quando aplicável, teste de regressão.
- Alterações em `unsafe`, syscalls, decodificadores e loader exigem revisão com foco em segurança (ver `CONTRIBUTING.md`).
- Nenhum crash é "não reproduzível" sem log serial e símbolos anexados.

## Threat model v1 (2026-09-07)

Escopo atual: loader UEFI + kernel com **modo usuário** (processos isolados por espaço de
endereçamento, handles com direitos, canais IPC com transferência de handles), serviços em
espaço de usuário (drivers VirtIO, NexoFS em disco gravável, pilha de rede com firewall por
aplicativo, compositor que **media** entrada, clipboard, arrasto, notificações e Contextos,
lançador com consentimento por clique e revogação de capacidade), pacotes locais com lista de
revogação e crash dumps em disco. Ainda **sem TLS, assinatura de pacotes, raiz de confiança,
Secure Boot ou usuários múltiplos** (decisões adiadas; ver o plano). Tudo roda em QEMU.

### Ativos

- Integridade do kernel, das tabelas de página e das pilhas/heap do kernel.
- **Isolamento entre processos** e **confinamento por capabilities**: um processo só alcança o
  que seus handles permitem (direitos ler/escrever/transferir/mapear/sinalizar).
- Dados do usuário no NexoFS (integridade sob corte de energia; sem sigilo — não há cifra).
- **Posse da entrada**: só a janela focada recebe teclado/ponteiro; clipboard e arrasto exigem
  posse ou concessão; um Contexto não vê janelas, avisos nem documentos de outro.
- Política de rede por aplicativo (perfil negar-por-padrão no `netd`).
- Saída de diagnóstico (serial) e crash dumps confiáveis — base do CI e da depuração.

### Adversários e vetores considerados

| Vetor | Estado | Mitigação existente | Próximos passos |
|---|---|---|---|
| **App malicioso ou comprometido** (ELF de terceiros) | parcial | espaço de endereçamento próprio; syscalls validam ponteiros/faixas (`copy_from_user`); só os handles recebidos no spawn ou por canal; o lançador **só concede depois do clique** (Permitir/Negar/por tempo) e **revoga** fechando o proxy da sessão; quota de memória compartilhável por processo; fila de canal limitada; prioridades (segundo plano não atrasa o normal) | quotas de handles; domínios |
| Mensagens IPC malformadas (entre serviços e do app) | coberto | decodificadores gerados do IDL (tamanhos, versões, campos aditivos); erro devolvido, nunca pânico; fuzz-lite dos decodificadores nos testes de host; **fuzz de syscalls** semanal no CI | fuzz guiado por cobertura |
| Pacotes de rede malformados | parcial | parsers de Ethernet/ARP/IPv4/ICMP/UDP/DHCP/DNS/TCP/IPv6 com fuzz-lite e fuzz de estados de protocolo; TCP com janela/retransmissão testados; firewall por aplicativo | fuzz de rede real; TLS (adiado) |
| Imagem de disco malformada / corte de energia | coberto | NexoFS com verificação e **reparo** na montagem (referências duplas, órfãos), teste de corte de energia durante `rename`/escrita; cenário `powercut` no CI | — |
| Kernel/loader/ELF malformado | coberto | loader valida cabeçalho, `ET_EXEC`, rejeita W+X e sobreposição; o kernel valida segmentos do ELF de usuário (faixa, W^X) e limita o tamanho (`process_spawn_mem`) | assinatura do kernel (ADR-0010, adiada) |
| Driver comprometido | parcial | concessões de dispositivo **por função PCI** (config, BARs, DMA, IRQ limitados ao BDF); drivers em espaço de usuário; caminho sem IOMMU é **explicitamente inseguro** (`Passthrough`) | IOMMU real; DMA restrito por capability |
| Bugs de concorrência no kernel (SMP) | parcial | stress de 24 h (zero erros) e de 7 dias em curso; regressão do despertar espúrio no `join` (bloco 94) — o único bug de escalonador encontrado até hoje veio de um bloco de espaço de usuário | sanitizers/`loom` onde couber |
| Corrupção de memória em `unsafe` | parcial | `unsafe_op_in_unsafe_fn=deny`, `SAFETY` obrigatório e **auditado mecanicamente** no `make lint` (`nexo-unsafe-audit`), crates puros `deny(unsafe_code)` | inventário por release |
| Execução de dados, escrita em código, estouro de pilha, endereço inválido | coberto | NX, W^X + CR0.WP, guard pages (kernel e usuário), `#DF` em IST, `#PF` fatal simbolizado; cenários `fault`/`overflow`/`panic` | PIE para os programas C (`nexo-cc` ainda linka `ET_EXEC`) |
| Atualização adulterada ou rollback | parcial | layout A/B com fallback estrutural e **health check pós-boot com rollback automático**; manifesto compara versões; lista de revogação `/apps/.revoked` | **assinatura de pacotes e raiz de confiança (adiadas)** — até lá, só repositórios locais/confiáveis |
| Vazamento entre janelas/Contextos (keylogging, leitura de tela) | coberto | posse da entrada no compositor; clipboard/arrasto mediados; leitor de tela só por assinatura de eventos semânticos; avisos e documentos por Contexto | — |
| Negação de serviço por um processo | parcial | quota de páginas compartilháveis, **quota de CPU por job** (`job_set_cpu_limit`: orçamento por janela de 1 s, com throttle no escalonador), `QueueFull` nas filas de canal, limite de superfícies/sessões no compositor, reinício com limite no gerenciador de serviços | quotas de heap |
| Acesso físico, canais laterais (Spectre e afins), TLS/rede hostil | fora do escopo | — | cifra de disco e TLS ficam para depois da raiz de confiança |

### Suposições

- Um único usuário local; o firmware UEFI e o QEMU são confiáveis.
- Pacotes vêm de repositórios locais ou confiáveis (não há assinatura ainda).
- O canal serial e o disco de dados são controlados pelo operador.

### Superfície `unsafe`

Ver [docs/unsafe-inventory.md](docs/unsafe-inventory.md).

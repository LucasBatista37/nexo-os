# services/

Workspace dos programas de usuário (ring 3, `x86_64-unknown-none`, linker script compartilhado, empacotados em `build/initrd` por `tools/build-image`):

| Programa | Papel |
|---|---|
| `init` | primeiro processo: inicia `svcmgr` e propaga o resultado |
| `svcmgr` | gerenciador de serviços mínimo: inicia `echo`, atende pedidos de conexão do cliente, reinicia o serviço quando ele cai (até 3 vezes) |
| `echo` | serviço de eco; cai de propósito após N pedidos para exercitar o reinício |
| `echo-client` | cliente que reconecta após falhas do serviço |
| `utest` | auto-testes de usuário (ABI, isolamento, IPC) invocados pelo kernel |

Próximos serviços (Plano Mestre §3.6): `device-manager`, `vfs`, `storage-manager`, `network-stack`, `security-broker`, `log-service`, …

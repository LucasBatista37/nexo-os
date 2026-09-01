# SDK do Nexo OS — como escrever um aplicativo

Plano §Fase 6 ("publicar SDK Rust", "criar documentação e exemplos", "criar gerador de projeto").
O SDK é Rust `no_std` (edition 2024, Rust 1.98, alvo `x86_64-unknown-none`), composto pelos crates
do repositório:

| Crate | Para quê |
|---|---|
| `nexo-sys` | syscalls (canais, memória compartilhada, processos) |
| `nexo-rt` | runtime mínimo (`log!`, panic handler) |
| `nexo-proto` | protocolos NXIP gerados da IDL (`idl/*.idl`), incl. `nexo.wm` |
| `nexo-gfx` | desenho 2D (`Surface`, cores, texto) |
| `nexo-ui` | toolkit (temas, `Label`/`Button`/`VStack`/`Nav`) |
| `nexo-pkg` | ler/validar pacotes NEXOPKG1 (para instaladores) |

## Começando

```text
tools/nexo-new meuapp          # gera services/meuapp/ já registrado no build
(cd services && cargo build --release -p nexo-meuapp)
tools/nexo-pack build --manifest services/meuapp/manifest.txt \
    --out meuapp.npk services/target/x86_64-unknown-none/release/nexo-meuapp
```

## O contrato de um aplicativo

1. **Handle 0 = canal com o orquestrador** (o shell). É por ele que o app recebe a sua sessão
   `nexo.wm` (mensagem `"sess"` com um handle) e é o **cordão de vida**: se fechar, o app encerra.
   (Sem observá-lo, app e compositor podem esperar um pelo outro no encerramento.)
2. **Janela**: `create_surface` devolve um `MemoryObject`; mapeie com `memory_map` e desenhe nos
   pixels (RGBX8888) com `nexo-gfx`/`nexo-ui`; `commit` recompõe. `set_title` dá nome à janela
   (Faixa de Atividades e leitores de tela usam).
3. **Entrada**: cliques chegam como evento `pointer{surface,x,y}` (coordenadas locais) e teclas
   como `key{surface,code,value}` — só quando a janela tem o foco (ou a captura). Respostas de RPC
   e eventos compartilham o canal da sessão: consuma com tolerância (pule eventos ao esperar uma
   resposta, guarde-os se precisar).
4. **Mediações**: clipboard só com a posse da entrada; `notify` para avisos; `grab` para entrada
   sensível; `prefs` para respeitar a redução de movimento.
5. **Manifesto** (`manifest.txt`): declare em `perms=` o que o app precisa (base do modelo de
   permissões declarativas + consentimento).

## Exemplos no repositório

- `services/calc` — app completo: botões `nexo-ui` + eventos `pointer` + clipboard mediado.
- `services/greeter` — entrada sensível com captura (`grab`).
- `services/shellui` — o shell: privilégios (`surface_info`/`activate`), broker de sessões.

## Depuração

`nexo_rt::log!` sai na serial (e no console). Os cenários de `tools/test-qemu` mostram como testar
um app de ponta a ponta com entrada sintética (`utest` modos 43–45 são drivers de exemplo).

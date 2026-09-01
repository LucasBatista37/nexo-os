# NEXOPKG1 — formato de pacote de aplicativos (v1)

Plano §Fase 6: "definir formato de pacote e manifesto" e a base das "permissões declarativas".
Implementação de referência: `libraries/pkg` (`nexo-pkg`, `no_std`/sem alocação/`forbid(unsafe)`);
ferramenta: `tools/nexo-pack` (`build`/`inspect`).

## Layout (little-endian, sem alinhamento)

| Campo | Tamanho | Conteúdo |
|---|---|---|
| magic | 8 B | `"NEXOPKG1"` |
| versao | u32 | 1 |
| manifest_len | u32 | bytes do manifesto |
| file_count | u32 | quantos arquivos (≤ 64) |
| crc32 | u32 | CRC32 (IEEE) de todo o payload |
| payload | — | manifesto + arquivos |

Cada arquivo no payload: `name_len: u16` · nome (UTF-8, ≤ 32 B) · `data_len: u32` · dados.
Bytes sobrando após o último arquivo são erro (tabela inconsistente). Todo o payload é validado
já no `Package::parse` (fuzz-lite de truncamentos e mutações nos testes de host).

## Manifesto

Texto UTF-8, uma declaração por linha (`chave=valor`; `#` comenta; ordem livre):

```
name=calc            # obrigatório, ≤ 32
version=0.1.0        # obrigatório, ≤ 16
entry=calc.elf       # obrigatório: executável dentro do pacote, ≤ 32
perms=janelas, clipboard   # opcional: permissões que o app DECLARA precisar
```

Chaves desconhecidas são **erro** (não há campos ignorados nesta versão: o manifesto é a
superfície de auditoria do pacote). `perms` é a declaração para o modelo de permissões
declarativas + consentimento; a **imposição** (instalador/portais checando as permissões na
concessão de capabilities) vem nos blocos de instalação.

## O que vem por cima (blocos futuros)

- assinatura e verificação (hash assinado do pacote inteiro);
- instalação transacional (staging + troca atômica, no espírito do commit do NexoFS);
- repositório e revisão/revogação.

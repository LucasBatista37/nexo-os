# ADR-0016 — NexoFS v0: sistema de arquivos persistente de teste com commits atômicos por setor

- **Status:** aceita (formato **instável** até `0.3-storage`)
- **Data:** 2026-08-30
- **Relacionados:** ADR-0015 (drivers em modo usuário), Plano §Fase 3 (gate F3)

## Decisão

Antes de escolher/portar um sistema de arquivos definitivo, o projeto usa um formato próprio e pequeno — **NexoFS v0** (`libraries/nexofs`, `no_std`, sem alocação, `forbid(unsafe_code)`) — servido por um processo de usuário (`services/fs`) sobre o driver de bloco (`services/blockdev`). Objetivo: cumprir o gate F3 com um código auditável e **testável no host** (mesma biblioteca roda em `cargo test` com um disco em memória).

Formato: blocos de 2 KiB (uma mensagem IPC por bloco); superbloco com CRC32; bitmap de blocos (cache reconstruído na montagem); tabela de inodes de 128 B com CRC32 (12 ponteiros diretos + 1 indireto → arquivos até 536 KiB); diretórios como arquivos de entradas de 64 B com CRC32 (nomes ≤ 55 B); hierarquia até 16 níveis.

Consistência em cortes de energia: **toda operação termina com um único registro de commit que cabe em um setor de 512 B** (inode ou entrada de diretório). Dados novos vão para blocos novos (copy-on-write); os antigos são liberados depois do commit. Um corte em qualquer escrita deixa cada arquivo na versão anterior ou na nova; blocos e inodes órfãos são recuperados na montagem (`Info::repairs`). Limitação documentada: para arquivos com mais de 12 blocos, o bloco indireto é confirmado antes do inode — mistura de versões *por bloco* é possível, ponteiros pendentes não.

Testes (host): roteiro de 11 operações interrompido em **cada** escrita, com escrita rasgada (0, 1 ou 3 setores gravados) — após a remontagem cada arquivo está em uma versão permitida, o volume continua utilizável e uma segunda montagem não repara nada; imagens corrompidas aleatoriamente (400 casos) nunca causam pânico. `tools/nexo-disk` (Python, independente do Rust) inspeciona e verifica volumes (`ls`, `cat`, `check`); o cenário `storage` usa-o após dois boots.

## Consequências

- Não é um sistema de arquivos de produção: sem journal, sem *extents*, sem atributos, sem tempos, um cliente por servidor, sem cache de blocos (cada bloco é uma mensagem ao driver). Estes itens continuam abertos na Fase 3 (cache de blocos e fila assíncrona; VFS e namespace por processo).
- Os últimos 256 setores do disco ficam fora do volume (área crua para os testes de bloco).
- Comportamento de **volume de teste**: iniciado com argumento 0, o `fs` formata o disco quando não há assinatura ou quando o volume está inutilizável (ex.: geometria de uma versão anterior), registrando um aviso; o argumento 2 é a montagem estrita (termina com 32). Um FS de produção nunca reformataria sozinho.
- Protocolo `nexo.fs` v0 é cru e provisório (`docs/spec/ipc-compat.md` §5), a ser substituído pelo protocolo tipado gerado da IDL.

## Alternativas

FAT (rejeitada como FS principal: sem CRCs nem semântica de commit; fica restrita ao ESP, item próprio da fase); ext2/littlefs portados (adiados: dependem de VFS e cache que ainda não existem; o v0 serve de banco de testes para essa infraestrutura).

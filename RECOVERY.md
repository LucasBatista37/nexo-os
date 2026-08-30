# Backup, rollback e recuperação

## Estado atual (`0.0.1-boot`)

Não há sistema instalado nem dados de usuário: o "sistema" é uma imagem imutável gerada por `tools/build-image` e executada em QEMU. Recuperar = regenerar a imagem a partir do repositório (build reproduzível, `make reproducible`).

### Recuperação do projeto

- Repositório git com tags de release; artefatos de cada tag (imagem, kernel, símbolos, hashes) anexados.
- Toolchain fixada (`docs/toolchain.md`) permite reconstruir qualquer release em máquina limpa.
- Rotina (Plano Mestre §12, a cada 4 semanas): clonar em diretório novo, `make ci`, comparar hash da imagem com o registrado na release.

### Diagnóstico de falhas de boot

1. `tools/run-qemu --test --log build/boot.log` captura a serial.
2. `tools/symbolize build/boot.log` resolve endereços de panics/exceções.
3. `tools/run-qemu --gdb` para inspeção passo a passo.

## Plano (Fases 8–10, ADR-0010)

- Layout A/B com slot ativo escolhido pelo bootloader e fallback automático após health check.
- Atualizações atômicas assinadas (TUF), proteção contra rollback, ambiente de recuperação independente.
- Backup/restauração de dados de usuário e reset preservando arquivos.
- Testes de corte de energia durante atualização e de slot corrompido (gate F8).

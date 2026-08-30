# tests/

- `host/`: reservado para testes de integração de host entre crates (os testes unitários vivem em cada crate).
- `qemu/`: os cenários de QEMU são definidos em `tools/test-qemu` (marcadores esperados/proibidos por cenário) e os testes de kernel em `kernel/src/selftest.rs`. Este diretório receberá suítes de integração/fuzz/hardware nas próximas fases.

Ver `docs/testing.md`.

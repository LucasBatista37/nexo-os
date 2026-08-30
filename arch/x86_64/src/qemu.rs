//! Dispositivo `isa-debug-exit` do QEMU (porta 0xf4).
//!
//! O código de saída do processo QEMU é `(valor << 1) | 1`.

use crate::cpu::outl;

/// Porta configurada em `tools/run-qemu`.
pub const DEBUG_EXIT_PORT: u16 = 0xf4;
/// Valor para sucesso → QEMU sai com 33.
pub const EXIT_SUCCESS: u32 = 0x10;
/// Valor para falha → QEMU sai com 35.
pub const EXIT_FAILURE: u32 = 0x11;

/// Código de processo esperado para `value`.
pub const fn host_exit_code(value: u32) -> u32 {
    (value << 1) | 1
}

/// Encerra o QEMU com `value`. Se o dispositivo não existir, para a CPU.
pub fn exit(value: u32) -> ! {
    // SAFETY: a porta 0xf4 só tem efeito se o dispositivo de debug existir.
    unsafe { outl(DEBUG_EXIT_PORT, value) };
    crate::cpu::halt_forever()
}

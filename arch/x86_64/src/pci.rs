//! Acesso ao espaço de configuração PCI pelo mecanismo legado (portas 0xCF8/0xCFC).
//!
//! Suficiente para os 256 bytes de configuração de cada função; o ECAM (MCFG)
//! entra quando forem necessários registradores estendidos PCIe.

use crate::cpu::{inl, outl};

const CONFIG_ADDRESS: u16 = 0xcf8;
const CONFIG_DATA: u16 = 0xcfc;

pub use crate::pci_types::Bdf;

fn address(bdf: Bdf, offset: u8) -> u32 {
    0x8000_0000
        | ((bdf.bus as u32) << 16)
        | ((bdf.device as u32 & 0x1f) << 11)
        | ((bdf.function as u32 & 7) << 8)
        | (offset as u32 & 0xfc)
}

/// Lê 32 bits do espaço de configuração (offset alinhado a 4).
///
/// # Safety
/// Acesso a portas de E/S; deve ser serializado pelo chamador.
pub unsafe fn config_read32(bdf: Bdf, offset: u8) -> u32 {
    // SAFETY: contrato da função.
    unsafe {
        outl(CONFIG_ADDRESS, address(bdf, offset));
        inl(CONFIG_DATA)
    }
}

/// Escreve 32 bits no espaço de configuração (offset alinhado a 4).
///
/// # Safety
/// Pode reprogramar o dispositivo; deve ser serializado pelo chamador.
pub unsafe fn config_write32(bdf: Bdf, offset: u8, value: u32) {
    // SAFETY: contrato da função.
    unsafe {
        outl(CONFIG_ADDRESS, address(bdf, offset));
        outl(CONFIG_DATA, value);
    }
}

/// Lê 16 bits (offset alinhado a 2).
///
/// # Safety
/// Ver [`config_read32`].
pub unsafe fn config_read16(bdf: Bdf, offset: u8) -> u16 {
    // SAFETY: contrato da função.
    let v = unsafe { config_read32(bdf, offset & 0xfc) };
    (v >> ((offset & 2) * 8)) as u16
}

/// Lê 8 bits.
///
/// # Safety
/// Ver [`config_read32`].
pub unsafe fn config_read8(bdf: Bdf, offset: u8) -> u8 {
    // SAFETY: contrato da função.
    let v = unsafe { config_read32(bdf, offset & 0xfc) };
    (v >> ((offset & 3) * 8)) as u8
}

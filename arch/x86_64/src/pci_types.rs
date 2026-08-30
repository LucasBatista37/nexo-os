//! Tipos PCI independentes de plataforma.

/// Endereço bus/device/function.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Bdf {
    /// Barramento.
    pub bus: u8,
    /// Dispositivo (0..32).
    pub device: u8,
    /// Função (0..8).
    pub function: u8,
}

impl Bdf {
    /// Cria um BDF.
    pub const fn new(bus: u8, device: u8, function: u8) -> Self {
        Bdf {
            bus,
            device,
            function,
        }
    }
    /// Codificação compacta `bus << 8 | device << 3 | function`.
    pub const fn packed(self) -> u16 {
        ((self.bus as u16) << 8) | ((self.device as u16) << 3) | (self.function as u16 & 7)
    }
    /// Decodifica de [`packed`](Self::packed).
    pub const fn from_packed(v: u16) -> Self {
        Bdf {
            bus: (v >> 8) as u8,
            device: ((v >> 3) & 0x1f) as u8,
            function: (v & 7) as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bdf_packing() {
        let b = Bdf::new(3, 31, 5);
        assert_eq!(Bdf::from_packed(b.packed()), b);
        assert_eq!(b.packed(), 0x03fd);
    }
}

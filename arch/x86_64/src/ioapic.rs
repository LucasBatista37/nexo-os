//! I/O APIC (registradores indiretos IOREGSEL/IOWIN).

const IOREGSEL: u64 = 0x00;
const IOWIN: u64 = 0x10;

const REG_ID: u32 = 0;
const REG_VER: u32 = 1;
const REG_TABLE: u32 = 0x10;

/// Handle de um I/O APIC mapeado.
#[derive(Clone, Copy)]
pub struct IoApic {
    base: u64,
    gsi_base: u32,
}

// SAFETY: acesso serializado pelo chamador (lock no kernel).
unsafe impl Send for IoApic {}
unsafe impl Sync for IoApic {}

/// Configuração de uma entrada de redirecionamento.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Redirection {
    /// Vetor entregue.
    pub vector: u8,
    /// APIC ID de destino (modo físico).
    pub dest_apic_id: u32,
    /// Gatilho por nível (`true`) ou borda.
    pub level_triggered: bool,
    /// Ativo em nível baixo.
    pub active_low: bool,
    /// Entrada mascarada.
    pub masked: bool,
}

impl IoApic {
    /// Cria o handle para o I/O APIC mapeado em `virt_base`, que atende GSIs a partir de `gsi_base`.
    ///
    /// # Safety
    /// `virt_base` deve mapear os registradores do I/O APIC sem cache.
    pub const unsafe fn new(virt_base: u64, gsi_base: u32) -> Self {
        IoApic {
            base: virt_base,
            gsi_base,
        }
    }

    fn read(&self, reg: u32) -> u32 {
        // SAFETY: registradores dentro da página mapeada.
        unsafe {
            core::ptr::write_volatile((self.base + IOREGSEL) as *mut u32, reg);
            core::ptr::read_volatile((self.base + IOWIN) as *const u32)
        }
    }

    fn write(&self, reg: u32, v: u32) {
        // SAFETY: idem.
        unsafe {
            core::ptr::write_volatile((self.base + IOREGSEL) as *mut u32, reg);
            core::ptr::write_volatile((self.base + IOWIN) as *mut u32, v);
        }
    }

    /// ID.
    pub fn id(&self) -> u32 {
        (self.read(REG_ID) >> 24) & 0xf
    }

    /// Versão (bits 0..8).
    pub fn version(&self) -> u32 {
        self.read(REG_VER) & 0xff
    }

    /// Número de entradas de redirecionamento.
    pub fn entries(&self) -> u32 {
        ((self.read(REG_VER) >> 16) & 0xff) + 1
    }

    /// Primeiro GSI atendido.
    pub fn gsi_base(&self) -> u32 {
        self.gsi_base
    }

    /// `true` se este I/O APIC atende `gsi`.
    pub fn handles(&self, gsi: u32) -> bool {
        gsi >= self.gsi_base && gsi < self.gsi_base + self.entries()
    }

    /// Programa a entrada de `gsi`.
    pub fn set_redirection(&self, gsi: u32, r: Redirection) {
        let idx = gsi - self.gsi_base;
        let low = r.vector as u32
            | (r.active_low as u32) << 13
            | (r.level_triggered as u32) << 15
            | (r.masked as u32) << 16;
        let high = r.dest_apic_id << 24;
        // Mascara antes de alterar (evita entrega com metade da entrada escrita).
        self.write(REG_TABLE + 2 * idx, low | (1 << 16));
        self.write(REG_TABLE + 2 * idx + 1, high);
        self.write(REG_TABLE + 2 * idx, low);
    }

    /// Lê a entrada crua `(low, high)`.
    pub fn redirection_raw(&self, gsi: u32) -> (u32, u32) {
        let idx = gsi - self.gsi_base;
        (
            self.read(REG_TABLE + 2 * idx),
            self.read(REG_TABLE + 2 * idx + 1),
        )
    }

    /// Mascara uma entrada.
    pub fn mask(&self, gsi: u32) {
        let idx = gsi - self.gsi_base;
        let low = self.read(REG_TABLE + 2 * idx);
        self.write(REG_TABLE + 2 * idx, low | (1 << 16));
    }

    /// Mascara todas as entradas.
    pub fn mask_all(&self) {
        for i in 0..self.entries() {
            self.write(REG_TABLE + 2 * i, 1 << 16);
            self.write(REG_TABLE + 2 * i + 1, 0);
        }
    }
}

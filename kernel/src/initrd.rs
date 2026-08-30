//! Acesso ao initramfs entregue pelo loader (formato `nexo-initrd`).

use nexo_initrd::Initrd;
use nexo_sync::Once;

static IMAGE: Once<Initrd<'static>> = Once::new();

/// Valida o initrd e lista seus membros.
pub fn init() {
    let Some(bytes) = crate::boot::initrd() else {
        kwarn!("initrd: ausente; nenhum programa de usuario disponivel");
        return;
    };
    match Initrd::parse(bytes) {
        Ok(ird) => {
            kinfo!(
                "initrd: {} membro(s) em {} KiB",
                ird.len(),
                bytes.len() >> 10
            );
            for m in ird.iter() {
                kinfo!("initrd:   {:<20} {:>8} bytes", m.name, m.data.len());
            }
            let _ = IMAGE.set(ird);
        }
        Err(e) => kerror!("initrd: imagem invalida: {e:?}"),
    }
}

/// Conteúdo do membro `name`.
pub fn find(name: &str) -> Option<&'static [u8]> {
    IMAGE.get()?.find(name)
}

/// Número de membros.
pub fn count() -> usize {
    IMAGE.get().map_or(0, |i| i.len())
}

//! Relógio de tempo real (CMOS/MC146818): leitura única no boot para ancorar o relógio de
//! parede. Portas 0x70 (índice) / 0x71 (dado); guarda de "atualização em andamento" (bit 7 do
//! registrador A) e leitura dupla até estabilizar; converte BCD e 12h quando o registrador B
//! pedir. O RTC do QEMU roda em UTC por padrão (`-rtc base=utc`).

use crate::cpu::{inb, outb};

const IDX: u16 = 0x70;
const DATA: u16 = 0x71;

fn read_reg(r: u8) -> u8 {
    // SAFETY: portas CMOS padrão do PC; leitura sem efeitos além da seleção de índice.
    unsafe {
        outb(IDX, r);
        inb(DATA)
    }
}

fn bcd(v: u8) -> u8 {
    (v >> 4) * 10 + (v & 0x0f)
}

/// Uma foto (segundo, minuto, hora, dia, mês, ano de 2 dígitos).
fn snapshot() -> [u8; 6] {
    [
        read_reg(0x00),
        read_reg(0x02),
        read_reg(0x04),
        read_reg(0x07),
        read_reg(0x08),
        read_reg(0x09),
    ]
}

/// Dias desde 1970-01-01 para uma data civil (algoritmo de Howard Hinnant, sem pânico).
fn days_from_civil(y: i64, m: u64, d: u64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = (m + 9) % 12; // mar=0 .. fev=11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe as i64 - 719468
}

/// Lê o RTC e devolve o instante como segundos Unix (UTC); `None` se a leitura não estabilizar.
pub fn read_epoch() -> Option<u64> {
    // espera o fim de uma atualização em andamento (bit 7 do registrador A)
    let mut spins = 0u32;
    while read_reg(0x0a) & 0x80 != 0 {
        spins += 1;
        if spins > 1_000_000 {
            return None;
        }
    }
    // duas leituras iguais seguidas = foto estável
    let mut a = snapshot();
    for _ in 0..16 {
        let b = snapshot();
        if a == b {
            let regb = read_reg(0x0b);
            let bin = regb & 0x04 != 0;
            let h24 = regb & 0x02 != 0;
            let cv = |v: u8| if bin { v } else { bcd(v) };
            let (s, mi, d, mo, yy) = (cv(a[0]), cv(a[1]), cv(a[3]), cv(a[4]), cv(a[5]));
            let mut h = a[2];
            let pm = !h24 && (h & 0x80) != 0;
            h &= 0x7f;
            let mut h = cv(h);
            if !h24 {
                h %= 12;
                if pm {
                    h += 12;
                }
            }
            if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || s > 59 || mi > 59 || h > 23 {
                return None;
            }
            let year = 2000 + yy as i64; // século fixo: válido até 2099
            let days = days_from_civil(year, mo as u64, d as u64);
            if days < 0 {
                return None;
            }
            return Some(days as u64 * 86400 + h as u64 * 3600 + mi as u64 * 60 + s as u64);
        }
        a = b;
    }
    None
}

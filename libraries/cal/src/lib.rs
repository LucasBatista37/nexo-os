//! `nexo-cal` — datas civis (Plano §Fase 6: "criar calculadora, calendário e utilitários").
//! Conversões entre segundos Unix (UTC) e data civil, dia da semana e dias no mês — o
//! suficiente para um calendário honesto. Algoritmos de Howard Hinnant (`civil_from_days` /
//! `days_from_civil`), `no_std`, sem alocação e sem pânico possível para qualquer entrada.
#![no_std]

/// Data civil (calendário gregoriano proléptico, UTC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    /// Ano (ex.: 2026).
    pub year: i64,
    /// Mês 1..=12.
    pub month: u8,
    /// Dia 1..=31.
    pub day: u8,
    /// Dia da semana, segunda = 0 .. domingo = 6.
    pub weekday: u8,
}

/// Dias desde 1970-01-01 para uma data civil.
pub fn days_from_civil(y: i64, m: u8, d: u8) -> i64 {
    let (m, d) = (m as i64, d as i64);
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // mar=0 .. fev=11
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Data civil para dias desde 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u8, u8) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m as u8, d as u8)
}

/// Dia da semana (segunda = 0 .. domingo = 6) para dias desde 1970-01-01 (que foi quinta).
pub fn weekday_from_days(z: i64) -> u8 {
    ((z + 3).rem_euclid(7)) as u8
}

/// `true` se o ano é bissexto.
pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Dias no mês (`0` para mês inválido).
pub fn days_in_month(y: i64, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Segundos Unix (UTC) para data civil com dia da semana.
pub fn civil_from_epoch(secs: u64) -> Civil {
    let days = (secs / 86400) as i64;
    let (year, month, day) = civil_from_days(days);
    Civil {
        year,
        month,
        day,
        weekday: weekday_from_days(days),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn known_dates_round_trip() {
        // valores conferidos com o datetime do Python (UTC)
        for (epoch, y, m, d, wd) in [
            (0u64, 1970, 1, 1, 3u8),      // quinta
            (951782400, 2000, 2, 29, 1),  // bissexto
            (1788220800, 2026, 9, 1, 1),  // terca
            (2147472000, 2038, 1, 19, 1), // alem do y2k38 de 32 bits
            (4107542400, 2100, 3, 1, 0),  // 2100 nao e bissexto
        ] {
            let c = civil_from_epoch(epoch);
            assert_eq!((c.year, c.month, c.day, c.weekday), (y, m, d, wd));
            assert_eq!(days_from_civil(y, m, d) * 86400, epoch as i64);
        }
    }

    #[test]
    fn month_lengths() {
        assert_eq!(days_in_month(2026, 9), 30);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2026, 13), 0);
    }

    #[test]
    fn round_trip_every_day_for_400_years() {
        let z0 = days_from_civil(1970, 1, 1);
        for off in 0..146097 {
            let z = z0 + off;
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
            assert!(d >= 1 && d <= days_in_month(y, m));
        }
    }
}

//! Hora local: para saber qué es "hoy" (de medianoche a medianoche).

/// Un día en milisegundos.
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Diferencia de la hora local con la UTC, en milisegundos (en Windows; en
/// otros sistemas, 0). Incluye el horario de verano vigente.
pub fn local_offset_ms() -> i64 {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::SYSTEMTIME;
        use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
        let ms = |t: SYSTEMTIME| {
            days_from_civil(t.wYear.into(), t.wMonth.into(), t.wDay.into()) * DAY_MS
                + i64::from(t.wHour) * 3_600_000
                + i64::from(t.wMinute) * 60_000
                + i64::from(t.wSecond) * 1_000
        };
        // SAFETY: las dos funciones solo devuelven la hora.
        let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
        // Redondeado a cuartos de hora: entre las dos llamadas pasa un instante.
        let quarter = 15 * 60_000;
        ((ms(local) - ms(utc)) as f64 / quarter as f64).round() as i64 * quarter
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Inicio (epoch ms) del día local que contiene `now`.
pub fn day_start(now: i64, offset_ms: i64) -> i64 {
    (now + offset_ms).div_euclid(DAY_MS) * DAY_MS - offset_ms
}

/// Días desde 1970-01-01 de una fecha civil (algoritmo de Howard Hinnant).
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fechas_y_comienzo_del_dia() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 2, 29), 11_016);
        assert_eq!(
            days_from_civil(2026, 11, 18) * DAY_MS,
            1_795_046_400_000 - DAY_MS
        );
        // UTC-4: las 02:00 UTC del 19 siguen siendo el 18 en local.
        let offset = -4 * 3_600_000;
        let now = 1_795_046_400_000 + 2 * 3_600_000; // 19 nov, 02:00 UTC
        assert_eq!(
            day_start(now, offset),
            1_795_046_400_000 - DAY_MS + 4 * 3_600_000
        );
        // A las 05:00 UTC ya es el 19 en local.
        assert_eq!(
            day_start(now + 3 * 3_600_000, offset),
            1_795_046_400_000 + 4 * 3_600_000
        );
    }
}

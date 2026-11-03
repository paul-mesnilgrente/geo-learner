//! Dates des parties, à l'heure locale : « 2 nov. », « 2 nov. 2025, 14:32 » ; « Nov 2 ».

use crate::i18n::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

const MONTHS: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
    "déc.",
];
const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl DateTime {
    /// Date et heure locales d'un instant (secondes depuis l'epoch Unix).
    pub fn local(unix: u64) -> Self {
        local(unix).unwrap_or_else(|| Self::utc(unix))
    }

    pub fn utc(unix: u64) -> Self {
        let days = (unix / 86_400) as i64;
        let seconds = unix % 86_400;
        let (year, month, day) = civil(days);
        Self {
            year,
            month,
            day,
            hour: (seconds / 3600) as u32,
            minute: (seconds / 60 % 60) as u32,
        }
    }

    /// « 2 nov. » ; « Nov 2 ».
    pub fn short(&self, lang: Lang) -> String {
        let month = (self.month - 1) as usize;
        match lang {
            Lang::Fr => format!("{} {}", self.day, MONTHS[month]),
            Lang::En => format!("{} {}", MONTHS_EN[month], self.day),
        }
    }

    /// « 2 nov., 14:32 », avec l'année si ce n'est pas celle de `now` ; « Nov 2, 14:32 ».
    pub fn long(&self, now: &DateTime, lang: Lang) -> String {
        let year = if self.year == now.year {
            String::new()
        } else {
            match lang {
                Lang::Fr => format!(" {}", self.year),
                Lang::En => format!(", {}", self.year),
            }
        };
        format!(
            "{}{year}, {:02}:{:02}",
            self.short(lang),
            self.hour,
            self.minute
        )
    }
}

/// Jour civil (année, mois, jour) d'un nombre de jours depuis le 1er janvier 1970
/// (algorithme de Howard Hinnant).
fn civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month, day)
}

#[cfg(unix)]
fn local(unix: u64) -> Option<DateTime> {
    let time = libc::time_t::try_from(unix).ok()?;
    // SAFETY : `localtime_r` remplit la structure fournie et ne garde aucun pointeur.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&time, &mut tm).is_null() {
            return None;
        }
        tm
    };
    Some(DateTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
    })
}

#[cfg(windows)]
fn local(unix: u64) -> Option<DateTime> {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTime;

    let utc = DateTime::utc(unix);
    let universal = SYSTEMTIME {
        wYear: u16::try_from(utc.year).ok()?,
        wMonth: utc.month as u16,
        wDayOfWeek: 0,
        wDay: utc.day as u16,
        wHour: utc.hour as u16,
        wMinute: utc.minute as u16,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY : structures valides ; un fuseau nul désigne le fuseau actif du système.
    let local = unsafe {
        let mut local: SYSTEMTIME = std::mem::zeroed();
        if SystemTimeToTzSpecificLocalTime(std::ptr::null(), &universal, &mut local) == 0 {
            return None;
        }
        local
    };
    Some(DateTime {
        year: i32::from(local.wYear),
        month: u32::from(local.wMonth),
        day: u32::from(local.wDay),
        hour: u32::from(local.wHour),
        minute: u32::from(local.wMinute),
    })
}

#[cfg(not(any(unix, windows)))]
fn local(_: u64) -> Option<DateTime> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_dates() {
        let d = DateTime::utc(0);
        assert_eq!((d.year, d.month, d.day, d.hour), (1970, 1, 1, 0));
        // 29 février 2024, 13:05 UTC.
        let d = DateTime::utc(1_709_211_900);
        assert_eq!(
            (d.year, d.month, d.day, d.hour, d.minute),
            (2024, 2, 29, 13, 5)
        );
        assert_eq!(d.short(Lang::Fr), "29 févr.");
        let now = DateTime::utc(1_735_689_600); // 1er janvier 2025
        assert_eq!(d.long(&now, Lang::Fr), "29 févr. 2024, 13:05");
        assert_eq!(now.long(&now, Lang::Fr), "1 janv., 00:00");
        assert_eq!(d.long(&now, Lang::En), "Feb 29, 2024, 13:05");
    }
}

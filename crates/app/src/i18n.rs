//! Langue de l'interface : celle du système tant que l'utilisateur n'en a pas choisi une, puis
//! celle qu'il a choisie (enregistrée dans les réglages).
//!
//! Les textes sont écrits dans chaque langue là où ils servent, avec [`tr!`] : ajouter une
//! langue, c'est ajouter une variante à [`Lang`] et un argument à chaque `tr!` (le compilateur
//! signale ceux qui manquent).

pub use geo_core::Lang;

/// Texte dans la langue voulue, mis en forme comme `format!` :
/// `tr!(lang, "{n} pays trouvés", "{n} countries found")`.
#[macro_export]
macro_rules! tr {
    ($lang:expr, $fr:literal, $en:literal $(,)?) => {
        match $lang {
            $crate::i18n::Lang::Fr => format!($fr),
            $crate::i18n::Lang::En => format!($en),
        }
    };
    ($lang:expr, $fr:literal, $en:literal, $($arg:tt)+) => {
        match $lang {
            $crate::i18n::Lang::Fr => format!($fr, $($arg)+),
            $crate::i18n::Lang::En => format!($en, $($arg)+),
        }
    };
}

/// Nom de la langue, écrit dans cette langue (sélecteur).
pub fn native_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "Français",
        Lang::En => "English",
    }
}

/// Langue du système, si elle est prise en charge ; l'anglais sinon.
pub fn system() -> Lang {
    system_locale()
        .as_deref()
        .and_then(Lang::from_locale)
        .unwrap_or(Lang::En)
}

#[cfg(not(windows))]
fn system_locale() -> Option<String> {
    // Ordre de priorité POSIX ; « C » et « POSIX » ne disent rien de la langue.
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
}

#[cfg(windows)]
fn system_locale() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    // SAFETY : le tampon a la taille maximale d'un nom de locale (LOCALE_NAME_MAX_LENGTH).
    let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    (len > 1).then(|| String::from_utf16_lossy(&buffer[..len as usize - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales() {
        assert_eq!(Lang::from_locale("fr_FR.UTF-8"), Some(Lang::Fr));
        assert_eq!(Lang::from_locale("en-US"), Some(Lang::En));
        assert_eq!(Lang::from_locale("de_DE"), None);
        let n = 3;
        assert_eq!(tr!(Lang::En, "{n} pays", "{n} countries"), "3 countries");
        assert_eq!(tr!(Lang::Fr, "Bonjour", "Hello"), "Bonjour");
    }
}

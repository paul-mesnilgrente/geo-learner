//! Liens vers l'extérieur (dépôt, politique de confidentialité, contact), ouverts dans le
//! navigateur ou le client mail du système, sans dépendance : `xdg-open` (relayé par les portails
//! dans Flatpak et par snapd dans un Snap) ou l'Explorateur sous Windows.

pub const REPOSITORY: &str = "https://github.com/paul-mesnilgrente/geo-learner";
pub const PRIVACY: &str = "https://github.com/paul-mesnilgrente/geo-learner/blob/main/PRIVACY.md";
pub const CONTACT: &str = "big.learner.app@gmail.com";

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Ouvre une adresse (https:, mailto:) avec l'application par défaut du système.
pub fn open(url: &str) {
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(windows))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = result {
        eprintln!("impossible d'ouvrir {url} : {e}");
    }
}

/// Système et format du paquet, pour un signalement : « Linux x86_64 (Flatpak) ».
pub fn system() -> String {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        other => other,
    };
    let package = if std::env::var_os("FLATPAK_ID").is_some() {
        " (Flatpak)"
    } else if std::env::var_os("SNAP").is_some() {
        " (Snap)"
    } else {
        ""
    };
    format!("{os} {}{package}", std::env::consts::ARCH)
}

/// Nouvelle issue GitHub, préremplie avec la version et le système.
pub fn new_issue() -> String {
    let body = format!(
        "**Version** : {VERSION}\n**Système** : {}\n\n**Ce qui se passe**\n\n\n**Ce qui était attendu**\n\n",
        system()
    );
    format!("{REPOSITORY}/issues/new?body={}", encode(&body))
}

/// E-mail au contact, avec la version et le système dans l'objet.
pub fn mail() -> String {
    let subject = format!("GeoLearner {VERSION} ({})", system());
    format!("mailto:{CONTACT}?subject={}", encode(&subject))
}

/// Encodage pour URL : tout sauf les caractères non réservés (RFC 3986) en `%XX`.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_encoded() {
        assert_eq!(encode("a b/é"), "a%20b%2F%C3%A9");
        let issue = new_issue();
        assert!(issue.starts_with(&format!("{REPOSITORY}/issues/new?body=")));
        assert!(!issue[REPOSITORY.len()..].contains(' '));
        assert!(mail().starts_with("mailto:big.learner.app@gmail.com?subject=GeoLearner%20"));
    }
}

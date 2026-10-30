//! Normalisation des réponses : casse, accents, ponctuation et articles ignorés.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

const LEADING_ARTICLES: &[&str] = &["le", "la", "les", "l", "the"];

/// Ramène un nom de pays à une forme canonique comparable.
///
/// `"L'Île Maurice"` → `"ile maurice"`, `"St-Kitts"` → `"saint kitts"`.
pub fn normalize(input: &str) -> String {
    let mut flat = String::with_capacity(input.len());
    for c in input.nfd() {
        if is_combining_mark(c) {
            continue;
        }
        match c {
            'œ' | 'Œ' => flat.push_str("oe"),
            'æ' | 'Æ' => flat.push_str("ae"),
            'ß' => flat.push_str("ss"),
            c if c.is_alphanumeric() => flat.extend(c.to_lowercase()),
            _ => flat.push(' '),
        }
    }

    let mut words: Vec<&str> = flat
        .split_whitespace()
        .map(|w| match w {
            "st" => "saint",
            "ste" => "sainte",
            w => w,
        })
        .collect();
    while words.len() > 1 && LEADING_ARTICLES.contains(&words[0]) {
        words.remove(0);
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn ignores_case_accents_and_punctuation() {
        assert_eq!(normalize("Côte d'Ivoire"), "cote d ivoire");
        assert_eq!(normalize("  ÉTATS-UNIS "), "etats unis");
        assert_eq!(normalize("Bahreïn"), "bahrein");
        assert_eq!(normalize("Viêt Nam"), "viet nam");
    }

    #[test]
    fn drops_leading_articles_only() {
        assert_eq!(normalize("Les Bahamas"), "bahamas");
        assert_eq!(normalize("l'Inde"), "inde");
        assert_eq!(normalize("Laos"), "laos");
        assert_eq!(normalize("Sri Lanka"), "sri lanka");
        // Un article seul reste tel quel.
        assert_eq!(normalize("la"), "la");
    }

    #[test]
    fn expands_saint_abbreviations() {
        assert_eq!(normalize("St-Kitts"), "saint kitts");
        assert_eq!(normalize("Ste Lucie"), "sainte lucie");
    }
}

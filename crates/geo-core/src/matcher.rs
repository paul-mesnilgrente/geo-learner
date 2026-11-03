//! Reconnaissance des noms de pays saisis au clavier ou dictés.

use std::collections::HashMap;

use crate::normalize::normalize;
use crate::world::World;

pub struct Matcher {
    answers: HashMap<String, usize>,
    /// Réponses sans espaces, pour la recherche approchée.
    compact: Vec<(String, usize)>,
    /// Pays dont un nom est le début du nom d'un autre (« niger » / « nigeria »).
    extendable: Vec<bool>,
    max_words: usize,
}

impl Matcher {
    pub fn new(world: &World) -> Self {
        Self::with_filter(world, |_| true)
    }

    /// Ne retient que les réponses acceptées par `keep` (les noms seuls, ou les numéros seuls).
    pub fn with_filter(world: &World, keep: impl Fn(&str) -> bool) -> Self {
        let mut answers = HashMap::new();
        let mut max_words = 1;
        for (i, country) in world.countries.iter().enumerate() {
            for answer in country.answers.iter().filter(|a| keep(a)) {
                max_words = max_words.max(answer.split(' ').count());
                answers.insert(answer.clone(), i);
            }
        }
        let compact = answers
            .iter()
            .map(|(a, &c)| (a.replace(' ', ""), c))
            .collect();
        let mut extendable = vec![false; world.countries.len()];
        for (short, &c) in &answers {
            extendable[c] |= answers.iter().any(|(long, &d)| {
                d != c && long.len() > short.len() && long.starts_with(short.as_str())
            });
        }
        Self {
            answers,
            compact,
            extendable,
            max_words,
        }
    }

    /// Un nom de ce pays peut-il encore s'allonger en celui d'un autre pays ?
    pub fn can_extend(&self, country: usize) -> bool {
        self.extendable[country]
    }

    /// Le pays dont le nom correspond exactement au texte (après normalisation).
    pub fn lookup(&self, text: &str) -> Option<usize> {
        self.answers.get(&normalize(text)).copied()
    }

    /// Tous les pays cités dans une phrase dictée, dans l'ordre, en privilégiant
    /// les noms les plus longs (« guinée équatoriale » plutôt que « guinée »).
    pub fn scan(&self, text: &str) -> Vec<usize> {
        self.scan_with(text, false)
    }

    /// Comme [`scan`](Self::scan), en tolérant de petites fautes de transcription
    /// (« État-Unis », « Côte d'Ivoir »). Les noms courts doivent rester exacts.
    pub fn scan_fuzzy(&self, text: &str) -> Vec<usize> {
        self.scan_with(text, true)
    }

    fn scan_with(&self, text: &str, fuzzy: bool) -> Vec<usize> {
        let normalized = normalize(text);
        let words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
        let mut found = Vec::new();
        let mut i = 0;
        while i < words.len() {
            let longest = self.max_words.min(words.len() - i);
            let exact = (1..=longest).rev().find_map(|len| {
                self.answers
                    .get(&words[i..i + len].join(" "))
                    .map(|&country| (country, len))
            });
            let hit = exact.or_else(|| {
                fuzzy
                    .then(|| {
                        (1..=longest).rev().find_map(|len| {
                            self.closest(&words[i..i + len].concat()).map(|c| (c, len))
                        })
                    })
                    .flatten()
            });
            match hit {
                Some((country, len)) => {
                    found.push(country);
                    i += len;
                }
                None => i += 1,
            }
        }
        found
    }

    /// Le pays dont un nom est le plus proche de `candidate` (sans espaces), s'il l'est assez.
    fn closest(&self, candidate: &str) -> Option<usize> {
        let len = candidate.chars().count();
        self.compact
            .iter()
            .filter_map(|(answer, country)| {
                let allowed = answer.chars().count() / 5;
                if allowed == 0 || len.abs_diff(answer.chars().count()) > allowed {
                    return None;
                }
                let d = levenshtein(candidate, answer);
                (d <= allowed).then_some((d, *country))
            })
            .min_by_key(|&(d, _)| d)
            .map(|(_, country)| country)
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::world_with;

    fn matcher() -> Matcher {
        Matcher::new(&world_with(&[
            ("GIN", &["guinee"]),
            ("GNQ", &["guinee equatoriale"]),
            ("FRA", &["france"]),
            ("CIV", &["cote d ivoire"]),
            ("USA", &["etats unis", "usa"]),
            ("OMN", &["oman"]),
            ("KNA", &["saint christophe et nieves"]),
        ]))
    }

    #[test]
    fn lookup_normalizes_input() {
        let m = matcher();
        assert_eq!(m.lookup("Côte d’Ivoire"), Some(3));
        assert_eq!(m.lookup("la France"), Some(2));
        assert_eq!(m.lookup("U.S.A."), None);
        assert_eq!(m.lookup("usa"), Some(4));
        assert_eq!(m.lookup("guin"), None);
    }

    #[test]
    fn scan_prefers_longest_names() {
        let m = matcher();
        assert_eq!(m.scan("guinée équatoriale france guinée"), vec![1, 2, 0]);
        assert_eq!(m.scan("euh la france et les états-unis"), vec![2, 4]);
        assert!(m.scan("rien du tout").is_empty());
    }

    #[test]
    fn fuzzy_scan_tolerates_small_mistakes() {
        let m = matcher();
        assert_eq!(m.scan_fuzzy("État-Unis."), vec![4]);
        assert_eq!(m.scan_fuzzy("Côte d'Ivoir, France"), vec![3, 2]);
        assert_eq!(m.scan_fuzzy("Saint-Christophé-Niévès"), vec![6]);
        // Les noms courts ne sont jamais devinés.
        assert!(m.scan_fuzzy("on m'a dit").is_empty());
        assert!(m.scan_fuzzy("omen").is_empty());
        assert!(m.scan_fuzzy("bonjour je ne sais pas").is_empty());
        // La recherche exacte ne tolère rien.
        assert!(m.scan("État-Unis").is_empty());
    }

    #[test]
    fn levenshtein_distance() {
        assert_eq!(levenshtein("chat", "chats"), 1);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn detects_names_that_can_grow() {
        let m = matcher();
        assert!(m.can_extend(0)); // guinée → guinée équatoriale
        assert!(!m.can_extend(1));
        assert!(!m.can_extend(2));
    }
}

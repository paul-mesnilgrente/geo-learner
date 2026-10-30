//! Reconnaissance des noms de pays saisis au clavier ou dictés.

use std::collections::HashMap;

use crate::normalize::normalize;
use crate::world::World;

pub struct Matcher {
    answers: HashMap<String, usize>,
    max_words: usize,
}

impl Matcher {
    pub fn new(world: &World) -> Self {
        let mut answers = HashMap::new();
        let mut max_words = 1;
        for (i, country) in world.countries.iter().enumerate() {
            for answer in &country.answers {
                max_words = max_words.max(answer.split(' ').count());
                answers.insert(answer.clone(), i);
            }
        }
        Self { answers, max_words }
    }

    /// Le pays dont le nom correspond exactement au texte (après normalisation).
    pub fn lookup(&self, text: &str) -> Option<usize> {
        self.answers.get(&normalize(text)).copied()
    }

    /// Tous les pays cités dans une phrase dictée, dans l'ordre, en privilégiant
    /// les noms les plus longs (« guinée équatoriale » plutôt que « guinée »).
    pub fn scan(&self, text: &str) -> Vec<usize> {
        let normalized = normalize(text);
        let words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
        let mut found = Vec::new();
        let mut i = 0;
        while i < words.len() {
            let longest = self.max_words.min(words.len() - i);
            let hit = (1..=longest).rev().find_map(|len| {
                self.answers
                    .get(&words[i..i + len].join(" "))
                    .map(|&country| (country, len))
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
}

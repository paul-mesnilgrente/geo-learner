//! Formulations qui dépendent des lieux du quiz : « ce pays », « cette région », « cet État »…

use geo_core::Info;

pub struct Words<'a> {
    one: &'a str,
    many: &'a str,
    feminine: bool,
}

impl<'a> Words<'a> {
    pub fn of(info: &'a Info) -> Self {
        Self {
            one: &info.one,
            many: &info.many,
            feminine: info.feminine,
        }
    }

    pub fn many(&self) -> &str {
        self.many
    }

    /// Le nom commence-t-il par une voyelle (élision : « l'État », « cet État ») ?
    fn vowel(&self) -> bool {
        self.one
            .chars()
            .next()
            .is_some_and(|c| "aeiouyàâéèêëîïôûAEIOUYÀÂÉÈÊËÎÏÔÛ".contains(c))
    }

    /// « le pays », « la région », « l'État ».
    pub fn the(&self) -> String {
        match (self.vowel(), self.feminine) {
            (true, _) => format!("l'{}", self.one),
            (false, true) => format!("la {}", self.one),
            (false, false) => format!("le {}", self.one),
        }
    }

    /// « du pays », « de la région », « de l'État ».
    pub fn of_the(&self) -> String {
        match (self.vowel(), self.feminine) {
            (true, _) => format!("de l'{}", self.one),
            (false, true) => format!("de la {}", self.one),
            (false, false) => format!("du {}", self.one),
        }
    }

    /// « ce pays », « cette région », « cet État ».
    pub fn this(&self) -> String {
        match (self.feminine, self.vowel()) {
            (true, _) => format!("cette {}", self.one),
            (false, true) => format!("cet {}", self.one),
            (false, false) => format!("ce {}", self.one),
        }
    }

    /// « un pays », « une région ».
    pub fn a(&self) -> String {
        format!("{} {}", if self.feminine { "une" } else { "un" }, self.one)
    }

    /// « d'un pays », « d'une région ».
    pub fn of_a(&self) -> String {
        format!("d'{}", self.a())
    }

    /// « tous les pays », « toutes les régions ».
    pub fn all(&self) -> String {
        format!(
            "{} les {}",
            if self.feminine { "toutes" } else { "tous" },
            self.many
        )
    }

    /// Accorde un adjectif ou participe avec les lieux : « trouvés » → « trouvées ».
    pub fn agree(&self, word: &str) -> String {
        if !self.feminine {
            return word.to_owned();
        }
        match word.strip_suffix('s') {
            Some(stem) => format!("{stem}es"),
            None => format!("{word}e"),
        }
    }
}

/// Première lettre en majuscule.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(one: &str, many: &str, feminine: bool) -> Info {
        Info {
            one: one.into(),
            many: many.into(),
            feminine,
            ..Default::default()
        }
    }

    #[test]
    fn french_articles_and_agreement() {
        let pays = info("pays", "pays", false);
        let region = info("région", "régions", true);
        let etat = info("État", "États", false);
        let (p, r, e) = (Words::of(&pays), Words::of(&region), Words::of(&etat));
        assert_eq!(
            [p.the(), r.the(), e.the()],
            ["le pays", "la région", "l'État"]
        );
        assert_eq!(
            [p.this(), r.this(), e.this()],
            ["ce pays", "cette région", "cet État"]
        );
        assert_eq!(
            [p.of_the(), r.of_the(), e.of_the()],
            ["du pays", "de la région", "de l'État"]
        );
        assert_eq!(r.of_a(), "d'une région");
        assert_eq!(r.all(), "toutes les régions");
        assert_eq!(
            [p.agree("trouvés"), r.agree("trouvés"), r.agree("inconnu")],
            ["trouvés", "trouvées", "inconnue"]
        );
    }
}

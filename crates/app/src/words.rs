//! Formulations qui dépendent des lieux du quiz et de la langue : « ce pays », « cette
//! région », « cet État » ; « this country », « a state or territory ».

use geo_core::Info;

use crate::i18n::Lang;

pub struct Words<'a> {
    lang: Lang,
    one: &'a str,
    many: &'a str,
    feminine: bool,
}

impl<'a> Words<'a> {
    pub fn of(info: &'a Info, lang: Lang) -> Self {
        Self {
            lang,
            one: info.one.get(lang),
            many: info.many.get(lang),
            feminine: info.feminine,
        }
    }

    pub fn one(&self) -> &str {
        self.one
    }

    pub fn many(&self) -> &str {
        self.many
    }

    /// Le nom commence-t-il par une voyelle (« l'État », « cet État », « an island ») ?
    fn vowel(&self) -> bool {
        self.one
            .chars()
            .next()
            .is_some_and(|c| "aeiouyàâéèêëîïôûAEIOUYÀÂÉÈÊËÎÏÔÛ".contains(c))
    }

    /// « le pays », « la région », « l'État » ; « the country ».
    pub fn the(&self) -> String {
        match (self.lang, self.vowel(), self.feminine) {
            (Lang::En, ..) => format!("the {}", self.one),
            (Lang::Fr, true, _) => format!("l'{}", self.one),
            (Lang::Fr, false, true) => format!("la {}", self.one),
            (Lang::Fr, false, false) => format!("le {}", self.one),
        }
    }

    /// « du pays », « de la région », « de l'État » ; « of the country ».
    pub fn of_the(&self) -> String {
        match (self.lang, self.vowel(), self.feminine) {
            (Lang::En, ..) => format!("of the {}", self.one),
            (Lang::Fr, true, _) => format!("de l'{}", self.one),
            (Lang::Fr, false, true) => format!("de la {}", self.one),
            (Lang::Fr, false, false) => format!("du {}", self.one),
        }
    }

    /// « ce pays », « cette région », « cet État » ; « this country ».
    pub fn this(&self) -> String {
        match (self.lang, self.feminine, self.vowel()) {
            (Lang::En, ..) => format!("this {}", self.one),
            (Lang::Fr, true, _) => format!("cette {}", self.one),
            (Lang::Fr, false, true) => format!("cet {}", self.one),
            (Lang::Fr, false, false) => format!("ce {}", self.one),
        }
    }

    /// « un pays », « une région » ; « a country », « an island ».
    pub fn a(&self) -> String {
        let article = match (self.lang, self.feminine, self.vowel()) {
            (Lang::En, _, true) => "an",
            (Lang::En, _, false) => "a",
            (Lang::Fr, true, _) => "une",
            (Lang::Fr, false, _) => "un",
        };
        format!("{article} {}", self.one)
    }

    /// « d'un pays », « d'une région » ; « of a country ».
    pub fn of_a(&self) -> String {
        match self.lang {
            Lang::Fr => format!("d'{}", self.a()),
            Lang::En => format!("of {}", self.a()),
        }
    }

    /// « tous les pays », « toutes les régions » ; « all countries ».
    pub fn all(&self) -> String {
        match (self.lang, self.feminine) {
            (Lang::En, _) => format!("all {}", self.many),
            (Lang::Fr, true) => format!("toutes les {}", self.many),
            (Lang::Fr, false) => format!("tous les {}", self.many),
        }
    }

    /// « Quel est ce pays ? », « Quelle est cette région ? » ; « Which country is this? ».
    pub fn which_is_this(&self) -> String {
        match self.lang {
            Lang::Fr => format!(
                "Quel{} est {} ?",
                if self.feminine { "le" } else { "" },
                self.this()
            ),
            Lang::En => format!("Which {} is this?", self.one),
        }
    }

    /// « Quel département porte le numéro… » ; « Which department has the number… ».
    pub fn which_has_number(&self) -> String {
        match self.lang {
            Lang::Fr => format!(
                "Quel{} {} porte le numéro…",
                if self.feminine { "le" } else { "" },
                self.one
            ),
            Lang::En => format!("Which {} has the number…", self.one),
        }
    }

    /// Participe ou adjectif accordé avec les lieux : (« trouvés », « found ») donne
    /// « trouvées » pour des régions, « found » en anglais.
    pub fn agree(&self, fr: &str, en: &str) -> String {
        if self.lang == Lang::En {
            return en.to_owned();
        }
        if !self.feminine {
            return fr.to_owned();
        }
        match fr.strip_suffix('s') {
            Some(stem) => format!("{stem}es"),
            None => format!("{fr}e"),
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
    use geo_core::Text;

    fn info(one: (&str, &str), many: (&str, &str), feminine: bool) -> Info {
        Info {
            one: Text::new(one.0, one.1),
            many: Text::new(many.0, many.1),
            feminine,
            ..Default::default()
        }
    }

    #[test]
    fn french_articles_and_agreement() {
        let pays = info(("pays", "country"), ("pays", "countries"), false);
        let region = info(("région", "region"), ("régions", "regions"), true);
        let etat = info(("État", "state"), ("États", "states"), false);
        let w = |i| Words::of(i, Lang::Fr);
        let (p, r, e) = (w(&pays), w(&region), w(&etat));
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
            [
                p.agree("trouvés", "found"),
                r.agree("trouvés", "found"),
                r.agree("inconnu", "unknown")
            ],
            ["trouvés", "trouvées", "inconnue"]
        );
    }

    #[test]
    fn english_articles() {
        let pays = info(("pays", "country"), ("pays", "countries"), false);
        let region = info(("région", "region"), ("régions", "regions"), true);
        let island = info(("île", "island"), ("îles", "islands"), true);
        let (p, r, i) = (
            Words::of(&pays, Lang::En),
            Words::of(&region, Lang::En),
            Words::of(&island, Lang::En),
        );
        assert_eq!([p.the(), r.this()], ["the country", "this region"]);
        assert_eq!([p.a(), i.a()], ["a country", "an island"]);
        assert_eq!(p.all(), "all countries");
        assert_eq!(r.agree("trouvés", "found"), "found");
    }
}

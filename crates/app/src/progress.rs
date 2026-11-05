//! Historique des parties, enregistré localement en JSON.

use std::collections::HashMap;
use std::path::PathBuf;

use geo_core::Mode;
use serde::{Deserialize, Serialize};

/// Dossier des données de l'application (progression, modèle vocal).
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("geo-learner")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Date de fin (secondes depuis l'epoch Unix).
    pub when: u64,
    /// Atlas joué (« monde », « fr-departements »…) ; les parties d'avant les atlas sont
    /// des parties du monde.
    #[serde(default = "world_atlas")]
    pub atlas: String,
    pub mode: Mode,
    /// Identifiant de la zone jouée.
    pub region: String,
    pub total: u32,
    pub found: u32,
    pub perfect: u32,
    pub score: f32,
    pub seconds: u32,
    /// Codes des pays ratés.
    #[serde(default)]
    pub missed: Vec<String>,
    /// Durée du minuteur (secondes), s'il était activé.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Partie de révision (quelques lieux de la zone) : elle compte pour la maîtrise, pas dans
    /// les séries ni les records.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub review: bool,
    /// Détail par lieu, dans l'ordre du jeu (absent des parties enregistrées avant son ajout).
    /// Il vit dans la partie : la supprimer l'efface aussi de la maîtrise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<Attempt>,
}

/// Bilan d'un lieu dans une partie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub code: String,
    /// Essai de la bonne réponse (1 = du premier coup) ; absent si raté.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tries: Option<u8>,
    /// Temps de réponse en millisecondes (mode libre : moment où le lieu a été cité).
    pub ms: u32,
    /// Lieux donnés à tort à la place de celui-ci.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrong: Vec<String>,
}

/// Résultat d'un lieu lors d'une partie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trial {
    /// Trouvé du premier coup.
    Clean,
    /// Trouvé après une ou deux erreurs.
    Shaky,
    Missed,
}

/// Niveau de maîtrise d'un lieu, d'après ses dernières parties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Raté la dernière fois.
    Weak,
    /// Trouvé la dernière fois, mais pas encore deux fois de suite du premier coup.
    Learning,
    /// Trouvé du premier coup lors des deux dernières parties.
    Known,
}

/// Ce qu'on sait d'un lieu dans un atlas et un mode, toutes zones confondues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mastery {
    pub level: Level,
    /// Parties où le lieu a été joué, et parmi elles celles où il a été raté.
    pub seen: u32,
    pub missed: u32,
    /// Date de la dernière partie où il a été joué.
    pub last: u64,
}

impl Session {
    /// La partie s'est arrêtée faute de temps.
    pub fn timed_out(&self) -> bool {
        self.limit.is_some_and(|limit| self.seconds >= limit)
    }

    /// La partie fait partie de la série (atlas, mode, zone) ; les révisions n'en font pas partie.
    pub fn is(&self, atlas: &str, mode: Mode, region: &str) -> bool {
        !self.review && self.atlas == atlas && self.mode == mode && self.region == region
    }
}

fn world_atlas() -> String {
    "monde".into()
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Progress {
    pub sessions: Vec<Session>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Progress {
    pub fn load() -> Self {
        let path = data_dir().join("progress.json");
        let mut progress: Progress = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        progress.path = Some(path);
        progress
    }

    pub fn record(&mut self, session: Session) {
        self.sessions.push(session);
        self.persist();
    }

    /// Supprime une partie (index dans `sessions`) et la renvoie, pour pouvoir l'annuler.
    pub fn remove(&mut self, index: usize) -> Option<Session> {
        if index >= self.sessions.len() {
            return None;
        }
        let session = self.sessions.remove(index);
        self.persist();
        Some(session)
    }

    /// Remet une partie supprimée à sa place.
    pub fn restore(&mut self, index: usize, session: Session) {
        let index = index.min(self.sessions.len());
        self.sessions.insert(index, session);
        self.persist();
    }

    /// Progression en JSON, pour l'exporter (même format que `progress.json`).
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Lit une progression exportée.
    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Ajoute les parties d'une progression importée qui manquent ici (une partie est reconnue
    /// à sa date, son quiz, son mode, sa zone et son score), dans l'ordre chronologique.
    /// Renvoie le nombre de parties ajoutées et celui des parties déjà présentes.
    pub fn merge(&mut self, other: Progress) -> (usize, usize) {
        let key = |s: &Session| {
            (
                s.when,
                s.atlas.clone(),
                s.mode,
                s.region.clone(),
                s.total,
                s.found,
                s.seconds,
            )
        };
        let known: std::collections::HashSet<_> = self.sessions.iter().map(key).collect();
        let total = other.sessions.len();
        let mut added = 0;
        for session in other.sessions {
            if !known.contains(&key(&session)) {
                self.sessions.push(session);
                added += 1;
            }
        }
        if added > 0 {
            // Tri stable : l'ordre des parties d'une même seconde est gardé.
            self.sessions.sort_by_key(|s| s.when);
            self.persist();
        }
        (added, total - added)
    }

    fn persist(&self) {
        if let Err(e) = self.save() {
            eprintln!("impossible d'enregistrer la progression : {e}");
        }
    }

    /// Maîtrise de chaque lieu déjà joué dans un atlas et un mode. Toutes les zones comptent :
    /// la France jouée dans « Europe » ou dans « Tous les pays », c'est le même savoir.
    /// Les parties d'avant le détail par lieu ne comptent que par leurs lieux ratés.
    pub fn mastery(&self, atlas: &str, mode: Mode) -> HashMap<String, Mastery> {
        let mut sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| s.atlas == atlas && s.mode == mode)
            .collect();
        sessions.sort_by_key(|s| s.when);
        // Résultats successifs de chaque lieu, du plus ancien au plus récent.
        let mut history: HashMap<&str, (Vec<Trial>, u64)> = HashMap::new();
        for s in sessions {
            let results: Vec<(&str, Trial)> = if s.answers.is_empty() {
                s.missed
                    .iter()
                    .map(|c| (c.as_str(), Trial::Missed))
                    .collect()
            } else {
                s.answers
                    .iter()
                    .map(|a| {
                        let trial = match a.tries {
                            Some(1) => Trial::Clean,
                            Some(_) => Trial::Shaky,
                            None => Trial::Missed,
                        };
                        (a.code.as_str(), trial)
                    })
                    .collect()
            };
            for (code, trial) in results {
                let entry = history.entry(code).or_default();
                entry.0.push(trial);
                entry.1 = s.when;
            }
        }
        history
            .into_iter()
            .map(|(code, (results, last))| {
                let level = match results.as_slice() {
                    [.., Trial::Missed] => Level::Weak,
                    [.., Trial::Clean, Trial::Clean] => Level::Known,
                    _ => Level::Learning,
                };
                let missed = results.iter().filter(|&&r| r == Trial::Missed).count();
                let mastery = Mastery {
                    level,
                    seen: results.len() as u32,
                    missed: missed as u32,
                    last,
                };
                (code.to_owned(), mastery)
            })
            .collect()
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        // Compact : le détail par lieu rendrait le fichier très long en format indenté.
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(tmp, path)
    }

    /// Meilleure partie pour un mode et une zone : meilleur score, puis temps le plus court.
    pub fn best(&self, atlas: &str, mode: Mode, region: &str) -> Option<&Session> {
        self.sessions
            .iter()
            .filter(|s| s.is(atlas, mode, region))
            .max_by(|a, b| a.score.total_cmp(&b.score).then(b.seconds.cmp(&a.seconds)))
    }

    pub fn recent(&self, count: usize) -> impl Iterator<Item = &Session> {
        self.sessions.iter().rev().take(count)
    }

    /// Parties d'une série (atlas, mode, zone), de la plus ancienne à la plus récente :
    /// (index dans `sessions`, partie).
    pub fn runs(&self, atlas: &str, mode: Mode, region: &str) -> Vec<(usize, &Session)> {
        let mut runs: Vec<(usize, &Session)> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is(atlas, mode, region))
            .collect();
        runs.sort_by_key(|(_, s)| s.when);
        runs
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(mode: Mode, region: &str, score: f32, seconds: u32) -> Session {
        Session {
            when: 0,
            atlas: "monde".into(),
            mode,
            region: region.into(),
            total: 10,
            found: 10,
            perfect: 10,
            score,
            seconds,
            missed: vec![],
            limit: None,
            review: false,
            answers: vec![],
        }
    }

    /// Importer ajoute les parties manquantes, dans l'ordre chronologique, sans doublons ; un
    /// export relu redonne la même progression.
    #[test]
    fn import_merges_missing_sessions() {
        let at = |when: u64, region: &str| Session {
            when,
            ..session(Mode::Free, region, 1.0, 60)
        };
        let mut here = Progress {
            sessions: vec![at(10, "europe"), at(30, "asie")],
            path: None,
        };
        let other = Progress {
            sessions: vec![at(10, "europe"), at(20, "afrique"), at(40, "oceanie")],
            path: None,
        };
        let back = Progress::from_json(&other.to_json()).unwrap();
        assert_eq!(back.sessions.len(), 3);
        assert_eq!(here.merge(back), (2, 1));
        let order: Vec<&str> = here.sessions.iter().map(|s| s.region.as_str()).collect();
        assert_eq!(order, ["europe", "afrique", "asie", "oceanie"]);
        assert!(Progress::from_json(b"{\"pas\": 1}").is_err());
    }

    fn attempt(code: &str, tries: Option<u8>) -> Attempt {
        Attempt {
            code: code.into(),
            tries,
            ms: 1000,
            wrong: vec![],
        }
    }

    #[test]
    fn mastery_follows_latest_runs_across_zones() {
        let mut p = Progress::default();
        let mut run = |when: u64, region: &str, answers: Vec<Attempt>| {
            let mut s = session(Mode::Locate, region, 0.5, 100);
            s.when = when;
            s.answers = answers;
            p.sessions.push(s);
        };
        // Ajoutées dans le désordre : seule la date compte.
        run(
            3,
            "europe",
            vec![attempt("FRA", Some(1)), attempt("SVN", Some(2))],
        );
        run(
            1,
            "monde",
            vec![attempt("FRA", Some(1)), attempt("SVN", None)],
        );
        run(
            2,
            "monde",
            vec![attempt("SVN", Some(1)), attempt("SVK", None)],
        );
        // Autre mode : ignoré.
        p.sessions.push(Session {
            answers: vec![attempt("SVK", Some(1))],
            ..session(Mode::Free, "europe", 1.0, 10)
        });
        // Partie d'avant le détail par lieu : seuls ses ratés comptent.
        p.sessions.push(Session {
            when: 0,
            missed: vec!["BTN".into()],
            ..session(Mode::Locate, "asie", 0.5, 100)
        });
        let m = p.mastery("monde", Mode::Locate);
        assert_eq!(m["FRA"].level, Level::Known);
        assert_eq!(m["SVN"].level, Level::Learning);
        assert_eq!((m["SVN"].seen, m["SVN"].missed, m["SVN"].last), (3, 1, 3));
        assert_eq!(m["SVK"].level, Level::Weak);
        assert_eq!(m["BTN"].level, Level::Weak);
        assert_eq!(m.len(), 4);
        // Supprimer une partie retire son détail : la Slovaquie n'a plus été jouée.
        let index = p.sessions.iter().position(|s| s.when == 2).unwrap();
        p.remove(index);
        let m = p.mastery("monde", Mode::Locate);
        assert!(!m.contains_key("SVK"));
        assert_eq!(m["SVN"].level, Level::Learning);
    }

    #[test]
    fn reviews_count_for_mastery_only() {
        let mut p = Progress::default();
        p.sessions.push(session(Mode::Locate, "europe", 0.5, 100));
        p.sessions.push(Session {
            when: 1,
            review: true,
            answers: vec![attempt("SVN", None)],
            ..session(Mode::Locate, "europe", 1.0, 10)
        });
        assert_eq!(p.runs("monde", Mode::Locate, "europe").len(), 1);
        assert_eq!(p.best("monde", Mode::Locate, "europe").unwrap().score, 0.5);
        assert_eq!(p.mastery("monde", Mode::Locate)["SVN"].level, Level::Weak);
    }

    #[test]
    fn attempts_serialize_compactly() {
        let a = Attempt {
            wrong: vec!["SVK".into()],
            ..attempt("SVN", None)
        };
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, r#"{"code":"SVN","ms":1000,"wrong":["SVK"]}"#);
        assert_eq!(serde_json::from_str::<Attempt>(&json).unwrap(), a);
    }

    #[test]
    fn best_prefers_score_then_time() {
        let mut p = Progress::default();
        p.sessions.push(session(Mode::Free, "europe", 0.8, 100));
        p.sessions.push(session(Mode::Free, "europe", 1.0, 300));
        p.sessions.push(session(Mode::Free, "europe", 1.0, 200));
        p.sessions.push(session(Mode::Locate, "europe", 1.0, 10));
        let best = p.best("monde", Mode::Free, "europe").unwrap();
        assert_eq!((best.score, best.seconds), (1.0, 200));
        assert!(p.best("monde", Mode::Identify, "europe").is_none());
    }

    #[test]
    fn runs_and_removal() {
        let mut p = Progress::default();
        p.sessions.push(session(Mode::Free, "europe", 0.5, 100));
        p.sessions.push(session(Mode::Free, "afrique", 0.6, 100));
        p.sessions.push(session(Mode::Free, "europe", 0.7, 100));
        p.sessions.push(session(Mode::Locate, "europe", 0.8, 100));
        let runs: Vec<usize> = p
            .runs("monde", Mode::Free, "europe")
            .iter()
            .map(|r| r.0)
            .collect();
        assert_eq!(runs, [0, 2]);
        // Suppression puis annulation : la partie retrouve sa place.
        let removed = p.remove(2).unwrap();
        assert_eq!(p.runs("monde", Mode::Free, "europe").len(), 1);
        p.restore(2, removed);
        assert_eq!(p.sessions[2].score, 0.7);
        assert!(p.remove(10).is_none());
    }
}

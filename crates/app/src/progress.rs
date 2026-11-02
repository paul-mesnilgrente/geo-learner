//! Historique des parties, enregistré localement en JSON.

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
}

impl Session {
    /// La partie s'est arrêtée faute de temps.
    pub fn timed_out(&self) -> bool {
        self.limit.is_some_and(|limit| self.seconds >= limit)
    }

    pub fn is(&self, atlas: &str, mode: Mode, region: &str) -> bool {
        self.atlas == atlas && self.mode == mode && self.region == region
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

    fn persist(&self) {
        if let Err(e) = self.save() {
            eprintln!("impossible d'enregistrer la progression : {e}");
        }
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
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

    /// Séries jouées dans un atlas : (mode, zone, nombre de parties).
    pub fn series(&self, atlas: &str) -> Vec<(Mode, &str, usize)> {
        let mut list: Vec<(Mode, &str, usize)> = Vec::new();
        for s in self.sessions.iter().filter(|s| s.atlas == atlas) {
            match list
                .iter_mut()
                .find(|(m, r, _)| *m == s.mode && *r == s.region)
            {
                Some(entry) => entry.2 += 1,
                None => list.push((s.mode, &s.region, 1)),
            }
        }
        list
    }

    /// Lieux les plus souvent ratés dans les parties retenues (code, nombre d'échecs).
    pub fn most_missed(
        &self,
        keep: impl Fn(&Session) -> bool,
        count: usize,
    ) -> Vec<(String, usize)> {
        let mut tally: std::collections::HashMap<&str, usize> = Default::default();
        for s in self.sessions.iter().filter(|s| keep(s)) {
            for code in &s.missed {
                *tally.entry(code).or_default() += 1;
            }
        }
        let mut list: Vec<(String, usize)> =
            tally.into_iter().map(|(c, n)| (c.to_owned(), n)).collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        list.truncate(count);
        list
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
        }
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
    fn series_runs_and_removal() {
        let mut p = Progress::default();
        p.sessions.push(session(Mode::Free, "europe", 0.5, 100));
        p.sessions.push(session(Mode::Free, "afrique", 0.6, 100));
        p.sessions.push(session(Mode::Free, "europe", 0.7, 100));
        p.sessions.push(session(Mode::Locate, "europe", 0.8, 100));
        assert_eq!(
            p.series("monde"),
            [
                (Mode::Free, "europe", 2),
                (Mode::Free, "afrique", 1),
                (Mode::Locate, "europe", 1)
            ]
        );
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

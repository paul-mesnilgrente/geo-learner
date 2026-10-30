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
    pub fn best(&self, mode: Mode, region: &str) -> Option<&Session> {
        self.sessions
            .iter()
            .filter(|s| s.mode == mode && s.region == region)
            .max_by(|a, b| a.score.total_cmp(&b.score).then(b.seconds.cmp(&a.seconds)))
    }

    pub fn recent(&self, count: usize) -> impl Iterator<Item = &Session> {
        self.sessions.iter().rev().take(count)
    }

    /// Pays les plus souvent ratés (code, nombre d'échecs).
    pub fn most_missed(&self, count: usize) -> Vec<(String, usize)> {
        let mut tally: std::collections::HashMap<&str, usize> = Default::default();
        for s in &self.sessions {
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
            mode,
            region: region.into(),
            total: 10,
            found: 10,
            perfect: 10,
            score,
            seconds,
            missed: vec![],
        }
    }

    #[test]
    fn best_prefers_score_then_time() {
        let mut p = Progress::default();
        p.sessions.push(session(Mode::Free, "europe", 0.8, 100));
        p.sessions.push(session(Mode::Free, "europe", 1.0, 300));
        p.sessions.push(session(Mode::Free, "europe", 1.0, 200));
        p.sessions.push(session(Mode::Locate, "europe", 1.0, 10));
        let best = p.best(Mode::Free, "europe").unwrap();
        assert_eq!((best.score, best.seconds), (1.0, 200));
        assert!(p.best(Mode::Identify, "europe").is_none());
    }
}

//! Réglages de l'utilisateur (minuteur), enregistrés localement en JSON.

use std::collections::BTreeMap;
use std::path::PathBuf;

use geo_core::Mode;
use serde::{Deserialize, Serialize};

use crate::progress::data_dir;

/// Bornes et pas du réglage de durée, en secondes.
const MIN_DURATION: u32 = 30;
const MAX_DURATION: u32 = 2 * 3600;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Minuteur activé.
    #[serde(default)]
    pub timer: bool,
    /// Avancement de la partie affiché par continent plutôt que par sous-région.
    #[serde(default)]
    pub by_continent: bool,
    /// Durées choisies par l'utilisateur (secondes), par atlas, mode et zone
    /// (`"monde/Free/europe"`).
    #[serde(default)]
    durations: BTreeMap<String, u32>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

/// Durée par défaut : 15 minutes pour citer les 197 pays du monde (comme JetPunk), au prorata
/// du nombre de pays ; davantage par pays quand il faut le reconnaître ou le chercher sur la carte.
pub fn default_duration(mode: Mode, countries: usize) -> u32 {
    let per_country = match mode {
        Mode::Free => 900.0 / 197.0,
        Mode::Identify => 7.0,
        Mode::Locate => 9.0,
    };
    let seconds = (countries as f32 * per_country / 30.0).ceil() as u32 * 30;
    seconds.clamp(60, MAX_DURATION)
}

/// Pas du réglage : 30 s pour les durées courtes, 1 min au-delà de 5 min.
fn step(seconds: u32, up: bool) -> u32 {
    if seconds < 300 || (seconds == 300 && !up) {
        30
    } else {
        60
    }
}

fn key(atlas: &str, mode: Mode, region: &str) -> String {
    format!("{atlas}/{mode:?}/{region}")
}

impl Settings {
    pub fn load() -> Self {
        let path = data_dir().join("settings.json");
        let mut settings: Settings = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        settings.path = Some(path);
        settings
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Durée du minuteur (secondes) pour ce mode et cette zone.
    pub fn duration(&self, atlas: &str, mode: Mode, region: &str, countries: usize) -> u32 {
        self.durations
            .get(&key(atlas, mode, region))
            .copied()
            .unwrap_or_else(|| default_duration(mode, countries))
    }

    pub fn is_custom(&self, atlas: &str, mode: Mode, region: &str) -> bool {
        self.durations.contains_key(&key(atlas, mode, region))
    }

    pub fn set_by_continent(&mut self, by_continent: bool) {
        self.by_continent = by_continent;
        self.save();
    }

    pub fn toggle_timer(&mut self) {
        self.timer = !self.timer;
        self.save();
    }

    /// Allonge (`up`) ou raccourcit la durée d'un pas.
    pub fn step_duration(
        &mut self,
        atlas: &str,
        mode: Mode,
        region: &str,
        countries: usize,
        up: bool,
    ) {
        let current = self.duration(atlas, mode, region, countries);
        let next = if up {
            current + step(current, true)
        } else {
            current.saturating_sub(step(current, false))
        };
        let next = next.clamp(MIN_DURATION, MAX_DURATION);
        if next == default_duration(mode, countries) {
            self.durations.remove(&key(atlas, mode, region));
        } else {
            self.durations.insert(key(atlas, mode, region), next);
        }
        self.save();
    }

    pub fn reset_duration(&mut self, atlas: &str, mode: Mode, region: &str) {
        self.durations.remove(&key(atlas, mode, region));
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_scale_with_the_number_of_countries() {
        assert_eq!(default_duration(Mode::Free, 197), 15 * 60);
        assert_eq!(default_duration(Mode::Free, 46), 4 * 60); // Europe : 3 min 30 → 4 min
        assert_eq!(default_duration(Mode::Free, 9), 60); // minimum 1 min
        assert_eq!(default_duration(Mode::Identify, 46), 330);
        assert_eq!(default_duration(Mode::Locate, 197), 1800);
    }

    #[test]
    fn steps_and_custom_durations() {
        let mut s = Settings::default();
        assert_eq!(s.duration("monde", Mode::Free, "europe", 46), 240);
        s.step_duration("monde", Mode::Free, "europe", 46, true);
        assert_eq!(s.duration("monde", Mode::Free, "europe", 46), 270);
        assert!(s.is_custom("monde", Mode::Free, "europe"));
        // Revenir à la valeur par défaut efface le réglage personnalisé.
        s.step_duration("monde", Mode::Free, "europe", 46, false);
        assert!(!s.is_custom("monde", Mode::Free, "europe"));
        // Au-delà de 5 min, le pas est d'une minute.
        assert_eq!(s.duration("monde", Mode::Free, "monde", 197), 900);
        s.step_duration("monde", Mode::Free, "monde", 197, true);
        assert_eq!(s.duration("monde", Mode::Free, "monde", 197), 960);
        s.reset_duration("monde", Mode::Free, "monde");
        assert_eq!(s.duration("monde", Mode::Free, "monde", 197), 900);
    }
}

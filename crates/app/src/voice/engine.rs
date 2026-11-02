//! Transcription d'une phrase (audio 16 kHz) par Vosk (via `utter`), restreinte aux noms de pays.

use std::sync::OnceLock;

use super::audio::RATE;
use super::models;
use geo_core::World;

/// Coût du chemin « mot inconnu » : plus il est bas, plus les paroles hors liste sont rejetées
/// (au risque de manquer de vrais noms de pays). -3 a donné le meilleur compromis aux essais.
const UNKNOWN_COST: f32 = -3.0;

static MODEL: OnceLock<Result<utter::Model, String>> = OnceLock::new();

pub struct Vosk {
    model: &'static utter::Model,
    grammar: Vec<String>,
}

impl Vosk {
    /// Modèle chargé, avec une grammaire limitée aux noms des lieux de l'atlas.
    pub fn load(world: &World) -> Result<Self, String> {
        if !models::installed() {
            return Err("Le modèle vocal n'est pas installé.".into());
        }
        let model = MODEL
            .get_or_init(|| {
                utter::Model::open(&models::path())
                    .map_err(|e| format!("Modèle vocal illisible : {e}"))
            })
            .as_ref()
            .map_err(Clone::clone)?;
        // Formes orales des pays dont tous les mots sont connus du modèle.
        let mut grammar: Vec<String> = Vec::new();
        for country in &world.countries {
            for form in &country.spoken {
                if form.split(' ').all(|w| model.word_ids.contains_key(w))
                    && !grammar.contains(form)
                {
                    grammar.push(form.clone());
                }
            }
        }
        Ok(Self { model, grammar })
    }

    pub fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
        let options = utter::RecognizerOptions {
            unknown_cost: Some(UNKNOWN_COST),
            ..Default::default()
        };
        let mut recognizer =
            utter::Recognizer::with_options(self.model, RATE as f32, &self.grammar, &options)
                .map_err(|e| format!("Reconnaissance vocale indisponible : {e}"))?;
        recognizer.accept_f32(audio).map_err(|e| format!("{e:?}"))?;
        Ok(field(recognizer.final_result(), "text"))
    }
}

/// Extrait un champ texte du JSON produit par le décodeur, sans les marqueurs `[sil]`, `[unk]`…
fn field(json: &str, key: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.get(key).and_then(|t| t.as_str()).map(str::to_owned))
        .unwrap_or_default()
        .split_whitespace()
        .filter(|w| !w.starts_with('['))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_drops_markers() {
        assert_eq!(
            field(r#"{"text": "france [unk] italie"}"#, "text"),
            "france italie"
        );
        assert_eq!(field(r#"{"partial": "[sil]"}"#, "partial"), "");
        assert_eq!(field("pas du json", "text"), "");
    }

    /// Transcrit les fichiers WAV 16 kHz d'un dossier :
    /// `GEO_VOICE_SAMPLES=dossier cargo test -p geo-learner --release -- --ignored samples --nocapture`
    #[test]
    #[ignore]
    fn samples() {
        let dir = std::env::var("GEO_VOICE_SAMPLES").expect("GEO_VOICE_SAMPLES");
        let world = &crate::ATLASES[0];
        let matcher = geo_core::Matcher::new(world);
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "wav"))
            .collect();
        files.sort();
        let mut vosk = Vosk::load(world).unwrap();
        for file in &files {
            let bytes = std::fs::read(file).unwrap();
            let audio: Vec<f32> = bytes[44..]
                .chunks(2)
                .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
                .collect();
            let start = std::time::Instant::now();
            let text = vosk.transcribe(&audio).unwrap();
            let names: Vec<&str> = matcher
                .scan_fuzzy(&text)
                .iter()
                .map(|&c| world.countries[c].name.as_str())
                .collect();
            println!(
                "{:>24} {:>5} ms  {text:?} → {names:?}",
                file.file_name().unwrap().to_string_lossy(),
                start.elapsed().as_millis()
            );
        }
    }
}

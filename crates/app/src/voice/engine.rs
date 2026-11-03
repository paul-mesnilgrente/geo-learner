//! Transcription d'une phrase (audio 16 kHz) par Vosk (via `utter`), restreinte aux noms de pays.

use std::sync::OnceLock;

use super::audio::RATE;
use super::models;
use crate::i18n::Lang;
use crate::tr;
use geo_core::World;

/// Coût du chemin « mot inconnu » : plus il est bas, plus les paroles hors liste sont rejetées
/// (au risque de manquer de vrais noms de pays). -3 a donné le meilleur compromis aux essais.
const UNKNOWN_COST: f32 = -3.0;

/// Modèles chargés, un par langue (dans l'ordre de `Lang::ALL`).
static MODELS: [OnceLock<Result<utter::Model, String>>; 2] = [OnceLock::new(), OnceLock::new()];

pub struct Vosk {
    lang: Lang,
    model: &'static utter::Model,
    grammar: Vec<String>,
}

impl Vosk {
    /// Modèle de la langue chargé, avec une grammaire limitée aux noms des lieux de l'atlas
    /// dans cette langue.
    pub fn load(world: &World, lang: Lang) -> Result<Self, String> {
        if !models::installed(lang) {
            return Err(tr!(
                lang,
                "Le modèle vocal n'est pas installé.",
                "The voice model is not installed."
            ));
        }
        let model = MODELS[lang.index()]
            .get_or_init(|| {
                utter::Model::open(&models::path(lang)).map_err(|e| {
                    tr!(
                        lang,
                        "Modèle vocal illisible : {e}",
                        "Unreadable voice model: {e}"
                    )
                })
            })
            .as_ref()
            .map_err(Clone::clone)?;
        // Formes orales des pays dont tous les mots sont connus du modèle.
        let mut grammar: Vec<String> = Vec::new();
        let none = Vec::new();
        for country in &world.countries {
            for form in country.spoken.get(lang.index()).unwrap_or(&none) {
                if form.split(' ').all(|w| model.word_ids.contains_key(w))
                    && !grammar.contains(form)
                {
                    grammar.push(form.clone());
                }
            }
        }
        Ok(Self {
            lang,
            model,
            grammar,
        })
    }

    pub fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
        let options = utter::RecognizerOptions {
            unknown_cost: Some(UNKNOWN_COST),
            ..Default::default()
        };
        let mut recognizer =
            utter::Recognizer::with_options(self.model, RATE as f32, &self.grammar, &options)
                .map_err(|e| {
                    tr!(
                        self.lang,
                        "Reconnaissance vocale indisponible : {e}",
                        "Speech recognition unavailable: {e}"
                    )
                })?;
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

    /// Part des noms de chaque quiz que le modèle de la langue sait reconnaître (tous leurs
    /// mots dans son vocabulaire) :
    /// `GEO_VOICE_LANG=en cargo test -p geo-learner --release -- --ignored coverage --nocapture`
    #[test]
    #[ignore]
    fn coverage() {
        let lang = std::env::var("GEO_VOICE_LANG")
            .ok()
            .and_then(|l| Lang::from_locale(&l))
            .unwrap_or(Lang::Fr);
        for world in crate::ATLASES.iter() {
            let vosk = Vosk::load(world, lang).unwrap();
            let heard = |c: &geo_core::Country| {
                c.spoken[lang.index()]
                    .iter()
                    .any(|f| vosk.grammar.contains(f))
            };
            let missing: Vec<&str> = world
                .countries
                .iter()
                .filter(|c| !heard(c))
                .map(|c| c.name.get(lang))
                .collect();
            println!(
                "{} : {}/{} ; absents : {missing:?}",
                world.info.title.get(lang),
                world.countries.len() - missing.len(),
                world.countries.len()
            );
        }
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
        let lang = std::env::var("GEO_VOICE_LANG")
            .ok()
            .and_then(|l| Lang::from_locale(&l))
            .unwrap_or(Lang::Fr);
        let mut vosk = Vosk::load(world, lang).unwrap();
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
                .map(|&c| world.countries[c].name.get(lang))
                .collect();
            println!(
                "{:>24} {:>5} ms  {text:?} → {names:?}",
                file.file_name().unwrap().to_string_lossy(),
                start.elapsed().as_millis()
            );
        }
    }
}

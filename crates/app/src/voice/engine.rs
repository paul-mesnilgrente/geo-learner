//! Moteurs de transcription : une phrase (audio 16 kHz) en entrée, du texte en sortie.

use std::sync::OnceLock;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use super::audio::RATE;
use super::models::Engine;
use crate::WORLD;

pub trait Transcriber: Send {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String>;
}

pub fn load(engine: Engine) -> Result<Box<dyn Transcriber>, String> {
    if !engine.installed() {
        return Err(format!("Le modèle {} n'est pas installé.", engine.label()));
    }
    match engine {
        Engine::WhisperSmall | Engine::WhisperBase => Ok(Box::new(Whisper::load(engine)?)),
        Engine::Vosk => Ok(Box::new(Vosk::load()?)),
    }
}

const PROMPT: &str = "Liste de pays.";

/// Whisper (whisper.cpp), transcription libre suivie d'une recherche des noms de pays.
struct Whisper {
    // Le contexte doit vivre aussi longtemps que l'état qui en dépend.
    _context: WhisperContext,
    state: WhisperState,
    threads: i32,
}

impl Whisper {
    fn load(engine: Engine) -> Result<Self, String> {
        whisper_rs::install_logging_hooks();
        let path = engine.path();
        let context = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
            .map_err(|e| format!("Modèle Whisper illisible : {e}"))?;
        let state = context.create_state().map_err(|e| e.to_string())?;
        // On laisse un cœur libre pour l'interface et la capture audio.
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .saturating_sub(1)
            .clamp(1, 8);
        Ok(Self {
            _context: context,
            state,
            threads: threads as i32,
        })
    }
}

impl Transcriber for Whisper {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
        // whisper.cpp refuse les extraits de moins d'une seconde : on complète par du silence.
        let mut padded = audio.to_vec();
        let minimum = RATE as usize * 11 / 10;
        if padded.len() < minimum {
            padded.resize(minimum, 0.0);
        }

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("fr"));
        params.set_n_threads(self.threads);
        params.set_no_context(true);
        params.set_no_timestamps(true);
        params.set_single_segment(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_max_tokens(40);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        // Un contexte audio réduit à la durée réelle accélère beaucoup les phrases courtes
        // (le modèle est entraîné sur des fenêtres de 30 s, soit 1500 positions).
        let seconds = padded.len() as f32 / RATE as f32;
        params.set_audio_ctx(((seconds * 50.0) as i32 + 64).min(1500));
        // Amorce courte : oriente vers une liste de noms, sans citer de pays (le modèle a
        // tendance à recopier ceux de l'amorce). `GEO_WHISPER_PROMPT` permet d'en essayer d'autres.
        let prompt = std::env::var("GEO_WHISPER_PROMPT").unwrap_or_else(|_| PROMPT.into());
        if !prompt.is_empty() {
            params.set_initial_prompt(&prompt);
        }

        self.state
            .full(params, &padded)
            .map_err(|e| e.to_string())?;
        let mut text = String::new();
        for segment in self.state.as_iter() {
            if segment.no_speech_probability() > 0.6 {
                continue;
            }
            if let Ok(s) = segment.to_str_lossy() {
                text.push_str(&s);
                text.push(' ');
            }
        }
        Ok(text.trim().to_owned())
    }
}

/// Vosk (via `utter`), restreint aux noms de pays.
struct Vosk {
    model: &'static utter::Model,
    grammar: Vec<String>,
}

/// Coût du chemin « mot inconnu » : plus il est bas, plus les paroles hors liste sont rejetées
/// (au risque de manquer de vrais noms de pays). -3 a donné le meilleur compromis aux essais.
const UNKNOWN_COST: f32 = -3.0;

static VOSK_MODEL: OnceLock<Result<utter::Model, String>> = OnceLock::new();

impl Vosk {
    fn load() -> Result<Self, String> {
        let model = VOSK_MODEL
            .get_or_init(|| {
                utter::Model::open(&Engine::Vosk.path())
                    .map_err(|e| format!("Modèle vocal illisible : {e}"))
            })
            .as_ref()
            .map_err(Clone::clone)?;
        // Formes orales des pays dont tous les mots sont connus du modèle.
        let mut grammar: Vec<String> = Vec::new();
        for country in &WORLD.countries {
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
}

impl Transcriber for Vosk {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
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

    /// Transcrit les fichiers WAV 16 kHz d'un dossier avec chaque moteur installé :
    /// `GEO_VOICE_SAMPLES=dossier cargo test -p geo-learner --release -- --ignored samples --nocapture`
    #[test]
    #[ignore]
    fn samples() {
        let dir = std::env::var("GEO_VOICE_SAMPLES").expect("GEO_VOICE_SAMPLES");
        let matcher = geo_core::Matcher::new(&WORLD);
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "wav"))
            .collect();
        files.sort();
        for engine in Engine::ALL.into_iter().filter(|e| e.installed()) {
            println!("=== {}", engine.label());
            let mut transcriber = load(engine).unwrap();
            for file in &files {
                let bytes = std::fs::read(file).unwrap();
                let audio: Vec<f32> = bytes[44..]
                    .chunks(2)
                    .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
                    .collect();
                let start = std::time::Instant::now();
                let text = transcriber.transcribe(&audio).unwrap();
                let names: Vec<&str> = matcher
                    .scan_fuzzy(&text)
                    .iter()
                    .map(|&c| WORLD.countries[c].name.as_str())
                    .collect();
                println!(
                    "{:>24} {:>5} ms  {text:?} → {names:?}",
                    file.file_name().unwrap().to_string_lossy(),
                    start.elapsed().as_millis()
                );
            }
        }
    }
}

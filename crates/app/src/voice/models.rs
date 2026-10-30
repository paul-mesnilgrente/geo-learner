//! Moteurs de reconnaissance proposés, emplacement de leurs modèles et téléchargement.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::progress::data_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Engine {
    WhisperSmall,
    WhisperBase,
    Vosk,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::WhisperSmall, Engine::WhisperBase, Engine::Vosk];

    pub fn label(self) -> &'static str {
        match self {
            Engine::WhisperSmall => "Whisper small",
            Engine::WhisperBase => "Whisper base",
            Engine::Vosk => "Vosk",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Engine::WhisperSmall => "Le plus précis en général",
            Engine::WhisperBase => "Plus léger et plus rapide",
            Engine::Vosk => "Instantané, limité aux noms de pays",
        }
    }

    pub fn size_mb(self) -> u32 {
        match self {
            Engine::WhisperSmall => 190,
            Engine::WhisperBase => 60,
            Engine::Vosk => 42,
        }
    }

    fn bytes(self) -> u64 {
        match self {
            Engine::WhisperSmall => 190_085_487,
            Engine::WhisperBase => 59_707_625,
            Engine::Vosk => 42_233_323,
        }
    }

    /// Empreinte SHA-256 du fichier téléchargé. Celles de Whisper sont publiées par
    /// Hugging Face ; Vosk n'en publie pas, c'est celle de l'archive officielle.
    fn sha256(self) -> &'static str {
        match self {
            Engine::WhisperSmall => {
                "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb"
            }
            Engine::WhisperBase => {
                "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898"
            }
            Engine::Vosk => "cabf6180e177eb9b3a9a9d43a437bd5e549f3a7d09525e5d69a3fed787be12ad",
        }
    }

    fn url(self) -> &'static str {
        match self {
            Engine::WhisperSmall => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin"
            }
            Engine::WhisperBase => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin"
            }
            Engine::Vosk => "https://alphacephei.com/vosk/models/vosk-model-small-fr-0.22.zip",
        }
    }

    /// Fichier (Whisper) ou dossier (Vosk) du modèle.
    pub fn path(self) -> PathBuf {
        let models = data_dir().join("models");
        match self {
            Engine::WhisperSmall => models.join("ggml-small-q5_1.bin"),
            Engine::WhisperBase => models.join("ggml-base-q5_1.bin"),
            Engine::Vosk => models.join("vosk-model-small-fr-0.22"),
        }
    }

    pub fn installed(self) -> bool {
        match self {
            Engine::Vosk => self.path().join("am").join("final.mdl").exists(),
            _ => self.path().exists(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Settings {
    voice_engine: Option<Engine>,
}

fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

/// Moteur choisi : celui enregistré, sinon le premier installé, sinon Whisper small.
pub fn saved_engine() -> Engine {
    std::fs::read(settings_path())
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .and_then(|s| s.voice_engine)
        .or_else(|| Engine::ALL.into_iter().find(|e| e.installed()))
        .unwrap_or(Engine::WhisperSmall)
}

pub fn save_engine(engine: Engine) {
    let settings = Settings {
        voice_engine: Some(engine),
    };
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(json) = serde_json::to_vec_pretty(&settings) {
        let _ = std::fs::write(settings_path(), json);
    }
}

/// Télécharge le modèle avec le `curl` du système (présent sous Linux et Windows 10+),
/// ce qui évite d'embarquer une pile HTTPS dans l'application.
pub fn download(engine: Engine, mut progress: impl FnMut(f32)) -> Result<(), String> {
    let models = data_dir().join("models");
    std::fs::create_dir_all(&models).map_err(|e| e.to_string())?;
    let part = models.join(format!("{engine:?}.part"));

    let mut command = std::process::Command::new("curl");
    command.args([
        "--fail",
        "--location",
        "--silent",
        "--show-error",
        "--output",
    ]);
    command.arg(&part).arg(engine.url());
    command.stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "curl est introuvable sur ce système".to_owned())?;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        let received = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        progress((received as f32 / engine.bytes() as f32).min(0.99));
        std::thread::sleep(Duration::from_millis(150));
    };
    if !status.success() {
        let mut message = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut message);
        }
        let _ = std::fs::remove_file(&part);
        return Err(message.trim().to_owned());
    }

    let digest = sha256_file(&part).map_err(|e| e.to_string())?;
    if digest != engine.sha256() {
        let _ = std::fs::remove_file(&part);
        return Err(
            "le fichier reçu est corrompu ou a été modifié (empreinte SHA-256 incorrecte)".into(),
        );
    }

    match engine {
        Engine::Vosk => {
            let archive = std::fs::File::open(&part).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(archive).map_err(|e| e.to_string())?;
            zip.extract(&models).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&part);
        }
        _ => std::fs::rename(&part, engine.path()).map_err(|e| e.to_string())?,
    }
    progress(1.0);
    if engine.installed() {
        Ok(())
    } else {
        Err("modèle incomplet".into())
    }
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_known_content() {
        let path = std::env::temp_dir().join(format!("geo-learner-sha-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let digest = sha256_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Télécharge réellement Whisper base (60 Mo) et vérifie son empreinte :
    /// `cargo test -p geo-learner -- --ignored downloads --nocapture`.
    #[test]
    #[ignore]
    fn downloads_and_verifies_a_model() {
        download(Engine::WhisperBase, |_| {}).unwrap();
        assert!(Engine::WhisperBase.installed());
    }
}

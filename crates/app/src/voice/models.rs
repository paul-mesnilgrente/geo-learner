//! Modèle de reconnaissance vocale : emplacement et téléchargement.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::progress::data_dir;

pub const MODEL_SIZE_MB: u32 = 42;
const MODEL_BYTES: u64 = 42_233_323;
const MODEL_URL: &str = "https://alphacephei.com/vosk/models/vosk-model-small-fr-0.22.zip";
/// Empreinte de l'archive officielle (Vosk n'en publie pas).
const MODEL_SHA256: &str = "cabf6180e177eb9b3a9a9d43a437bd5e549f3a7d09525e5d69a3fed787be12ad";

/// Dossier du modèle Vosk français.
pub fn path() -> PathBuf {
    data_dir().join("models").join("vosk-model-small-fr-0.22")
}

pub fn installed() -> bool {
    path().join("am").join("final.mdl").exists()
}

/// Télécharge le modèle avec le `curl` du système (présent sous Linux et Windows 10+),
/// ce qui évite d'embarquer une pile HTTPS dans l'application.
pub fn download(mut progress: impl FnMut(f32)) -> Result<(), String> {
    let models = data_dir().join("models");
    std::fs::create_dir_all(&models).map_err(|e| e.to_string())?;
    let part = models.join("vosk.zip.part");

    let mut command = std::process::Command::new("curl");
    command.args([
        "--fail",
        "--location",
        "--silent",
        "--show-error",
        "--output",
    ]);
    command.arg(&part).arg(MODEL_URL);
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
        progress((received as f32 / MODEL_BYTES as f32).min(0.99));
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
    if digest != MODEL_SHA256 {
        let _ = std::fs::remove_file(&part);
        return Err(
            "le fichier reçu est corrompu ou a été modifié (empreinte SHA-256 incorrecte)".into(),
        );
    }

    let archive = std::fs::File::open(&part).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(archive).map_err(|e| e.to_string())?;
    zip.extract(&models).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&part);
    progress(1.0);
    if installed() {
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
}

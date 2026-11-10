//! Modèle de reconnaissance vocale : emplacement et téléchargement.

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::i18n::Lang;
use crate::progress::data_dir;
use crate::tr;

/// Petit modèle Vosk d'une langue : son archive officielle et son empreinte (Vosk n'en publie
/// pas ; relevée au premier téléchargement).
struct Model {
    name: &'static str,
    bytes: u64,
    sha256: &'static str,
}

fn model(lang: Lang) -> Model {
    match lang {
        Lang::Fr => Model {
            name: "vosk-model-small-fr-0.22",
            bytes: 42_233_323,
            sha256: "cabf6180e177eb9b3a9a9d43a437bd5e549f3a7d09525e5d69a3fed787be12ad",
        },
        Lang::En => Model {
            name: "vosk-model-small-en-us-0.15",
            bytes: 41_205_931,
            sha256: "30f26242c4eb449f948e42cb302dd7a686cb29a3423a8367f99ff41780942498",
        },
    }
}

/// Taille du téléchargement, en Mo.
pub fn size_mb(lang: Lang) -> u32 {
    (model(lang).bytes as f64 / 1_000_000.0).round() as u32
}

/// Dossier du modèle Vosk de cette langue.
pub fn path(lang: Lang) -> PathBuf {
    data_dir().join("models").join(model(lang).name)
}

pub fn installed(lang: Lang) -> bool {
    path(lang).join("am").join("final.mdl").exists()
}

/// Télécharge le modèle avec le `curl` du système (présent sous Linux et Windows 10+),
/// ce qui évite d'embarquer une pile HTTPS dans l'application. curl envoie l'archive sur sa
/// sortie standard et c'est l'application qui l'écrit : sous Windows, curl.exe n'écrit pas
/// toujours là où l'application voit ses fichiers (dossiers virtualisés du paquet MSIX,
/// chemins non ASCII), et il échouait alors avec « client returned ERROR on write ».
pub fn download(lang: Lang, mut progress: impl FnMut(f32)) -> Result<(), String> {
    let model = model(lang);
    let url = format!("https://alphacephei.com/vosk/models/{}.zip", model.name);
    let models = data_dir().join("models");
    std::fs::create_dir_all(&models).map_err(|e| e.to_string())?;
    let part = models.join(format!("{}.zip.part", model.name));

    let mut command = std::process::Command::new("curl");
    command.args(["--fail", "--location", "--silent", "--show-error"]);
    command.arg(&url);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|_| {
        tr!(
            lang,
            "curl est introuvable sur ce système",
            "curl was not found on this system"
        )
    })?;
    let written = copy_with_progress(child.stdout.take(), &part, |received| {
        progress((received as f32 / model.bytes as f32).min(0.99));
    });
    if written.is_err() {
        // Plus personne ne lit sa sortie : curl ne doit pas rester bloqué.
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{}: {e}", part.display()));
    }
    if !status.success() {
        let mut message = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut message);
        }
        let _ = std::fs::remove_file(&part);
        return Err(message.trim().to_owned());
    }

    let digest = sha256_file(&part).map_err(|e| e.to_string())?;
    if digest != model.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(tr!(
            lang,
            "le fichier reçu est corrompu ou a été modifié (empreinte SHA-256 incorrecte)",
            "the downloaded file is corrupt or was modified (wrong SHA-256 fingerprint)"
        ));
    }

    let archive = std::fs::File::open(&part).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(archive).map_err(|e| e.to_string())?;
    zip.extract(&models).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&part);
    progress(1.0);
    if installed(lang) {
        Ok(())
    } else {
        Err(tr!(lang, "modèle incomplet", "incomplete model"))
    }
}

/// Recopie la sortie de curl dans `path`, en signalant le nombre d'octets reçus.
fn copy_with_progress(
    source: Option<impl Read>,
    path: &Path,
    mut progress: impl FnMut(u64),
) -> std::io::Result<()> {
    use std::io::Write;
    let Some(mut source) = source else {
        return Err(std::io::Error::other("no curl output"));
    };
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut buffer = vec![0u8; 1 << 16];
    let mut received = 0u64;
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        received += n as u64;
        progress(received);
    }
    file.flush()
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
    fn copy_writes_everything_and_reports_progress() {
        let path = std::env::temp_dir().join(format!("geo-learner-copy-{}", std::process::id()));
        let data = vec![7u8; 200_000];
        let mut seen = Vec::new();
        copy_with_progress(Some(&data[..]), &path, |n| seen.push(n)).unwrap();
        let written = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(written, data);
        assert_eq!(seen.last(), Some(&200_000));
    }

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

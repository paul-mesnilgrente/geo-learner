//! Saisie vocale hors ligne : micro (cpal) → reconnaissance (utter, modèle Vosk français)
//! restreinte aux noms de pays.

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc as std_mpsc};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use iced::Subscription;
use iced::futures::channel::mpsc::{self, Sender};
use iced::futures::{SinkExt, Stream, StreamExt};

use crate::WORLD;
use crate::progress::data_dir;

const MODEL_NAME: &str = "vosk-model-small-fr-0.22";
const MODEL_URL: &str = "https://alphacephei.com/vosk/models/vosk-model-small-fr-0.22.zip";
pub const MODEL_SIZE_MB: u32 = 41;
const MODEL_BYTES: u64 = 42_233_323;
/// Coût du chemin « mot inconnu » : plus il est bas, plus les paroles hors liste sont rejetées
/// (au risque de manquer de vrais noms de pays). -3 a donné le meilleur compromis aux essais.
const UNKNOWN_COST: f32 = -3.0;

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// Le modèle n'est pas encore téléchargé.
    Missing,
    Downloading(f32),
    Idle,
    Loading,
    Listening,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Sender<Command>),
    State(State),
    /// Transcription provisoire de la phrase en cours.
    Partial(String),
    /// Phrase terminée.
    Heard(String),
    /// Niveau sonore entre 0 et 1.
    Level(f32),
}

#[derive(Debug, Clone, Copy)]
pub enum Command {
    Start,
    Stop,
    Download,
}

pub fn subscription() -> Subscription<Event> {
    Subscription::run(worker)
}

fn model_dir() -> PathBuf {
    data_dir().join("models").join(MODEL_NAME)
}

fn installed() -> bool {
    model_dir().join("am").join("final.mdl").exists()
}

fn worker() -> impl Stream<Item = Event> {
    iced::stream::channel(256, async |mut output: Sender<Event>| {
        let (commands, mut inbox) = mpsc::channel::<Command>(16);
        let _ = output.send(Event::Ready(commands)).await;
        let initial = if installed() {
            State::Idle
        } else {
            State::Missing
        };
        let _ = output.send(Event::State(initial)).await;

        // Drapeau d'arrêt de la session d'écoute en cours (vrai = arrêtée).
        let mut session: Option<Arc<AtomicBool>> = None;
        let mut downloading = false;

        while let Some(command) = inbox.next().await {
            match command {
                Command::Start => {
                    if session
                        .as_ref()
                        .is_some_and(|stop| !stop.load(Ordering::Relaxed))
                    {
                        continue;
                    }
                    let stop = Arc::new(AtomicBool::new(false));
                    session = Some(stop.clone());
                    let mut tx = output.clone();
                    std::thread::spawn(move || {
                        let state = match listen(&mut tx, &stop) {
                            Ok(()) => State::Idle,
                            Err(e) => State::Error(e),
                        };
                        stop.store(true, Ordering::Relaxed);
                        let _ = tx.try_send(Event::Partial(String::new()));
                        let _ = tx.try_send(Event::State(state));
                    });
                }
                Command::Stop => {
                    if let Some(stop) = session.take() {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
                Command::Download => {
                    if downloading || installed() {
                        continue;
                    }
                    downloading = true;
                    let mut tx = output.clone();
                    std::thread::spawn(move || {
                        let state = match download(&mut tx) {
                            Ok(()) => State::Idle,
                            Err(e) => State::Error(format!("Téléchargement impossible : {e}")),
                        };
                        let _ = tx.try_send(Event::State(state));
                    });
                }
            }
        }
    })
}

static MODEL: OnceLock<Result<utter::Model, String>> = OnceLock::new();

fn model() -> Result<&'static utter::Model, String> {
    MODEL
        .get_or_init(|| {
            utter::Model::open(&model_dir()).map_err(|e| format!("Modèle vocal illisible : {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Formes orales des pays dont tous les mots sont connus du modèle.
fn grammar(model: &utter::Model) -> Vec<String> {
    let mut phrases: Vec<String> = Vec::new();
    for country in &WORLD.countries {
        for form in &country.spoken {
            if form.split(' ').all(|w| model.word_ids.contains_key(w)) && !phrases.contains(form) {
                phrases.push(form.clone());
            }
        }
    }
    phrases
}

fn listen(tx: &mut Sender<Event>, stop: &AtomicBool) -> Result<(), String> {
    if !installed() {
        return Err("Le modèle vocal n'est pas installé.".into());
    }
    let _ = tx.try_send(Event::State(State::Loading));
    let model = model()?;
    let grammar = grammar(model);

    let device = cpal::default_host()
        .default_input_device()
        .ok_or("Aucun micro détecté.")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("Micro indisponible : {e}"))?;
    let rate = config.sample_rate();
    let channels = usize::from(config.channels()).max(1);

    let (audio_tx, audio_rx) = std_mpsc::sync_channel::<Vec<f32>>(128);
    let on_error = |e| eprintln!("erreur audio : {e}");
    let stream_config = config.config();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream::<f32, _, _>(
            stream_config,
            move |data, _| {
                let _ = audio_tx.try_send(downmix(data, channels, |s| s));
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream::<i16, _, _>(
            stream_config,
            move |data, _| {
                let _ = audio_tx.try_send(downmix(data, channels, |s| f32::from(s) / 32768.0));
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream::<i32, _, _>(
            stream_config,
            move |data, _| {
                let _ = audio_tx.try_send(downmix(data, channels, |s| s as f32 / 2_147_483_648.0));
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream::<u16, _, _>(
            stream_config,
            move |data, _| {
                let _ = audio_tx.try_send(downmix(data, channels, |s| {
                    (f32::from(s) - 32768.0) / 32768.0
                }));
            },
            on_error,
            None,
        ),
        other => return Err(format!("Format audio non géré : {other:?}")),
    }
    .map_err(|e| format!("Impossible d'ouvrir le micro : {e}"))?;
    stream
        .play()
        .map_err(|e| format!("Impossible de démarrer le micro : {e}"))?;

    let options = utter::RecognizerOptions {
        unknown_cost: Some(UNKNOWN_COST),
        ..Default::default()
    };
    let mut recognizer = utter::Recognizer::with_options(model, rate as f32, &grammar, &options)
        .map_err(|e| format!("Reconnaissance vocale indisponible : {e}"))?;
    let _ = tx.try_send(Event::State(State::Listening));

    let mut last_partial = String::new();
    let mut last_report = Instant::now();
    let mut peak = 0.0f32;
    while !stop.load(Ordering::Relaxed) {
        let chunk = match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => chunk,
            Err(std_mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
        };
        peak = peak.max(rms(&chunk));
        let Ok(step) = recognizer.accept_f32(&chunk) else {
            continue;
        };
        if step.endpoint {
            let text = field(recognizer.result(), "text");
            last_partial.clear();
            let _ = tx.try_send(Event::Partial(String::new()));
            if !text.is_empty() {
                let _ = tx.try_send(Event::Heard(text));
            }
        } else if last_report.elapsed() >= Duration::from_millis(120) {
            let partial = field(recognizer.partial(), "partial");
            if partial != last_partial {
                last_partial = partial.clone();
                let _ = tx.try_send(Event::Partial(partial));
            }
        }
        if last_report.elapsed() >= Duration::from_millis(120) {
            let _ = tx.try_send(Event::Level((peak * 6.0).min(1.0)));
            peak = 0.0;
            last_report = Instant::now();
        }
    }
    drop(stream);
    let _ = tx.try_send(Event::Level(0.0));
    Ok(())
}

fn downmix<T: Copy>(data: &[T], channels: usize, convert: impl Fn(T) -> f32) -> Vec<f32> {
    data.chunks(channels)
        .map(|frame| {
            (frame.iter().map(|&s| convert(s)).sum::<f32>() / frame.len() as f32).clamp(-1.0, 1.0)
        })
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
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

fn download(tx: &mut Sender<Event>) -> Result<(), String> {
    let models = data_dir().join("models");
    std::fs::create_dir_all(&models).map_err(|e| e.to_string())?;
    let _ = tx.try_send(Event::State(State::Downloading(0.0)));

    // Le téléchargement passe par le `curl` du système (présent sous Linux et Windows 10+),
    // ce qui évite d'embarquer une pile HTTPS dans l'application.
    let archive_path = models.join(format!("{MODEL_NAME}.zip.part"));
    let mut command = std::process::Command::new("curl");
    command.args([
        "--fail",
        "--location",
        "--silent",
        "--show-error",
        "--output",
    ]);
    command.arg(&archive_path).arg(MODEL_URL);
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
        let received = std::fs::metadata(&archive_path)
            .map(|m| m.len())
            .unwrap_or(0);
        let _ = tx.try_send(Event::State(State::Downloading(
            (received as f32 / MODEL_BYTES as f32).min(0.99),
        )));
        std::thread::sleep(Duration::from_millis(150));
    };
    if !status.success() {
        let mut message = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut message);
        }
        let _ = std::fs::remove_file(&archive_path);
        return Err(message.trim().to_owned());
    }

    let _ = tx.try_send(Event::State(State::Downloading(1.0)));
    let archive = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(archive).map_err(|e| e.to_string())?;
    zip.extract(&models).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&archive_path);
    if installed() {
        Ok(())
    } else {
        Err("archive inattendue".into())
    }
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

    /// Télécharge réellement le modèle (≈ 42 Mo) : `cargo test -p geo-learner -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_model() {
        let (mut tx, _rx) = mpsc::channel(1024);
        download(&mut tx).unwrap();
        assert!(installed());
    }

    /// Ouvre le micro par défaut pendant 3 s avec le vrai pipeline : `-- --ignored listens`.
    #[test]
    #[ignore]
    fn listens_to_microphone() {
        let (mut tx, mut rx) = mpsc::channel(4096);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = std::thread::spawn(move || listen(&mut tx, &flag));
        std::thread::sleep(Duration::from_secs(3));
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap().unwrap();
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let levels: Vec<f32> = events
            .iter()
            .filter_map(|e| {
                if let Event::Level(l) = e {
                    Some(*l)
                } else {
                    None
                }
            })
            .collect();
        println!("{} évènements, niveaux : {levels:?}", events.len());
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::State(State::Listening)))
        );
    }

    #[test]
    fn downmix_averages_channels() {
        assert_eq!(downmix(&[0.5f32, -0.5, 1.0, 1.0], 2, |s| s), vec![0.0, 1.0]);
    }
}

//! Saisie vocale hors ligne : micro (cpal) → 16 kHz → découpage en phrases → transcription
//! (Whisper ou Vosk) → noms de pays.
//!
//! Diagnostic : avec `GEO_VOICE_DUMP=<dossier>`, chaque phrase captée est enregistrée en WAV
//! avec sa transcription.

mod audio;
mod engine;
mod models;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc as std_mpsc};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use iced::Subscription;
use iced::futures::channel::mpsc::{self, Sender};
use iced::futures::{SinkExt, Stream, StreamExt};

pub use models::{Engine, save_engine, saved_engine};

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Downloading(Engine, f32),
    Loading,
    Listening,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Sender<Command>),
    State(State),
    /// De la parole est en cours.
    Speaking(bool),
    /// Une phrase est en cours de transcription.
    Transcribing(bool),
    /// Transcription d'une phrase.
    Heard(String),
    /// Niveau sonore entre 0 et 1.
    Level(f32),
}

#[derive(Debug, Clone, Copy)]
pub enum Command {
    Start(Engine),
    Stop,
    Download(Engine),
}

pub fn subscription() -> Subscription<Event> {
    Subscription::run(worker)
}

fn worker() -> impl Stream<Item = Event> {
    iced::stream::channel(256, async |mut output: Sender<Event>| {
        let (commands, mut inbox) = mpsc::channel::<Command>(16);
        let _ = output.send(Event::Ready(commands)).await;

        // Drapeau d'arrêt de la session d'écoute en cours (vrai = arrêtée).
        let mut session: Option<Arc<AtomicBool>> = None;
        let downloading = Arc::new(AtomicBool::new(false));

        while let Some(command) = inbox.next().await {
            match command {
                Command::Start(engine) => {
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
                        let state = match listen(engine, &mut tx, &stop) {
                            Ok(()) => State::Idle,
                            Err(e) => State::Error(e),
                        };
                        stop.store(true, Ordering::Relaxed);
                        let _ = tx.try_send(Event::Speaking(false));
                        let _ = tx.try_send(Event::Level(0.0));
                        let _ = tx.try_send(Event::State(state));
                    });
                }
                Command::Stop => {
                    if let Some(stop) = session.take() {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
                Command::Download(engine) => {
                    if engine.installed() || downloading.swap(true, Ordering::Relaxed) {
                        continue;
                    }
                    let mut tx = output.clone();
                    let downloading = downloading.clone();
                    std::thread::spawn(move || {
                        let mut progress = tx.clone();
                        let result = models::download(engine, |p| {
                            let _ = progress.try_send(Event::State(State::Downloading(engine, p)));
                        });
                        downloading.store(false, Ordering::Relaxed);
                        let state = match result {
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

fn listen(engine: Engine, tx: &mut Sender<Event>, stop: &AtomicBool) -> Result<(), String> {
    let _ = tx.try_send(Event::State(State::Loading));
    let mut transcriber = engine::load(engine)?;

    // La transcription tourne à part pour ne jamais bloquer la capture.
    let (segments, inbox) = std_mpsc::channel::<Vec<f32>>();
    let mut out = tx.clone();
    let transcription = std::thread::spawn(move || {
        let dump = std::env::var_os("GEO_VOICE_DUMP").map(std::path::PathBuf::from);
        for (n, segment) in inbox.into_iter().enumerate() {
            let _ = out.try_send(Event::Transcribing(true));
            let result = transcriber.transcribe(&segment);
            let _ = out.try_send(Event::Transcribing(false));
            match result {
                Ok(text) => {
                    if let Some(dir) = &dump {
                        let base = dir.join(format!("{}-{n:03}", crate::progress::now()));
                        let _ = std::fs::create_dir_all(dir);
                        let _ = audio::write_wav(&base.with_extension("wav"), &segment);
                        let _ = std::fs::write(base.with_extension("txt"), &text);
                    }
                    if !text.is_empty() {
                        let _ = out.try_send(Event::Heard(text));
                    }
                }
                Err(e) => eprintln!("transcription : {e}"),
            }
        }
    });

    let device = cpal::default_host()
        .default_input_device()
        .ok_or("Aucun micro détecté.")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("Micro indisponible : {e}"))?;
    let channels = usize::from(config.channels()).max(1);

    let (audio_tx, audio_rx) = std_mpsc::channel::<Vec<f32>>();
    let on_error = |e| eprintln!("erreur audio : {e}");
    let stream_config = config.config();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream::<f32, _, _>(
            stream_config,
            move |data, _| {
                let _ = audio_tx.send(audio::downmix(data, channels, |s| s));
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream::<i16, _, _>(
            stream_config,
            move |data, _| {
                let _ = audio_tx.send(audio::downmix(data, channels, |s| f32::from(s) / 32768.0));
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream::<i32, _, _>(
            stream_config,
            move |data, _| {
                let _ = audio_tx.send(audio::downmix(data, channels, |s| {
                    s as f32 / 2_147_483_648.0
                }));
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream::<u16, _, _>(
            stream_config,
            move |data, _| {
                let _ = audio_tx.send(audio::downmix(data, channels, |s| {
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
    let _ = tx.try_send(Event::State(State::Listening));

    let mut resampler = audio::Resampler::new(config.sample_rate());
    let mut vad = audio::Vad::default();
    let mut resampled = Vec::new();
    let mut events = Vec::new();
    let mut last_level = Instant::now();
    let mut peak = 0.0f32;
    while !stop.load(Ordering::Relaxed) {
        let chunk = match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => chunk,
            Err(std_mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
        };
        resampled.clear();
        resampler.process(&chunk, &mut resampled);
        peak = peak.max(audio::rms(&resampled));
        vad.push(&resampled, &mut events);
        for event in events.drain(..) {
            match event {
                audio::VadEvent::Started => {
                    let _ = tx.try_send(Event::Speaking(true));
                }
                audio::VadEvent::Segment(segment) => {
                    let _ = tx.try_send(Event::Speaking(false));
                    let _ = segments.send(segment);
                }
                audio::VadEvent::Discarded => {
                    let _ = tx.try_send(Event::Speaking(false));
                }
            }
        }
        if last_level.elapsed() >= Duration::from_millis(100) {
            let _ = tx.try_send(Event::Level((peak * 6.0).min(1.0)));
            peak = 0.0;
            last_level = Instant::now();
        }
    }
    drop(stream);
    drop(segments);
    let _ = transcription.join();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ouvre le micro par défaut pendant 3 s avec le vrai pipeline (moteur choisi installé requis) :
    /// `cargo test -p geo-learner -- --ignored listens --nocapture`.
    #[test]
    #[ignore]
    fn listens_to_microphone() {
        let (mut tx, mut rx) = mpsc::channel(4096);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let engine = saved_engine();
        let worker = std::thread::spawn(move || listen(engine, &mut tx, &flag));
        std::thread::sleep(Duration::from_secs(3));
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap().unwrap();
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        println!("{} évènements avec {}", events.len(), engine.label());
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::State(State::Listening)))
        );
    }
}

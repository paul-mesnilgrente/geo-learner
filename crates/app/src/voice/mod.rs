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
    /// Transcription provisoire de la phrase en cours (vide quand elle est terminée).
    Partial(String),
    /// Pays reconnus avec certitude. `done` : la phrase est terminée (`text` est alors
    /// sa transcription finale).
    Heard {
        countries: Vec<usize>,
        text: String,
        done: bool,
    },
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
    let (jobs, inbox) = std_mpsc::channel::<Job>();
    let busy = Arc::new(AtomicBool::new(false));
    let transcription = {
        let mut out = tx.clone();
        let busy = busy.clone();
        std::thread::spawn(move || transcribe_jobs(transcriber.as_mut(), inbox, &busy, &mut out))
    };

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

    let mut pipeline = Pipeline::new(config.sample_rate(), jobs, busy);
    let mut last_level = Instant::now();
    let mut peak = 0.0f32;
    while !stop.load(Ordering::Relaxed) {
        let chunk = match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => chunk,
            Err(std_mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
        };
        peak = peak.max(pipeline.feed(&chunk, tx));
        if last_level.elapsed() >= Duration::from_millis(100) {
            let _ = tx.try_send(Event::Level((peak * 6.0).min(1.0)));
            peak = 0.0;
            last_level = Instant::now();
        }
    }
    drop(stream);
    drop(pipeline);
    let _ = transcription.join();
    Ok(())
}

/// Durée minimale d'audio nouveau entre deux transcriptions provisoires (0,3 s).
const PARTIAL_STEP: usize = audio::RATE as usize * 3 / 10;

/// Du micro aux transcriptions : conversion en 16 kHz, découpage en phrases et envoi
/// des phrases (en cours ou terminées) au fil de transcription.
struct Pipeline {
    resampler: audio::Resampler,
    vad: audio::Vad,
    jobs: std_mpsc::Sender<Job>,
    busy: Arc<AtomicBool>,
    /// Longueur de la phrase en cours lors de la dernière transcription provisoire.
    transcribed: usize,
    resampled: Vec<f32>,
    events: Vec<audio::VadEvent>,
}

impl Pipeline {
    fn new(rate: u32, jobs: std_mpsc::Sender<Job>, busy: Arc<AtomicBool>) -> Self {
        Self {
            resampler: audio::Resampler::new(rate),
            vad: audio::Vad::default(),
            jobs,
            busy,
            transcribed: 0,
            resampled: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Traite un bloc du micro (mono) et renvoie son niveau sonore.
    fn feed(&mut self, chunk: &[f32], tx: &mut Sender<Event>) -> f32 {
        self.resampled.clear();
        self.resampler.process(chunk, &mut self.resampled);
        self.vad.push(&self.resampled, &mut self.events);
        for event in self.events.drain(..) {
            match event {
                audio::VadEvent::Started => {
                    self.transcribed = 0;
                    let _ = tx.try_send(Event::Speaking(true));
                }
                audio::VadEvent::Segment(segment) => {
                    let _ = tx.try_send(Event::Speaking(false));
                    let _ = self.jobs.send(Job::Final(segment));
                }
                audio::VadEvent::Discarded => {
                    let _ = tx.try_send(Event::Speaking(false));
                }
            }
        }
        // Pendant la parole, on retranscrit régulièrement la phrase en cours, dès que le
        // moteur est libre, pour valider les pays sans attendre la pause.
        if let Some(current) = self.vad.current()
            && current.len() >= self.transcribed + PARTIAL_STEP
            && !self.busy.swap(true, Ordering::Relaxed)
        {
            self.transcribed = current.len();
            let _ = self.jobs.send(Job::Partial(current.to_vec()));
        }
        audio::rms(&self.resampled)
    }
}

enum Job {
    /// Phrase en cours, transcrite à titre provisoire.
    Partial(Vec<f32>),
    /// Phrase terminée.
    Final(Vec<f32>),
}

fn transcribe_jobs(
    transcriber: &mut dyn engine::Transcriber,
    inbox: std_mpsc::Receiver<Job>,
    busy: &AtomicBool,
    out: &mut Sender<Event>,
) {
    let matcher = geo_core::Matcher::new(&crate::WORLD);
    let dump = std::env::var_os("GEO_VOICE_DUMP").map(std::path::PathBuf::from);
    let mut agreement = Agreement::default();
    let mut finals = 0;
    for job in inbox {
        match job {
            Job::Partial(audio) => {
                let result = transcriber.transcribe(&audio);
                busy.store(false, Ordering::Relaxed);
                let Ok(text) = result else { continue };
                let heard = matcher.scan_fuzzy(&text);
                let countries = agreement.partial(&heard, |c| matcher.can_extend(c));
                let _ = out.try_send(Event::Partial(text.clone()));
                if !countries.is_empty() {
                    let _ = out.try_send(Event::Heard {
                        countries,
                        text,
                        done: false,
                    });
                }
            }
            Job::Final(audio) => {
                let text = match transcriber.transcribe(&audio) {
                    Ok(text) => text,
                    Err(e) => {
                        eprintln!("transcription : {e}");
                        String::new()
                    }
                };
                if let Some(dir) = &dump {
                    let base = dir.join(format!("{}-{finals:03}", crate::progress::now()));
                    let _ = std::fs::create_dir_all(dir);
                    let _ = audio::write_wav(&base.with_extension("wav"), &audio);
                    let _ = std::fs::write(base.with_extension("txt"), &text);
                }
                finals += 1;
                let countries = agreement.finish(&matcher.scan_fuzzy(&text));
                let _ = out.try_send(Event::Partial(String::new()));
                let _ = out.try_send(Event::Heard {
                    countries,
                    text,
                    done: true,
                });
            }
        }
    }
}

/// Validation au fil de la parole (« accord local ») : un pays n'est retenu que lorsque
/// deux transcriptions successives s'accordent sur lui. Le dernier nom entendu attend la
/// suite s'il peut encore s'allonger (« Niger » / « Nigeria »).
#[derive(Default)]
struct Agreement {
    previous: Vec<usize>,
    committed: Vec<usize>,
}

impl Agreement {
    fn partial(&mut self, heard: &[usize], can_extend: impl Fn(usize) -> bool) -> Vec<usize> {
        let mut agreed = heard
            .iter()
            .zip(&self.previous)
            .take_while(|(a, b)| a == b)
            .count();
        if agreed == heard.len() && agreed > 0 && can_extend(heard[agreed - 1]) {
            agreed -= 1;
        }
        self.previous = heard.to_vec();
        self.commit(&heard[..agreed])
    }

    /// Phrase terminée : tout ce qui a été entendu est retenu.
    fn finish(&mut self, heard: &[usize]) -> Vec<usize> {
        let new = self.commit(heard);
        *self = Self::default();
        new
    }

    fn commit(&mut self, countries: &[usize]) -> Vec<usize> {
        let new: Vec<usize> = countries
            .iter()
            .copied()
            .filter(|c| !self.committed.contains(c))
            .collect();
        self.committed.extend(&new);
        new
    }
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

    #[test]
    fn agreement_waits_for_two_matching_transcriptions() {
        let mut a = Agreement::default();
        let never = |_| false;
        assert!(a.partial(&[1], never).is_empty());
        assert_eq!(a.partial(&[1, 2], never), vec![1]);
        assert_eq!(a.partial(&[1, 2], never), vec![2]);
        assert!(a.partial(&[1, 2, 3], never).is_empty());
        // La fin de phrase retient le reste, sans doublon.
        assert_eq!(a.finish(&[1, 2, 3, 4]), vec![3, 4]);
        // Nouvelle phrase : l'état repart de zéro.
        assert!(a.partial(&[5], never).is_empty());
    }

    #[test]
    fn agreement_holds_back_names_that_can_grow() {
        let (niger, nigeria, france) = (10, 11, 12);
        let can_extend = |c| c == niger;
        let mut a = Agreement::default();
        assert!(a.partial(&[niger], can_extend).is_empty());
        assert!(a.partial(&[niger], can_extend).is_empty());
        assert!(a.partial(&[nigeria], can_extend).is_empty());
        assert_eq!(a.partial(&[nigeria, france], can_extend), vec![nigeria]);
        // « Niger » suivi d'un autre pays est sûr.
        let mut a = Agreement::default();
        a.partial(&[niger, france], can_extend);
        assert_eq!(a.partial(&[niger, france], can_extend), vec![niger, france]);
    }

    /// Rejoue un enregistrement WAV 16 kHz en temps réel dans le pipeline et affiche
    /// le moment où chaque pays est validé :
    /// `GEO_VOICE_WAV=fichier.wav cargo test -p geo-learner --release -- --ignored streams --nocapture`
    #[test]
    #[ignore]
    fn streams_a_recording() {
        let path = std::env::var("GEO_VOICE_WAV").expect("GEO_VOICE_WAV");
        let bytes = std::fs::read(path).unwrap();
        let audio: Vec<f32> = bytes[44..]
            .chunks(2)
            .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
            .collect();
        for engine in Engine::ALL.into_iter().filter(|e| e.installed()) {
            let mut transcriber = engine::load(engine).unwrap();
            let (jobs, inbox) = std_mpsc::channel();
            let busy = Arc::new(AtomicBool::new(false));
            let (mut tx, mut rx) = mpsc::channel(4096);
            let worker = {
                let (busy, mut out) = (busy.clone(), tx.clone());
                std::thread::spawn(move || {
                    transcribe_jobs(transcriber.as_mut(), inbox, &busy, &mut out)
                })
            };
            let mut pipeline = Pipeline::new(audio::RATE, jobs, busy);
            let start = Instant::now();
            // Blocs de 20 ms, au rythme réel, suivis d'une seconde de silence.
            let silence = vec![0.0; audio::RATE as usize];
            for chunk in audio.chunks(320).chain(silence.chunks(320)) {
                pipeline.feed(chunk, &mut tx);
                std::thread::sleep(Duration::from_millis(20));
                while let Ok(event) = rx.try_recv() {
                    if let Event::Heard {
                        countries, done, ..
                    } = event
                    {
                        let names: Vec<&str> = countries
                            .iter()
                            .map(|&c| crate::WORLD.countries[c].name.as_str())
                            .collect();
                        println!(
                            "{:>13} {:5.2} s {names:?}{}",
                            engine.label(),
                            start.elapsed().as_secs_f32(),
                            if done { " (fin de phrase)" } else { "" }
                        );
                    }
                }
            }
            drop(pipeline);
            worker.join().unwrap();
            while let Ok(event) = rx.try_recv() {
                if let Event::Heard { countries, .. } = event {
                    let names: Vec<&str> = countries
                        .iter()
                        .map(|&c| crate::WORLD.countries[c].name.as_str())
                        .collect();
                    println!(
                        "{:>13} {:5.2} s {names:?} (fin de phrase)",
                        engine.label(),
                        start.elapsed().as_secs_f32()
                    );
                }
            }
        }
    }
}

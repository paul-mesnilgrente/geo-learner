//! Traitement du signal du micro : conversion en 16 kHz et découpage en phrases.

use std::collections::VecDeque;

/// Fréquence attendue par les modèles de reconnaissance.
pub const RATE: u32 = 16_000;

/// Conversion de fréquence d'échantillonnage en flux : filtre passe-bas (sinc fenêtrée)
/// puis interpolation linéaire.
pub struct Resampler {
    step: f64,
    taps: Vec<f32>,
    history: VecDeque<f32>,
    filtered: Vec<f32>,
    position: f64,
}

impl Resampler {
    pub fn new(input_rate: u32) -> Self {
        let step = f64::from(input_rate) / f64::from(RATE);
        let taps = if input_rate == RATE {
            Vec::new()
        } else {
            // Coupure à 7 kHz, sous la fréquence de Nyquist de la sortie (8 kHz).
            let cutoff = 7_000.0 / input_rate as f32;
            let n = 64;
            let middle = (n - 1) as f32 / 2.0;
            let mut taps: Vec<f32> = (0..n)
                .map(|k| {
                    let x = k as f32 - middle;
                    let sinc = if x == 0.0 {
                        2.0 * cutoff
                    } else {
                        (2.0 * std::f32::consts::PI * cutoff * x).sin() / (std::f32::consts::PI * x)
                    };
                    let window = 0.54
                        - 0.46 * (2.0 * std::f32::consts::PI * k as f32 / (n - 1) as f32).cos();
                    sinc * window
                })
                .collect();
            let sum: f32 = taps.iter().sum();
            taps.iter_mut().for_each(|t| *t /= sum);
            taps
        };
        Self {
            step,
            history: VecDeque::from(vec![0.0; taps.len()]),
            taps,
            filtered: Vec::new(),
            position: 0.0,
        }
    }

    pub fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        if self.taps.is_empty() {
            output.extend_from_slice(input);
            return;
        }
        for &x in input {
            self.history.pop_front();
            self.history.push_back(x);
            let y = self
                .history
                .iter()
                .zip(&self.taps)
                .map(|(h, t)| h * t)
                .sum();
            self.filtered.push(y);
        }
        while self.position + 1.0 < self.filtered.len() as f64 {
            let i = self.position as usize;
            let t = (self.position - i as f64) as f32;
            output.push(self.filtered[i] * (1.0 - t) + self.filtered[i + 1] * t);
            self.position += self.step;
        }
        let consumed = (self.position as usize).min(self.filtered.len());
        self.filtered.drain(..consumed);
        self.position -= consumed as f64;
    }
}

const FRAME: usize = (RATE / 1000 * 30) as usize; // 30 ms
const START_FRAMES: usize = 3; // 90 ms de parole pour démarrer
const END_FRAMES: usize = 20; // 600 ms de silence pour terminer
const PREROLL_FRAMES: usize = 10; // 300 ms conservés avant le début
const MIN_SPEECH_FRAMES: usize = 6; // phrases plus courtes ignorées (bruits)
const MAX_FRAMES: usize = 400; // 12 s au plus par phrase

/// Détection de parole par énergie, avec un plancher de bruit adaptatif.
pub struct Vad {
    frame: Vec<f32>,
    noise: f32,
    preroll: VecDeque<Vec<f32>>,
    segment: Vec<f32>,
    active: bool,
    loud: usize,
    quiet: usize,
    speech: usize,
    frames: usize,
}

pub enum VadEvent {
    /// Début de parole détecté.
    Started,
    /// Phrase terminée (audio 16 kHz).
    Segment(Vec<f32>),
    /// Fausse alerte : la parole était trop courte.
    Discarded,
}

impl Default for Vad {
    fn default() -> Self {
        Self {
            frame: Vec::with_capacity(FRAME),
            noise: 0.005,
            preroll: VecDeque::new(),
            segment: Vec::new(),
            active: false,
            loud: 0,
            quiet: 0,
            speech: 0,
            frames: 0,
        }
    }
}

impl Vad {
    pub fn push(&mut self, samples: &[f32], events: &mut Vec<VadEvent>) {
        for &s in samples {
            self.frame.push(s);
            if self.frame.len() == FRAME {
                let frame = std::mem::replace(&mut self.frame, Vec::with_capacity(FRAME));
                self.frame_done(frame, events);
            }
        }
    }

    fn threshold(&self) -> f32 {
        (self.noise * 3.0).max(0.01)
    }

    fn frame_done(&mut self, frame: Vec<f32>, events: &mut Vec<VadEvent>) {
        let level = rms(&frame);
        let loud = level > self.threshold();
        if !self.active {
            // Le plancher suit vite les baisses et lentement les hausses.
            self.noise = if level < self.noise {
                level
            } else {
                self.noise * 0.995 + level * 0.005
            };
            self.loud = if loud { self.loud + 1 } else { 0 };
            self.preroll.push_back(frame);
            if self.preroll.len() > PREROLL_FRAMES {
                self.preroll.pop_front();
            }
            if self.loud >= START_FRAMES {
                self.active = true;
                self.speech = self.loud;
                self.quiet = 0;
                self.frames = self.preroll.len();
                self.segment = self.preroll.drain(..).flatten().collect();
                events.push(VadEvent::Started);
            }
            return;
        }
        self.segment.extend_from_slice(&frame);
        self.frames += 1;
        if loud {
            self.speech += 1;
            self.quiet = 0;
        } else {
            self.quiet += 1;
        }
        if self.quiet >= END_FRAMES || self.frames >= MAX_FRAMES {
            self.active = false;
            self.loud = 0;
            let segment = std::mem::take(&mut self.segment);
            if self.speech >= MIN_SPEECH_FRAMES {
                events.push(VadEvent::Segment(segment));
            } else {
                events.push(VadEvent::Discarded);
            }
        }
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Moyenne des canaux d'un bloc entrelacé.
pub fn downmix<T: Copy>(data: &[T], channels: usize, convert: impl Fn(T) -> f32) -> Vec<f32> {
    data.chunks(channels)
        .map(|frame| {
            (frame.iter().map(|&s| convert(s)).sum::<f32>() / frame.len() as f32).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Écrit un fichier WAV 16 bits mono (diagnostic).
pub fn write_wav(path: &std::path::Path, samples: &[f32]) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn resampler_keeps_speech_band_and_removes_aliases() {
        let mut out = Vec::new();
        let mut r = Resampler::new(48_000);
        // Traité par petits blocs, comme le flux du micro.
        for chunk in sine(1_000.0, 48_000, 1.0, 0.5).chunks(441) {
            r.process(chunk, &mut out);
        }
        assert!(
            (out.len() as i32 - 16_000).abs() < 10,
            "{} échantillons",
            out.len()
        );
        let level = rms(&out[1000..]);
        assert!((level - 0.5 / 2f32.sqrt()).abs() < 0.02, "niveau {level}");

        // Une fréquence au-delà de 8 kHz ne doit pas se replier dans la bande utile.
        let mut out = Vec::new();
        Resampler::new(48_000).process(&sine(12_000.0, 48_000, 1.0, 0.5), &mut out);
        assert!(rms(&out[1000..]) < 0.02);
    }

    #[test]
    fn vad_cuts_one_segment_per_utterance() {
        let mut vad = Vad::default();
        let mut events = Vec::new();
        let silence = vec![0.001f32; RATE as usize];
        vad.push(&silence, &mut events);
        vad.push(&sine(300.0, RATE, 0.8, 0.3), &mut events);
        vad.push(&silence, &mut events);
        vad.push(&sine(300.0, RATE, 0.05, 0.3), &mut events); // claquement
        vad.push(&silence, &mut events);
        let segments: Vec<usize> = events
            .iter()
            .filter_map(|e| {
                if let VadEvent::Segment(s) = e {
                    Some(s.len())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(segments.len(), 1);
        // Parole (0,8 s) + amorce (0,3 s) + fin de silence (0,6 s).
        let seconds = segments[0] as f32 / RATE as f32;
        assert!((1.5..1.9).contains(&seconds), "{seconds} s");
    }

    #[test]
    fn downmix_averages_channels() {
        assert_eq!(downmix(&[0.5f32, -0.5, 1.0, 1.0], 2, |s| s), vec![0.0, 1.0]);
    }
}

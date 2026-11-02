//! Outils de développement (builds debug uniquement) : partie de démonstration et capture d'écran.
//!
//! ```sh
//! GEO_DEMO="mode=free;region=europe;answers=FRA,DEU;zoom=10,50,2" GEO_SHOT=/tmp/shot.png cargo run
//! ```

use std::path::Path;

use geo_core::Mode;
use iced::window::Screenshot;

#[derive(Debug, Default)]
pub struct Demo {
    pub mode: Option<Mode>,
    pub region: Option<String>,
    /// Codes des pays donnés comme réponses au démarrage.
    pub answers: Vec<String>,
    /// Caméra : longitude, latitude, zoom.
    pub zoom: Option<(f32, f32, f32)>,
    pub input: Option<String>,
    pub listening: bool,
    pub give_up: bool,
    pub summary: bool,
    /// Textes saisis l'un après l'autre dans le champ de réponse.
    pub typed: Vec<String>,
    /// Code du premier pays à deviner (modes Nommer et Localiser).
    pub target: Option<String>,
    /// Temps de jeu déjà écoulé, en secondes.
    pub elapsed: Option<u64>,
    /// Minuteur imposé, en secondes.
    pub timer: Option<u64>,
    /// Avancement affiché par continent.
    pub by_continent: bool,
    /// Quiz à ouvrir (identifiant d'atlas : « fr-departements »…).
    pub atlas: Option<String>,
}

pub fn demo() -> Option<Demo> {
    let spec = std::env::var("GEO_DEMO").ok()?;
    let mut demo = Demo::default();
    for part in spec.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "mode" => {
                demo.mode = match value {
                    "free" => Some(Mode::Free),
                    "identify" => Some(Mode::Identify),
                    "locate" => Some(Mode::Locate),
                    _ => None,
                }
            }
            "region" => demo.region = Some(value.to_owned()),
            "answers" => demo.answers = value.split(',').map(str::to_owned).collect(),
            "zoom" => {
                let v: Vec<f32> = value.split(',').filter_map(|x| x.parse().ok()).collect();
                if let [lon, lat, zoom] = v[..] {
                    demo.zoom = Some((lon, lat, zoom));
                }
            }
            "input" => demo.input = Some(value.to_owned()),
            "voice" => demo.listening = value == "listening",
            "giveup" => demo.give_up = true,
            "summary" => demo.summary = true,
            "target" => demo.target = Some(value.to_owned()),
            "elapsed" => demo.elapsed = value.parse().ok(),
            "timer" => demo.timer = value.parse().ok(),
            "continents" => demo.by_continent = true,
            "atlas" => demo.atlas = Some(value.to_owned()),
            "type" => demo.typed = value.split('|').map(str::to_owned).collect(),
            _ => {}
        }
    }
    Some(demo)
}

/// Taille de fenêtre imposée (`GEO_WINDOW=1920x1080`), pour les captures du Store.
pub fn window_size() -> Option<iced::Size> {
    let spec = std::env::var("GEO_WINDOW").ok()?;
    let (w, h) = spec.split_once('x')?;
    Some(iced::Size::new(w.parse().ok()?, h.parse().ok()?))
}

pub fn shot_path() -> Option<String> {
    std::env::var("GEO_SHOT").ok()
}

/// Enregistre la capture en PNG (via ImageMagick).
pub fn save(shot: &Screenshot, path: &str) {
    let raw = format!("{path}.rgba");
    if std::fs::write(&raw, &shot.rgba).is_err() {
        return;
    }
    let status = std::process::Command::new("convert")
        .args([
            "-size",
            &format!("{}x{}", shot.size.width, shot.size.height),
            "-depth",
            "8",
        ])
        .arg(format!("rgba:{raw}"))
        .arg(path)
        .status();
    let _ = std::fs::remove_file(Path::new(&raw));
    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("conversion de la capture impossible");
    }
}

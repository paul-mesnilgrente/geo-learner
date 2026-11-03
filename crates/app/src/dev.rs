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
    /// Onglet Progression avec ce nombre de parties fictives dans la zone choisie
    /// (mode « Tous les pays »), plus quelques autres séries.
    pub progress: Option<usize>,
    /// Supprime la partie la plus récente de la série (`delete=1`, bandeau d'annulation).
    pub delete: bool,
    /// Page d'accueil défilée jusqu'en bas (`bottom=1`).
    pub bottom: bool,
    /// Demande de confirmation affichée sur la partie la plus récente (`confirm`).
    pub confirm: bool,
    /// Carte de situation de l'encart n° N ouverte (`globe=N`).
    pub globe: Option<usize>,
    /// Accueil affiché même avec un quiz ou des parties fictives (`home`).
    pub home: bool,
    /// Zoom de la carte de situation figé à cet instant, entre 0 et 1 (`flight=0.5`).
    pub flight: Option<f32>,
    /// Exploration de la carte ouverte (`explore`), avec ce lieu choisi (`explore=FR-75`).
    pub explore: Option<String>,
    /// Langue imposée, en mémoire seulement (`lang=en`).
    pub lang: Option<geo_core::Lang>,
    /// Partie de révision limitée à ces lieux (`review=FRA,DEU`).
    pub review: Vec<String>,
    /// Questions passées après les réponses (`skips=3`).
    pub skips: usize,
}

/// Parties fictives d'une série : scores en progrès, temps en baisse, un minuteur dépassé,
/// et un détail par lieu où chaque lieu s'améliore au fil des parties.
pub fn fake_runs(
    atlas: &str,
    mode: Mode,
    region: &str,
    codes: &[String],
    count: usize,
) -> Vec<crate::progress::Session> {
    let total = codes.len() as u32;
    let now = crate::progress::now();
    (0..count)
        .map(|i| {
            let t = i as f32 / count.max(2) as f32;
            let wobble = ((i * 7919) % 13) as f32 / 13.0 - 0.5;
            let score = (0.45 + 0.5 * t + 0.12 * wobble).clamp(0.0, 1.0);
            let seconds = (780.0 - 330.0 * t + 90.0 * wobble) as u32;
            let limit = (i == count / 3).then_some(seconds);
            crate::progress::Session {
                when: now - (count - i) as u64 * 86_400 * 2 + (i as u64 % 5) * 3600,
                atlas: atlas.into(),
                mode,
                region: region.into(),
                total,
                found: (score * total as f32).round() as u32,
                perfect: (score * total as f32 * 0.9) as u32,
                score,
                seconds,
                missed: vec!["SVN".into(), "GNB".into(), "TUV".into(), "BTN".into()][..(i % 4) + 1]
                    .to_vec(),
                limit,
                review: false,
                answers: codes
                    .iter()
                    .enumerate()
                    .map(|(j, code)| {
                        // Difficulté propre au lieu, qui baisse avec les parties.
                        let hard = (j * 37 % 100) as f32 / 100.0 - 0.6 * t;
                        let roll = ((i * 53 + j * 29) % 100) as f32 / 100.0;
                        let tries = if roll < hard {
                            None
                        } else if roll < hard + 0.15 {
                            Some(2)
                        } else {
                            Some(1)
                        };
                        crate::progress::Attempt {
                            code: code.clone(),
                            tries,
                            ms: 2000 + (j as u32 * 397) % 6000,
                            wrong: Vec::new(),
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

pub fn demo() -> Option<Demo> {
    let spec = std::env::var("GEO_DEMO").ok()?;
    let mut demo = Demo::default();
    for part in spec.split(';') {
        // Les options sans valeur (`giveup`, `summary`…) sont des drapeaux.
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
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
            "progress" => demo.progress = value.parse().ok(),
            "delete" => demo.delete = true,
            "bottom" => demo.bottom = true,
            "confirm" => demo.confirm = true,
            "globe" => demo.globe = value.parse().ok(),
            "home" => demo.home = true,
            "flight" => demo.flight = value.parse().ok(),
            "explore" => demo.explore = Some(value.to_owned()),
            "lang" => demo.lang = geo_core::Lang::from_locale(value),
            "review" => demo.review = value.split(',').map(str::to_owned).collect(),
            "skips" => demo.skips = value.parse().unwrap_or(0),
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

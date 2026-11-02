// Pas de console sous Windows en version release.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
#[cfg(debug_assertions)]
mod dev;
mod icons;
mod map;
mod progress;
mod settings;
mod style;
mod voice;
mod words;

use std::sync::LazyLock;

use geo_core::World;

/// Atlas intégrés à l'exécutable, un par quiz, dans l'ordre du catalogue.
pub static ATLASES: LazyLock<Vec<World>> = LazyLock::new(|| {
    [
        include_bytes!("../../../assets/world.bin").as_slice(),
        include_bytes!("../../../assets/fr-departements.bin"),
        include_bytes!("../../../assets/fr-regions.bin"),
        include_bytes!("../../../assets/us-etats.bin"),
    ]
    .iter()
    .map(|bytes| World::from_bytes(bytes).expect("atlas invalide"))
    .collect()
});

fn main() -> iced::Result {
    let size = iced::Size::new(1360.0, 860.0);
    #[cfg(debug_assertions)]
    let size = dev::window_size().unwrap_or(size);
    iced::application(app::App::new, app::App::update, app::App::view)
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .font(include_bytes!("../../../assets/fonts/Inter-Regular.ttf").as_slice())
        .font(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf").as_slice())
        .font(include_bytes!("../../../assets/fonts/Inter-Bold.ttf").as_slice())
        .default_font(style::FONT)
        .window(iced::window::Settings {
            size,
            min_size: Some(iced::Size::new(960.0, 640.0)),
            icon: iced::window::icon::from_rgba(
                include_bytes!("../../../assets/icon-64.rgba").to_vec(),
                64,
                64,
            )
            .ok(),
            ..Default::default()
        })
        .antialiasing(true)
        .run()
}

#[cfg(test)]
mod tests {
    use super::ATLASES;

    fn owner(territory: &str) -> Option<&'static str> {
        let world = &ATLASES[0];
        let t = world
            .territories
            .iter()
            .find(|t| t.name == territory)
            .expect(territory);
        t.owner.map(|o| world.countries[o].name.as_str())
    }

    #[test]
    fn territories_follow_their_sovereign() {
        assert_eq!(owner("Groenland"), Some("Danemark"));
        assert_eq!(owner("Porto Rico"), Some("États-Unis"));
        assert_eq!(owner("Nouvelle-Calédonie"), Some("France"));
        assert_eq!(owner("Sint Maarten"), Some("Pays-Bas"));
        assert_eq!(owner("Sahara occidental"), None);
    }

    #[test]
    fn atlases_are_complete() {
        let counts: Vec<(String, usize)> = ATLASES
            .iter()
            .map(|a| (a.info.id.clone(), a.countries.len()))
            .collect();
        assert_eq!(
            counts,
            [
                ("monde".to_owned(), 197),
                ("fr-departements".to_owned(), 101),
                ("fr-regions".to_owned(), 18),
                ("us-etats".to_owned(), 50),
            ]
        );
    }
}

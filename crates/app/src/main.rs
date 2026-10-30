// Pas de console sous Windows en version release.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
#[cfg(debug_assertions)]
mod dev;
mod icons;
mod map;
mod progress;
mod style;
mod voice;

use std::sync::LazyLock;

use geo_core::World;

/// Carte et données des pays, intégrées à l'exécutable.
pub static WORLD: LazyLock<World> = LazyLock::new(|| {
    World::from_bytes(include_bytes!("../../../assets/world.bin"))
        .expect("données de carte invalides")
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
    use super::WORLD;

    fn owner(territory: &str) -> Option<&'static str> {
        let t = WORLD
            .territories
            .iter()
            .find(|t| t.name == territory)
            .expect(territory);
        t.owner.map(|o| WORLD.countries[o].name.as_str())
    }

    #[test]
    fn territories_follow_their_sovereign() {
        assert_eq!(owner("Groenland"), Some("Danemark"));
        assert_eq!(owner("Porto Rico"), Some("États-Unis"));
        assert_eq!(owner("Nouvelle-Calédonie"), Some("France"));
        assert_eq!(owner("Sint Maarten"), Some("Pays-Bas"));
        assert_eq!(owner("Sahara occidental"), None);
    }
}

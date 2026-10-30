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
    iced::application(app::App::new, app::App::update, app::App::view)
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .font(include_bytes!("../../../assets/fonts/Inter-Regular.ttf").as_slice())
        .font(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf").as_slice())
        .font(include_bytes!("../../../assets/fonts/Inter-Bold.ttf").as_slice())
        .default_font(style::FONT)
        .window(iced::window::Settings {
            size: iced::Size::new(1360.0, 860.0),
            min_size: Some(iced::Size::new(960.0, 640.0)),
            ..Default::default()
        })
        .antialiasing(true)
        .run()
}

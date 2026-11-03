// Pas de console sous Windows en version release.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod chart;
mod dates;
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

/// Quiz du catalogue, dans l'ordre de l'accueil : un atlas intégré à l'exécutable chacun, sauf
/// les numéros des départements, dérivés des départements (sans carte, rien de plus à stocker).
pub static ATLASES: LazyLock<Vec<World>> = LazyLock::new(|| {
    let load = |bytes: &[u8]| World::from_bytes(bytes).expect("atlas invalide");
    let departements = load(include_bytes!("../../../assets/fr-departements.bin"));
    let numbers = numbers_quiz(&departements);
    vec![
        load(include_bytes!("../../../assets/world.bin")),
        departements,
        numbers,
        load(include_bytes!("../../../assets/fr-regions.bin")),
        load(include_bytes!("../../../assets/us-etats.bin")),
        load(include_bytes!("../../../assets/ca-provinces.bin")),
        load(include_bytes!("../../../assets/au-etats.bin")),
    ]
});

/// Quiz « Numéros des départements » : mêmes lieux et mêmes zones, sans carte.
fn numbers_quiz(departements: &World) -> World {
    let mut world = departements.clone();
    world.info.id = "fr-numeros".into();
    world.info.title = "Numéros des départements".into();
    world.info.numbers = true;
    world
}

/// Numéro d'un lieu (« 75 », « 2A », « 971 ») : sa première réponse qui contient un chiffre.
pub fn number_of(country: &geo_core::Country) -> Option<String> {
    country
        .answers
        .iter()
        .find(|a| a.chars().any(|c| c.is_ascii_digit()))
        .map(|a| a.to_uppercase())
}

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

    /// « Centre » est le début de « Centre-Val de Loire » : il ne doit pas suffire.
    #[test]
    fn partial_region_names_are_not_answers() {
        let regions = ATLASES.iter().find(|a| a.info.id == "fr-regions").unwrap();
        let matcher = geo_core::Matcher::new(regions);
        assert_eq!(matcher.lookup("Centre"), None);
        assert!(matcher.lookup("Centre-Val de Loire").is_some());
        assert!(matcher.lookup("PACA").is_some());
    }

    /// Chaque encart connaît la position réelle de ses terres (carte de situation).
    #[test]
    fn insets_know_where_they_really_are() {
        let us = ATLASES.iter().find(|a| a.info.id == "us-etats").unwrap();
        let [x0, y0, x1, y1] = us.info.insets[1].origin; // Hawaï : tout l'archipel principal
        assert!(x0 < -159.0 && x1 > -155.5 && y0 > 18.0 && y1 < 23.0);
        let fr = ATLASES
            .iter()
            .find(|a| a.info.id == "fr-departements")
            .unwrap();
        let [x0, y0, x1, y1] = fr.info.insets[3].origin; // La Réunion
        assert!((55.0..56.0).contains(&((x0 + x1) / 2.0)));
        assert!((-21.5..-20.8).contains(&((y0 + y1) / 2.0)));
        // Le déplacement inverse ramène la Guadeloupe de son encart à sa vraie place.
        let gp = fr.countries.iter().find(|c| c.code == "FR-GP").unwrap();
        let inset = fr
            .info
            .insets
            .iter()
            .find(|i| i.contains(gp.label))
            .unwrap();
        let [lon, lat] = inset.real(gp.label);
        assert!(
            (-62.0..-61.0).contains(&lon) && (15.8..16.6).contains(&lat),
            "{lon} {lat}"
        );
    }

    /// Chaque département a un numéro, unique, avec ses zéros (« 01 », pas « 1 »).
    #[test]
    fn every_departement_has_a_number() {
        let numbers = ATLASES.iter().find(|a| a.info.numbers).unwrap();
        let list: Vec<String> = numbers
            .countries
            .iter()
            .map(|c| super::number_of(c).unwrap_or_else(|| panic!("{} sans numéro", c.name)))
            .collect();
        let mut unique = list.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 101);
        let name = |n: &str| &numbers.countries[list.iter().position(|x| x == n).unwrap()].name;
        assert_eq!(name("01"), "Ain");
        assert_eq!(name("2A"), "Corse-du-Sud");
        assert_eq!(name("75"), "Paris");
        assert_eq!(name("976"), "Mayotte");
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
                ("fr-numeros".to_owned(), 101),
                ("fr-regions".to_owned(), 18),
                ("us-etats".to_owned(), 50),
                ("ca-provinces".to_owned(), 13),
                ("au-etats".to_owned(), 8),
            ]
        );
    }
}

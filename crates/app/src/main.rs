// Pas de console sous Windows en version release.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod chart;
mod dates;
#[cfg(debug_assertions)]
mod dev;
mod flags;
mod i18n;
mod icons;
mod links;
mod map;
mod progress;
mod settings;
mod style;
mod voice;
mod words;

use std::sync::LazyLock;

use geo_core::World;

/// Quiz du catalogue, dans l'ordre de l'accueil : un atlas intégré à l'exécutable chacun, sauf
/// les numéros des départements, dérivés des départements (sans carte, rien de plus à stocker),
/// et les drapeaux, dérivés du monde (seuls les drapeaux sont stockés, dans `flags.bin`).
/// Les océans, les mers et les fleuves reprennent les terres de l'atlas du monde.
pub static ATLASES: LazyLock<Vec<World>> = LazyLock::new(|| {
    let load = |bytes: &[u8]| World::from_bytes(bytes).expect("atlas invalide");
    let world = load(include_bytes!("../../../assets/world.bin"));
    let oceans = load(include_bytes!("../../../assets/oceans.bin"));
    // L'Antarctique, embarqué une seule fois, avec les océans.
    let antarctica = oceans.territories.clone();
    let oceans = with_land(oceans, &world, &[]);
    let seas = with_land(
        load(include_bytes!("../../../assets/seas.bin")),
        &world,
        &antarctica,
    );
    let rivers = with_land(
        load(include_bytes!("../../../assets/rivers.bin")),
        &world,
        &[],
    );
    let flags = flags_quiz(&world);
    let mut departements = load(include_bytes!("../../../assets/fr-departements.bin"));
    add_number_zones(&mut departements);
    let numbers = numbers_quiz(&departements);
    vec![
        world,
        flags,
        oceans,
        seas,
        rivers,
        departements,
        numbers,
        load(include_bytes!("../../../assets/fr-regions.bin")),
        load(include_bytes!("../../../assets/us-etats.bin")),
        load(include_bytes!("../../../assets/ca-provinces.bin")),
        load(include_bytes!("../../../assets/au-etats.bin")),
    ]
});

/// Quiz « Numéros des départements » : mêmes lieux et mêmes zones, sans carte, plus une zone
/// par dizaine (01–09, 10–19…) pour apprendre par petits paquets.
fn numbers_quiz(departements: &World) -> World {
    let mut world = departements.clone();
    world.info.id = "fr-numeros".into();
    world.info.title = geo_core::Text::new("Numéros des départements", "Department numbers");
    world.info.numbers = true;
    world
}

/// Quiz « Drapeaux du monde » : les pays du monde, mêmes zones, reconnus à leur drapeau.
fn flags_quiz(world: &World) -> World {
    let mut flags = world.clone();
    flags.info.id = "drapeaux".into();
    flags.info.title = geo_core::Text::new("Drapeaux du monde", "Flags of the world");
    flags.info.flags = true;
    flags
}

/// Quiz sur fond de carte du monde (océans, mers, fleuves) : ses lieux, avec les terres du monde
/// (pays et territoires de l'atlas du monde, sans noms à taper), plus `extra` et les terres qu'il
/// embarque.
fn with_land(mut atlas: World, world: &World, extra: &[geo_core::Territory]) -> World {
    let land = world
        .countries
        .iter()
        .map(|c| geo_core::Territory {
            name: c.name.clone(),
            answers: Vec::new(),
            owner: None,
            label: c.label,
            shape: c.shape.clone(),
        })
        .chain(world.territories.iter().map(|t| geo_core::Territory {
            answers: Vec::new(),
            owner: None,
            ..t.clone()
        }));
    atlas.territories.extend(extra.iter().cloned());
    atlas.territories.extend(land);
    for t in &mut atlas.territories {
        t.answers.clear();
    }
    atlas
}

/// Zones « Par numéro » des départements : une par dizaine (01–09, 10–19…, 971–976), pour
/// apprendre par petits paquets, avec ou sans carte.
fn add_number_zones(world: &mut World) {
    // Dizaine d'un département : la Corse (2A, 2B) reste dans les 20, l'outre-mer à part.
    let tens = |c: &geo_core::Country| -> Option<u32> {
        let number = number_of(c)?;
        match number.as_str() {
            "2A" | "2B" => Some(2),
            n if n.len() == 3 => Some(97),
            n => n.parse::<u32>().ok().map(|n| n / 10),
        }
    };
    let mut groups: Vec<(u32, Vec<usize>)> = Vec::new();
    for (i, c) in world.countries.iter().enumerate() {
        let Some(t) = tens(c) else { continue };
        match groups.iter_mut().find(|(k, _)| *k == t) {
            Some((_, list)) => list.push(i),
            None => groups.push((t, vec![i])),
        }
    }
    groups.sort_by_key(|(k, _)| *k);

    let all = &world.regions[0];
    let parent = world.regions.len();
    let continent = geo_core::Region {
        id: "par-numero".into(),
        name: geo_core::Text::new("Par numéro", "By number"),
        kind: geo_core::RegionKind::Continent,
        parent: None,
        members: all.members.clone(),
        center: all.center,
        view: all.view,
        hidden: false,
    };
    world.regions.push(continent);
    for (t, members) in groups {
        let name = match t {
            0 => "01–09".to_owned(),
            9 => "90–95".to_owned(),
            97 => "971–976".to_owned(),
            t => format!("{t}0–{t}9"),
        };
        let view = members_view(world, &members);
        world.regions.push(geo_core::Region {
            id: format!("numeros-{t}"),
            name: geo_core::Text::new(&name, &name),
            kind: geo_core::RegionKind::Group,
            parent: Some(parent),
            members,
            center: world.regions[0].center,
            view,
            hidden: false,
        });
    }
}

/// Cadrage (lon, lat) englobant le territoire principal de ces lieux, avec une marge.
fn members_view(world: &World, members: &[usize]) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for &m in members {
        let shape = &world.countries[m].shape;
        for (ring, _) in shape
            .rings
            .iter()
            .zip(&shape.main)
            .filter(|(_, main)| **main)
        {
            for &[x, y] in ring {
                b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            }
        }
    }
    let margin = 0.08 * (b[2] - b[0]).max(b[3] - b[1]);
    [b[0] - margin, b[1] - margin, b[2] + margin, b[3] + margin]
}

/// Numéro d'un lieu (« 75 », « 2A », « 971 ») : sa première réponse qui contient un chiffre.
pub fn number_of(country: &geo_core::Country) -> Option<String> {
    country
        .answers
        .iter()
        .find(|a| a.chars().any(|c| c.is_ascii_digit()))
        .map(|a| a.to_uppercase())
}

/// Identifiant de l'application sous Linux : nom du fichier .desktop, du metainfo et de l'icône
/// (voir `packaging/linux/`).
#[cfg(target_os = "linux")]
const APP_ID: &str = "com.paul_mesnilgrente.GeoLearner";

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
            // Rattache la fenêtre à son fichier .desktop (icône et nom dans le dock).
            #[cfg(target_os = "linux")]
            platform_specific: iced::window::settings::PlatformSpecific {
                application_id: APP_ID.to_string(),
                ..Default::default()
            },
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
            .find(|t| t.name.get(geo_core::Lang::Fr) == territory)
            .expect(territory);
        t.owner
            .map(|o| world.countries[o].name.get(geo_core::Lang::Fr))
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
            .map(|c| super::number_of(c).unwrap_or_else(|| panic!("{} sans numéro", c.code)))
            .collect();
        let mut unique = list.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 101);
        let name = |n: &str| {
            numbers.countries[list.iter().position(|x| x == n).unwrap()]
                .name
                .get(geo_core::Lang::Fr)
        };
        assert_eq!(name("01"), "Ain");
        assert_eq!(name("2A"), "Corse-du-Sud");
        assert_eq!(name("75"), "Paris");
        assert_eq!(name("976"), "Mayotte");
    }

    /// Les dizaines couvrent chaque département une fois : 01–09, …, 90–95, 971–976.
    #[test]
    fn numbers_quiz_has_one_zone_per_ten() {
        let numbers = ATLASES.iter().find(|a| a.info.numbers).unwrap();
        let tens: Vec<&geo_core::Region> = numbers
            .regions
            .iter()
            .filter(|r| r.id.starts_with("numeros-"))
            .collect();
        let names: Vec<&str> = tens
            .iter()
            .map(|r| r.name.get(geo_core::Lang::Fr))
            .collect();
        assert_eq!(
            names,
            [
                "01–09",
                "10–19",
                "20–29",
                "30–39",
                "40–49",
                "50–59",
                "60–69",
                "70–79",
                "80–89",
                "90–95",
                "971–976"
            ]
        );
        assert_eq!(tens.iter().map(|r| r.members.len()).sum::<usize>(), 101);
        // La Corse avec les 20 : 2A, 2B et 21 à 29.
        assert_eq!(tens[2].members.len(), 11);
    }

    #[test]
    fn departements_can_be_played_by_number() {
        let departements = ATLASES
            .iter()
            .find(|a| a.info.id == "fr-departements")
            .unwrap();
        let numbers = ATLASES.iter().find(|a| a.info.numbers).unwrap();
        let ids = |w: &geo_core::World| -> Vec<String> {
            w.regions
                .iter()
                .filter(|r| r.id.starts_with("numeros-") || r.id == "par-numero")
                .map(|r| r.id.clone())
                .collect()
        };
        assert_eq!(ids(departements).len(), 12);
        assert_eq!(ids(departements), ids(numbers));
    }

    /// Océan qui contient un point (lon, lat) : règle pair-impair sur tous ses anneaux.
    fn ocean_at(oceans: &geo_core::World, [x, y]: [f32; 2]) -> Option<&str> {
        let inside = |ring: &[[f32; 2]]| {
            let mut c = false;
            for (i, &[xi, yi]) in ring.iter().enumerate() {
                let [xj, yj] = ring[(i + ring.len() - 1) % ring.len()];
                if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                    c = !c;
                }
            }
            c
        };
        oceans
            .countries
            .iter()
            .find(|o| o.shape.rings.iter().filter(|r| inside(r)).count() % 2 == 1)
            .map(|o| o.code.as_str())
    }

    #[test]
    fn seas_belong_to_their_ocean() {
        let oceans = ATLASES.iter().find(|a| a.info.water).unwrap();
        for (sea, point, ocean) in [
            ("Méditerranée", [18.0, 35.0], Some("ATL")),
            ("mer Noire", [34.0, 43.0], Some("ATL")),
            ("Baltique", [19.0, 57.0], Some("ATL")),
            ("Caraïbes", [-75.0, 15.0], Some("ATL")),
            ("mer de Norvège", [2.0, 66.0], Some("ATL")),
            (
                "passage de Drake, à l'est du cap Horn",
                [-62.0, -58.0],
                Some("ATL"),
            ),
            ("mer Rouge", [38.0, 20.0], Some("IND")),
            ("golfe Persique", [51.0, 27.0], Some("IND")),
            ("mer de Timor", [126.0, -11.0], Some("IND")),
            ("mer d'Andaman", [96.0, 10.0], Some("IND")),
            ("mer de Béring", [-178.0, 58.0], Some("PAC")),
            ("mer de Java", [110.0, -5.0], Some("PAC")),
            ("mer de Corail", [155.0, -15.0], Some("PAC")),
            ("mer d'Arafura", [135.0, -9.5], Some("PAC")),
            ("golfe de Thaïlande", [101.5, 10.0], Some("PAC")),
            (
                "passage de Drake, à l'ouest du cap Horn",
                [-70.0, -58.0],
                Some("PAC"),
            ),
            ("baie d'Hudson", [-85.0, 60.0], Some("ARC")),
            ("mer de Barents", [40.0, 73.0], Some("ARC")),
            ("mer du Groenland", [-5.0, 75.0], Some("ARC")),
            ("mer des Tchouktches", [-170.0, 70.0], Some("ARC")),
            ("mer de Weddell", [-40.0, -70.0], Some("SOU")),
            ("Caspienne", [51.0, 42.0], None),
        ] {
            assert_eq!(ocean_at(oceans, point), ocean, "{sea}");
        }
    }

    /// Chaque mer est dans l'océan de sa zone, tel que le dessine le quiz des océans (la
    /// Caspienne dans aucun).
    #[test]
    fn seas_match_the_oceans_quiz() {
        let oceans = ATLASES.iter().find(|a| a.info.id == "oceans").unwrap();
        let seas = ATLASES.iter().find(|a| a.info.id == "mers").unwrap();
        for (i, sea) in seas.countries.iter().enumerate() {
            let zone = &seas.regions[seas.continent_of(i).unwrap()].id;
            let expected = match zone.as_str() {
                "atlantique" => Some("ATL"),
                "pacifique" => Some("PAC"),
                "indien" => Some("IND"),
                "arctique" => Some("ARC"),
                "austral" => Some("SOU"),
                _ => None,
            };
            assert_eq!(ocean_at(oceans, sea.label), expected, "{}", sea.code);
        }
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
                ("drapeaux".to_owned(), 197),
                ("oceans".to_owned(), 5),
                ("mers".to_owned(), 52),
                ("fleuves".to_owned(), 55),
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

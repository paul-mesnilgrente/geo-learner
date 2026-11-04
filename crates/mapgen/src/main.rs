//! Génère les atlas embarqués dans l'application (`assets/*.bin`) à partir des contours
//! Natural Earth et des fichiers de `data/`.
//!
//! ```sh
//! mapgen world <admin0.geojson> <sortie>                          # pays du monde
//! mapgen atlas <atlas.toml> <admin0.geojson> <admin1.geojson> <sortie>  # subdivisions
//! mapgen water <atlas.toml> <admin0.geojson> <marine.geojson> <sortie>  # océans, mers
//! mapgen rivers <atlas.toml> <rivers.geojson> <lakes.geojson> <sortie>  # fleuves
//! mapgen flags <admin0.geojson> <dossier svg> <world.bin> <sortie>     # drapeaux
//! mapgen flag-sheet <flags.bin> <admin0.geojson> <dossier svg> <png>   # planche de contrôle
//! ```
//! (voir `scripts/build-map.sh`).

mod flags;

use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

use geo_core::world::{
    Country, Info, Inset, Lang, Region, RegionKind, Ring, Shape, Territory, Text, World,
};
use geo_core::{Matcher, normalize};
use serde::Deserialize;
use serde_json::Value;

/// Tolérances de simplification (en degrés) des deux niveaux de détail.
#[derive(Clone, Copy, Deserialize)]
struct Tolerance {
    hi: f64,
    lo: f64,
}

/// Pays du monde (Natural Earth 1:50m).
const WORLD_TOLERANCE: Tolerance = Tolerance { hi: 0.01, lo: 0.08 };
/// Subdivisions (Natural Earth 1:10m) : départements, États…
const ATLAS_TOLERANCE: Tolerance = Tolerance {
    hi: 0.004,
    lo: 0.02,
};
/// Entités Natural Earth ignorées.
const SKIPPED: &[&str] = &["ATA"];

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
struct CountriesFile {
    countries: Vec<CountryDef>,
    merge: Vec<MergeDef>,
}

#[derive(Deserialize)]
struct CountryDef {
    code: String,
    name: String,
    sub: String,
    #[serde(default)]
    aka: Vec<String>,
    #[serde(default)]
    say: Vec<String>,
    /// Nom anglais, s'il diffère de celui de Natural Earth (ou s'il en manque un).
    en: Option<String>,
    /// Formes orales anglaises supplémentaires.
    #[serde(default)]
    say_en: Vec<String>,
    label: Option<[f32; 2]>,
    #[serde(default)]
    bubble: bool,
    #[serde(default)]
    one_bubble: bool,
    /// Atlas : entités Natural Earth réunies en un seul lieu (départements d'une région).
    #[serde(default)]
    parts: Vec<String>,
    /// Atlas d'eau : contour (lon, lat) dessiné sous les terres, en un ou plusieurs anneaux.
    #[serde(default)]
    rings: Vec<Ring>,
    /// Atlas d'eau : entités marines de Natural Earth (par leur nom anglais) qui forment le lieu.
    #[serde(default)]
    marine: Vec<String>,
    /// Fleuves : tronçons de Natural Earth (par leur nom) qui forment le cours ; avec `rings`, des
    /// tracés ajoutés à la main là où les données manquent.
    #[serde(default)]
    river: Vec<String>,
    /// Fleuves : ne garder que les tronçons compris dans ce cadre [lon_min, lat_min, lon_max,
    /// lat_max] (homonymes : Colorado des États-Unis et d'Argentine).
    within: Option<[f32; 4]>,
}

#[derive(Deserialize)]
struct MergeDef {
    from: String,
    into: String,
}

#[derive(Deserialize)]
struct RegionsFile {
    world: ZoneDef,
    #[serde(default)]
    continent: Vec<ZoneDef>,
    #[serde(default)]
    sub: Vec<ZoneDef>,
    #[serde(default)]
    group: Vec<ZoneDef>,
}

#[derive(Deserialize)]
struct ZoneDef {
    #[serde(default)]
    id: String,
    name: String,
    /// Nom anglais.
    en: Option<String>,
    continent: Option<String>,
    view: Option<[f32; 4]>,
    center: Option<f32>,
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    hidden: bool,
}

/// Fichier d'un atlas de subdivisions (`data/atlases/*.toml`).
#[derive(Deserialize)]
struct AtlasFile {
    atlas: AtlasDef,
    #[serde(flatten)]
    zones: RegionsFile,
    #[serde(default)]
    inset: Vec<InsetDef>,
    items: Vec<CountryDef>,
}

#[derive(Deserialize)]
struct AtlasDef {
    id: String,
    title: String,
    category: String,
    one: String,
    many: String,
    #[serde(default)]
    feminine: bool,
    #[serde(default)]
    zoom_sub: bool,
    #[serde(default = "default_levels")]
    levels: [String; 2],
    /// Textes anglais : titre, catégorie, noms des lieux (singulier, pluriel), niveaux de zones.
    title_en: String,
    category_en: String,
    one_en: String,
    many_en: String,
    #[serde(default = "default_levels_en")]
    levels_en: [String; 2],
    /// Pays (code ADM0_A3) dont les subdivisions sont lues dans Natural Earth admin-1.
    #[serde(default)]
    country: String,
    /// Les terres des autres pays dans ce cadre [lon_min, lat_min, lon_max, lat_max] sont
    /// dessinées en gris, pour situer la carte.
    #[serde(default)]
    context: [f32; 4],
    /// Atlas d'eau : terres (codes ADM0_A3) embarquées, en plus de celles du monde reprises à
    /// l'exécution (l'Antarctique, absent de l'atlas du monde).
    #[serde(default)]
    land: Vec<String>,
    /// Simplification des contours, en degrés (`{ hi = 0.01, lo = 0.08 }`) : plus forte pour
    /// un grand pays vu de loin (Canada).
    tolerance: Option<Tolerance>,
}

fn default_levels() -> [String; 2] {
    ["Continents".into(), "Régions".into()]
}

fn default_levels_en() -> [String; 2] {
    ["Continents".into(), "Regions".into()]
}

fn levels(fr: &[String; 2], en: &[String; 2]) -> [Text; 2] {
    [Text::new(&fr[0], &en[0]), Text::new(&fr[1], &en[1])]
}

/// Encart : entités déplacées (et mises à l'échelle) près de la carte principale.
#[derive(Deserialize)]
struct InsetDef {
    codes: Vec<String>,
    /// Nouveau centre (lon, lat).
    to: [f64; 2],
    scale: f64,
    /// Ne garder que les anneaux dont le centre est dans ce cadre (îles éloignées écartées).
    crop: Option<[f64; 4]>,
}

/// Contours et métadonnées lus dans Natural Earth, pour chaque lieu d'un atlas.
struct Source {
    rings: Vec<Vec<Ring>>,
    labels: Vec<Option<[f32; 2]>>,
    english: Vec<Option<String>>,
    territories: Vec<Territory>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage : mapgen world <admin0> <sortie> | mapgen atlas <toml> <admin0> <admin1> <sortie> \
         | mapgen water <toml> <admin0> <marine> <sortie> | mapgen rivers <toml> <rivers> <lakes> <sortie> \
         | mapgen flags <admin0> <svg> <world.bin> <sortie> | mapgen flag-sheet <flags.bin> <admin0> <svg> <png>";
    let read_json = |path: &str| -> Result<Value> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    };
    let arg = |i: usize| -> Result<&str> { Ok(args.get(i).ok_or(usage)?.as_str()) };
    match args.get(1).map(String::as_str) {
        Some("flags") => {
            let admin0 = read_json(arg(2)?)?;
            let world = World::from_bytes(&std::fs::read(arg(4)?)?)?;
            let list = flags::build(&admin0, Path::new(arg(3)?), &world)?;
            let bytes = geo_core::Flag::list_to_bytes(&list)?;
            std::fs::write(arg(5)?, &bytes)?;
            let mut sizes: Vec<(usize, &str)> = list
                .iter()
                .map(|f| {
                    (
                        postcard::to_allocvec(f).map_or(0, |b| b.len()),
                        f.code.as_str(),
                    )
                })
                .collect();
            sizes.sort_unstable_by(|a, b| b.cmp(a));
            println!(
                "Drapeaux : {} → {} ({} Ko) ; les plus lourds : {:?}",
                list.len(),
                arg(5)?,
                bytes.len() / 1024,
                &sizes[..8.min(sizes.len())]
            );
            return Ok(());
        }
        Some("flag-sheet") => {
            let list = geo_core::Flag::list_from_bytes(&std::fs::read(arg(2)?)?)?;
            let admin0 = read_json(arg(3)?)?;
            let iso = |code: &str| -> Option<String> {
                if code == "KOS" {
                    return Some("xk".into());
                }
                admin0["features"].as_array()?.iter().find_map(|f| {
                    let p = &f["properties"];
                    (p["ADM0_A3"].as_str() == Some(code))
                        .then(|| p["ISO_A2_EH"].as_str().map(str::to_lowercase))
                        .flatten()
                })
            };
            flags::sheet(&list, Path::new(arg(4)?), &iso, Path::new(arg(5)?))?;
            return Ok(());
        }
        _ => {}
    }
    let (world, output) = match args.get(1).map(String::as_str) {
        Some("world") => {
            let countries: CountriesFile =
                toml::from_str(&std::fs::read_to_string("data/countries.toml")?)?;
            let regions: RegionsFile =
                toml::from_str(&std::fs::read_to_string("data/regions.toml")?)?;
            let admin0 = read_json(args.get(2).ok_or(usage)?)?;
            (
                build_world(&countries, &regions, &admin0)?,
                args.get(3).ok_or(usage)?,
            )
        }
        Some("atlas") => {
            let atlas: AtlasFile =
                toml::from_str(&std::fs::read_to_string(args.get(2).ok_or(usage)?)?)?;
            let admin0 = read_json(args.get(3).ok_or(usage)?)?;
            let admin1 = read_json(args.get(4).ok_or(usage)?)?;
            (
                build_atlas(&atlas, &admin0, &admin1)?,
                args.get(5).ok_or(usage)?,
            )
        }
        Some("rivers") => {
            let atlas: AtlasFile =
                toml::from_str(&std::fs::read_to_string(args.get(2).ok_or(usage)?)?)?;
            let rivers = read_json(args.get(3).ok_or(usage)?)?;
            let lakes = read_json(args.get(4).ok_or(usage)?)?;
            (
                build_rivers(&atlas, &rivers, &lakes)?,
                args.get(5).ok_or(usage)?,
            )
        }
        Some("water") => {
            let atlas: AtlasFile =
                toml::from_str(&std::fs::read_to_string(args.get(2).ok_or(usage)?)?)?;
            let admin0 = read_json(args.get(3).ok_or(usage)?)?;
            let marine = read_json(args.get(4).ok_or(usage)?)?;
            (
                build_water(&atlas, &admin0, &marine)?,
                args.get(5).ok_or(usage)?,
            )
        }
        _ => return Err(usage.into()),
    };
    let bytes = world.to_bytes()?;
    std::fs::write(output, &bytes)?;

    let points = |lo: bool| -> usize {
        world
            .countries
            .iter()
            .map(|c| &c.shape)
            .chain(world.territories.iter().map(|t| &t.shape))
            .flat_map(|s| if lo { &s.rings_lo } else { &s.rings })
            .map(Vec::len)
            .sum()
    };
    if !world.lakes.is_empty() {
        let lake_points: usize = world
            .lakes
            .iter()
            .flat_map(|s| s.rings.iter().chain(&s.rings_lo))
            .map(Vec::len)
            .sum();
        println!("{} lacs, {lake_points} points", world.lakes.len());
    }
    println!(
        "{} : {} {}, {} terres voisines, {} zones, {} points (détaillé) / {} (simplifié) → {} ({} Ko)",
        world.info.title.get(Lang::Fr),
        world.countries.len(),
        world.info.many.get(Lang::Fr),
        world.territories.len(),
        world.regions.len(),
        points(false),
        points(true),
        output,
        bytes.len() / 1024
    );
    Ok(())
}

fn build_world(defs: &CountriesFile, zones: &RegionsFile, features: &Value) -> Result<World> {
    let index: HashMap<&str, usize> = defs
        .countries
        .iter()
        .enumerate()
        .map(|(i, c)| (c.code.as_str(), i))
        .collect();
    let merges: HashMap<&str, &str> = defs
        .merge
        .iter()
        .map(|m| (m.from.as_str(), m.into.as_str()))
        .collect();

    let n = defs.countries.len();
    let mut source = Source {
        rings: vec![Vec::new(); n],
        labels: vec![None; n],
        english: vec![None; n],
        territories: Vec::new(),
    };
    // Territoire → partie principale (HOMEPART) de son État souverain.
    let mut homes: HashMap<&str, &str> = HashMap::new();
    for feature in features["features"].as_array().ok_or("GeoJSON invalide")? {
        let props = &feature["properties"];
        if props["HOMEPART"].as_i64() == Some(1)
            && let (Some(sov), Some(code)) = (props["SOV_A3"].as_str(), props["ADM0_A3"].as_str())
        {
            homes.insert(sov, code);
        }
    }

    for feature in features["features"].as_array().ok_or("GeoJSON invalide")? {
        let props = &feature["properties"];
        let code = props["ADM0_A3"].as_str().ok_or("ADM0_A3 manquant")?;
        if SKIPPED.contains(&code) {
            continue;
        }
        let polygons = read_rings(&feature["geometry"])?;
        let target = index
            .get(code)
            .copied()
            .or_else(|| merges.get(code).and_then(|into| index.get(into).copied()));
        match target {
            Some(i) => {
                source.rings[i].extend(polygons);
                if defs.countries[i].code == code {
                    let x = props["LABEL_X"].as_f64().unwrap_or(0.0) as f32;
                    let y = props["LABEL_Y"].as_f64().unwrap_or(0.0) as f32;
                    source.labels[i] = Some([x, y]);
                    source.english[i] = props["NAME_EN"].as_str().map(str::to_owned);
                }
            }
            None => {
                let owner = props["SOV_A3"]
                    .as_str()
                    .and_then(|sov| homes.get(sov))
                    .and_then(|home| index.get(home).copied());
                source
                    .territories
                    .push(territory(props, code, owner, polygons, WORLD_TOLERANCE));
            }
        }
    }

    let info = Info {
        id: "monde".into(),
        title: Text::new("Pays du monde", "Countries of the world"),
        category: Text::new("Monde", "World"),
        one: Text::new("pays", "country"),
        many: Text::new("pays", "countries"),
        feminine: false,
        zoom_sub: false,
        levels: levels(
            &default_levels(),
            &["Continents".into(), "Subregions".into()],
        ),
        insets: Vec::new(),
        numbers: false,
        water: false,
        lines: false,
        flags: false,
    };
    assemble(info, &defs.countries, zones, source, WORLD_TOLERANCE)
}

/// Terre affichée sans être à trouver (territoire, pays voisin d'un atlas).
fn territory(
    props: &Value,
    code: &str,
    owner: Option<usize>,
    polygons: Vec<Ring>,
    tolerance: Tolerance,
) -> Territory {
    let mut name = props["NAME_FR"].as_str().unwrap_or(code).to_owned();
    let mut en = props["NAME_EN"].as_str().unwrap_or(&name).to_owned();
    if code == "SXM" {
        // Natural Earth nomme aussi « Saint-Martin » la partie néerlandaise.
        name = "Sint Maarten".into();
        en = "Sint Maarten".into();
    }
    let name = capitalize(&name);
    let mut answers = vec![normalize(&name)];
    push_unique(&mut answers, normalize(&en));
    Territory {
        name: Text::new(name, en),
        answers,
        owner,
        label: [
            props["LABEL_X"].as_f64().unwrap_or(0.0) as f32,
            props["LABEL_Y"].as_f64().unwrap_or(0.0) as f32,
        ],
        shape: make_shape(polygons, tolerance),
    }
}

fn build_atlas(file: &AtlasFile, admin0: &Value, admin1: &Value) -> Result<World> {
    let def = &file.atlas;
    let tolerance = def.tolerance.unwrap_or(ATLAS_TOLERANCE);
    // Entités admin-1 du pays, par code ISO 3166-2. Un code peut en désigner plusieurs
    // (Nouvelle-Galles du Sud et île Lord Howe) : la plus étendue d'abord, elle donne le
    // point d'étiquette et le nom anglais.
    let mut features: HashMap<&str, Vec<(f64, &Value)>> = HashMap::new();
    for feature in admin1["features"].as_array().ok_or("GeoJSON invalide")? {
        let props = &feature["properties"];
        if props["adm0_a3"].as_str() == Some(def.country.as_str())
            && let Some(code) = props["iso_3166_2"].as_str()
        {
            let b = bounds_f64(read_rings_f64(&feature["geometry"])?.iter().flatten());
            let area = (b[2] - b[0]) * (b[3] - b[1]);
            features.entry(code).or_default().push((area, feature));
        }
    }
    for list in features.values_mut() {
        list.sort_by(|a, b| b.0.total_cmp(&a.0));
    }
    let features: HashMap<&str, Vec<&Value>> = features
        .into_iter()
        .map(|(code, list)| (code, list.into_iter().map(|(_, f)| f).collect()))
        .collect();

    // Encarts : déplacement de chaque entité concernée.
    let mut moves: HashMap<&str, (Move, Option<[f64; 4]>)> = HashMap::new();
    let mut insets = Vec::new();
    for inset in &file.inset {
        let mut rings = Vec::new();
        for code in &inset.codes {
            let list = features
                .get(code.as_str())
                .ok_or_else(|| format!("encart : {code} inconnu"))?;
            for feature in list {
                rings.extend(cropped(read_rings_f64(&feature["geometry"])?, inset.crop));
            }
        }
        let shift = Move::new(&rings, inset.to, inset.scale);
        let moved: Vec<Vec<[f64; 2]>> = rings
            .iter()
            .map(|r| r.iter().map(|&p| shift.apply(p)).collect())
            .collect();
        let b = bounds_f64(moved.iter().flatten());
        let pad = 0.35;
        // Position réelle : emprise de toutes ces terres, ou de la plus grande si elles passent
        // l'antiméridien (Alaska et ses Aléoutiennes).
        let all = bounds_f64(rings.iter().flatten());
        let area = |b: &[f64; 4]| (b[2] - b[0]) * (b[3] - b[1]);
        let origin = if all[2] - all[0] < 180.0 {
            all
        } else {
            rings
                .iter()
                .map(|r| bounds_f64(r.iter()))
                .max_by(|a, b| area(a).total_cmp(&area(b)))
                .ok_or("encart vide")?
        };
        insets.push(Inset {
            frame: [
                (b[0] - pad) as f32,
                (b[1] - pad) as f32,
                (b[2] + pad) as f32,
                (b[3] + pad) as f32,
            ],
            origin: origin.map(|v| v as f32),
            from: shift.from.map(|v| v as f32),
            to: shift.to.map(|v| v as f32),
            factor: [
                (1.0 / (shift.scale * shift.stretch())) as f32,
                (1.0 / shift.scale) as f32,
            ],
        });
        for code in &inset.codes {
            moves.insert(code.as_str(), (shift, inset.crop));
        }
    }

    let n = file.items.len();
    let mut source = Source {
        rings: vec![Vec::new(); n],
        labels: vec![None; n],
        english: vec![None; n],
        territories: Vec::new(),
    };
    for (i, item) in file.items.iter().enumerate() {
        let parts: Vec<&str> = if item.parts.is_empty() {
            vec![item.code.as_str()]
        } else {
            item.parts.iter().map(String::as_str).collect()
        };
        let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut label = [0.0f64; 2];
        for &code in &parts {
            let list = features
                .get(code)
                .ok_or_else(|| format!("{} : entité {code} introuvable", item.name))?;
            let props = &list[0]["properties"];
            let mut part = Vec::new();
            for feature in list {
                part.extend(read_rings_f64(&feature["geometry"])?);
            }
            let mut point = [
                props["longitude"].as_f64().unwrap_or(0.0),
                props["latitude"].as_f64().unwrap_or(0.0),
            ];
            if let Some((shift, crop)) = moves.get(code) {
                part = cropped(part, *crop)
                    .iter()
                    .map(|r| r.iter().map(|&p| shift.apply(p)).collect())
                    .collect();
                point = shift.apply(point);
            }
            label = [
                label[0] + point[0] / parts.len() as f64,
                label[1] + point[1] / parts.len() as f64,
            ];
            rings.extend(part);
            if parts.len() == 1 {
                source.english[i] = props["name_en"].as_str().map(str::to_owned);
            }
        }
        // Plusieurs entités : on ne garde que leur contour commun (région = départements).
        if parts.len() > 1 {
            rings = dissolve(&rings);
        }
        source.rings[i] = rings
            .iter()
            .map(|r| r.iter().map(|&[x, y]| [x as f32, y as f32]).collect())
            .collect();
        source.labels[i] = Some([label[0] as f32, label[1] as f32]);
    }

    // Terres voisines : pays (admin-0) dans le cadre, sauf celui de l'atlas.
    let [x0, y0, x1, y1] = def.context;
    for feature in admin0["features"].as_array().ok_or("GeoJSON invalide")? {
        let props = &feature["properties"];
        let code = props["ADM0_A3"].as_str().unwrap_or("");
        if code == def.country || SKIPPED.contains(&code) {
            continue;
        }
        let polygons = read_rings(&feature["geometry"])?;
        let inside = polygons
            .iter()
            .flatten()
            .any(|&[x, y]| x >= x0 && x <= x1 && y >= y0 && y <= y1);
        if inside {
            source
                .territories
                .push(territory(props, code, None, polygons, tolerance));
        }
    }

    let info = Info {
        id: def.id.clone(),
        title: Text::new(&def.title, &def.title_en),
        category: Text::new(&def.category, &def.category_en),
        one: Text::new(&def.one, &def.one_en),
        many: Text::new(&def.many, &def.many_en),
        feminine: def.feminine,
        zoom_sub: def.zoom_sub,
        levels: levels(&def.levels, &def.levels_en),
        insets,
        numbers: false,
        water: false,
        lines: false,
        flags: false,
    };
    assemble(info, &file.items, &file.zones, source, tolerance)
}

/// Atlas d'eau : chaque lieu est un grand polygone tracé à la main dans le fichier (océans),
/// dont les bords ne comptent qu'en mer (le reste passe sous les terres, qui le recouvrent), ou
/// des entités marines de Natural Earth (mers). Les terres du monde sont reprises à
/// l'exécution ; seules celles de `land` sont embarquées.
fn build_water(file: &AtlasFile, admin0: &Value, marine: &Value) -> Result<World> {
    let def = &file.atlas;
    let n = file.items.len();
    let mut source = Source {
        rings: Vec::with_capacity(n),
        labels: vec![None; n],
        english: vec![None; n],
        territories: Vec::new(),
    };
    let features = marine["features"].as_array().ok_or("GeoJSON invalide")?;
    for (i, item) in file.items.iter().enumerate() {
        let mut rings = item.rings.clone();
        for name in &item.marine {
            let feature = features
                .iter()
                .find(|f| f["properties"]["name"].as_str() == Some(name.as_str()))
                .ok_or_else(|| format!("{} : entité marine « {name} » introuvable", item.name))?;
            rings.extend(read_rings(&feature["geometry"])?);
        }
        if rings.is_empty() {
            return Err(format!("{} : aucun contour (`rings` ou `marine`)", item.name).into());
        }
        // Polygones tracés à la main : densifiés (voir plus bas) ; ceux de Natural Earth sont
        // simplifiés comme les pays.
        if !item.rings.is_empty() {
            rings = rings.iter().map(|r| densify(r, 1.0)).collect();
        }
        source.labels[i] = Some(inner_point(&rings));
        source.rings.push(rings);
    }
    for feature in admin0["features"].as_array().ok_or("GeoJSON invalide")? {
        let props = &feature["properties"];
        let code = props["ADM0_A3"].as_str().unwrap_or("");
        if def.land.iter().any(|c| c == code) {
            // Densifiée : l'Antarctique longe le pôle d'un bord à l'autre de la carte en un seul
            // segment, que la projection doit suivre.
            let polygons = read_rings(&feature["geometry"])?
                .iter()
                .map(|r| densify(r, 1.0))
                .collect();
            source
                .territories
                .push(territory(props, code, None, polygons, WORLD_TOLERANCE));
        }
    }
    let info = Info {
        id: def.id.clone(),
        title: Text::new(&def.title, &def.title_en),
        category: Text::new(&def.category, &def.category_en),
        one: Text::new(&def.one, &def.one_en),
        many: Text::new(&def.many, &def.many_en),
        feminine: def.feminine,
        zoom_sub: false,
        levels: levels(&def.levels, &def.levels_en),
        insets: Vec::new(),
        numbers: false,
        water: true,
        lines: false,
        flags: false,
    };
    let rings: Vec<Vec<Ring>> = source.rings.clone();
    let mut world = assemble(info, &file.items, &file.zones, source, WORLD_TOLERANCE)?;
    // Contours tracés à la main gardés tels quels, densifiés : simplifiés, les bords suivant un
    // méridien ou le bord de la carte redeviendraient des cordes, et laisseraient des jours le
    // long des terres.
    for ((country, rings), item) in world.countries.iter_mut().zip(rings).zip(&file.items) {
        if item.rings.is_empty() {
            continue;
        }
        country.shape.main = vec![true; rings.len()];
        country.shape.rings_lo = rings.clone();
        country.shape.rings = rings;
    }
    Ok(world)
}

/// Tracés des fleuves (Natural Earth 1:10m) : plus simplifiés que les côtes, un fleuve se lit
/// de loin.
const RIVER_TOLERANCE: Tolerance = Tolerance { hi: 0.02, lo: 0.05 };
/// Lacs : leurs rives comptent moins que celles des pays.
const LAKE_TOLERANCE: Tolerance = Tolerance { hi: 0.03, lo: 0.1 };

/// Lacs plus petits que cette emprise (en degrés², corrigée de la latitude) écartés.
const MIN_LAKE: f32 = 1.0;

/// Atlas de fleuves : chaque lieu est un ensemble de tracés ouverts (tronçons de Natural Earth
/// réunis sous le nom du fleuve, et tracés à la main). Les terres du monde sont reprises à
/// l'exécution ; les grands lacs sont embarqués.
fn build_rivers(file: &AtlasFile, rivers: &Value, lakes: &Value) -> Result<World> {
    let def = &file.atlas;
    let n = file.items.len();
    let mut source = Source {
        rings: Vec::with_capacity(n),
        labels: vec![None; n],
        english: vec![None; n],
        territories: Vec::new(),
    };
    let features = rivers["features"].as_array().ok_or("GeoJSON invalide")?;
    for (i, item) in file.items.iter().enumerate() {
        let mut lines = item.rings.clone();
        let within = |line: &Ring| match item.within {
            Some([x0, y0, x1, y1]) => line
                .iter()
                .all(|&[x, y]| (x0..=x1).contains(&x) && (y0..=y1).contains(&y)),
            None => true,
        };
        for name in &item.river {
            let before = lines.len();
            for feature in features
                .iter()
                .filter(|f| f["properties"]["name"].as_str() == Some(name.as_str()))
            {
                lines.extend(
                    read_lines(&feature["geometry"])?
                        .into_iter()
                        .filter(&within),
                );
            }
            if lines.len() == before {
                return Err(format!("{} : tronçon « {name} » introuvable", item.name).into());
            }
        }
        if lines.is_empty() {
            return Err(format!("{} : aucun tracé (`river` ou `rings`)", item.name).into());
        }
        source.labels[i] = Some(midpoint(&lines));
        source.rings.push(lines);
    }
    let info = Info {
        id: def.id.clone(),
        title: Text::new(&def.title, &def.title_en),
        category: Text::new(&def.category, &def.category_en),
        one: Text::new(&def.one, &def.one_en),
        many: Text::new(&def.many, &def.many_en),
        feminine: def.feminine,
        zoom_sub: false,
        levels: levels(&def.levels, &def.levels_en),
        insets: Vec::new(),
        numbers: false,
        water: false,
        lines: true,
        flags: false,
    };
    let mut world = assemble(info, &file.items, &file.zones, source, RIVER_TOLERANCE)?;
    for country in &mut world.countries {
        country.shape.main = vec![true; country.shape.rings.len()];
    }
    for feature in lakes["features"].as_array().ok_or("GeoJSON invalide")? {
        let rings = read_rings(&feature["geometry"])?;
        let Some(outer) = rings.iter().max_by(|a, b| area(a).total_cmp(&area(b))) else {
            continue;
        };
        let [x0, y0, x1, y1] = bounds_f32(outer);
        let extent = (x1 - x0) * (y1 - y0) * ((y0 + y1) / 2.0).to_radians().cos();
        if extent >= MIN_LAKE {
            world.lakes.push(make_shape(rings, LAKE_TOLERANCE));
        }
    }
    Ok(world)
}

/// Tracés d'une géométrie GeoJSON linéaire.
fn read_lines(geometry: &Value) -> Result<Vec<Ring>> {
    let coords = &geometry["coordinates"];
    let lines: Vec<&Value> = match geometry["type"].as_str() {
        Some("LineString") => vec![coords],
        Some("MultiLineString") => coords
            .as_array()
            .ok_or("MultiLineString invalide")?
            .iter()
            .collect(),
        other => return Err(format!("géométrie non gérée : {other:?}").into()),
    };
    Ok(lines
        .into_iter()
        .filter_map(Value::as_array)
        .map(|line| {
            line.iter()
                .map(|p| {
                    [
                        p[0].as_f64().unwrap_or(0.0) as f32,
                        p[1].as_f64().unwrap_or(0.0) as f32,
                    ]
                })
                .collect::<Ring>()
        })
        .filter(|line| line.len() >= 2)
        .collect())
}

/// Milieu (en longueur) du plus long tracé d'un fleuve : point d'ancrage de son nom.
fn midpoint(lines: &[Ring]) -> [f32; 2] {
    let length = |line: &Ring| -> f32 {
        line.windows(2)
            .map(|w| {
                let k = w[0][1].to_radians().cos();
                ((w[1][0] - w[0][0]) * k).hypot(w[1][1] - w[0][1])
            })
            .sum()
    };
    let longest = lines
        .iter()
        .max_by(|a, b| length(a).total_cmp(&length(b)))
        .expect("fleuve sans tracé");
    let half = length(longest) / 2.0;
    let mut walked = 0.0;
    for w in longest.windows(2) {
        let k = w[0][1].to_radians().cos();
        let step = ((w[1][0] - w[0][0]) * k).hypot(w[1][1] - w[0][1]);
        if walked + step >= half && step > 0.0 {
            let t = (half - walked) / step;
            return [
                w[0][0] + (w[1][0] - w[0][0]) * t,
                w[0][1] + (w[1][1] - w[0][1]) * t,
            ];
        }
        walked += step;
    }
    longest[0]
}

/// Point d'un lieu le plus éloigné de ses bords (approché sur une grille), pour son étiquette :
/// le centre d'une mer en croissant (Baltique, mer Noire) peut tomber hors de l'eau.
fn inner_point(rings: &[Ring]) -> [f32; 2] {
    let inside = |[x, y]: [f32; 2]| {
        let mut c = false;
        for ring in rings {
            for (i, &[xi, yi]) in ring.iter().enumerate() {
                let [xj, yj] = ring[(i + ring.len() - 1) % ring.len()];
                if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                    c = !c;
                }
            }
        }
        c
    };
    let edge = |p: [f32; 2]| -> f64 {
        rings
            .iter()
            .flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
            .map(|(a, b)| segment_dist2(p, a, b))
            .fold(f64::MAX, f64::min)
    };
    // Grille sur l'anneau le plus étendu (une mer coupée par l'antiméridien en a deux).
    let largest = rings
        .iter()
        .max_by(|a, b| area(a).total_cmp(&area(b)))
        .expect("lieu sans contour");
    let b = bounds_f32(largest);
    let steps = 60;
    let mut best = (center(largest), -1.0);
    for i in 0..=steps {
        for j in 0..=steps {
            let p = [
                b[0] + (b[2] - b[0]) * i as f32 / steps as f32,
                b[1] + (b[3] - b[1]) * j as f32 / steps as f32,
            ];
            // Distance corrigée de l'écartement des méridiens.
            if inside(p) {
                let d = edge(p) * f64::from(p[1].to_radians().cos().max(0.2));
                if d > best.1 {
                    best = (p, d);
                }
            }
        }
    }
    best.0
}

fn bounds_f32(ring: &[[f32; 2]]) -> [f32; 4] {
    ring.iter().fold(
        [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
        |[a, b, c, d], &[x, y]| [a.min(x), b.min(y), c.max(x), d.max(y)],
    )
}

/// Ajoute des points le long des côtés d'un anneau (fermé implicitement), tous les `step` degrés
/// au plus, pour que la projection les courbe.
fn densify(ring: &[[f32; 2]], step: f32) -> Ring {
    let mut out = Vec::new();
    for (i, &a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        let length = (b[0] - a[0]).abs().max((b[1] - a[1]).abs());
        let parts = (length / step).ceil().max(1.0) as usize;
        for k in 0..parts {
            let t = k as f32 / parts as f32;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// Déplacement d'un encart : translation vers `to` et mise à l'échelle, en corrigeant
/// l'écartement des méridiens pour conserver les formes.
#[derive(Clone, Copy)]
struct Move {
    from: [f64; 2],
    to: [f64; 2],
    scale: f64,
    /// Longitude de référence pour « dérouler » les longitudes (Aléoutiennes au-delà de 180°).
    reference: f64,
}

impl Move {
    fn new(rings: &[Vec<[f64; 2]>], to: [f64; 2], scale: f64) -> Self {
        let reference = rings.first().and_then(|r| r.first()).map_or(0.0, |p| p[0]);
        let mut unwrapped = Move {
            from: [0.0; 2],
            to,
            scale,
            reference,
        };
        let points: Vec<[f64; 2]> = rings
            .iter()
            .flatten()
            .map(|&p| unwrapped.unwrap(p))
            .collect();
        let b = bounds_f64(points.iter());
        unwrapped.from = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        unwrapped
    }

    fn unwrap(&self, [lon, lat]: [f64; 2]) -> [f64; 2] {
        let mut lon = lon;
        while lon - self.reference > 180.0 {
            lon -= 360.0;
        }
        while self.reference - lon > 180.0 {
            lon += 360.0;
        }
        [lon, lat]
    }

    /// Correction de la largeur des degrés de longitude entre les deux latitudes.
    fn stretch(&self) -> f64 {
        self.from[1].to_radians().cos() / self.to[1].to_radians().cos()
    }

    fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        let [lon, lat] = self.unwrap(p);
        let stretch = self.stretch();
        [
            self.to[0] + (lon - self.from[0]) * self.scale * stretch,
            self.to[1] + (lat - self.from[1]) * self.scale,
        ]
    }
}

/// Anneaux dont le centre est dans le cadre (tous sans cadre).
fn cropped(rings: Vec<Vec<[f64; 2]>>, crop: Option<[f64; 4]>) -> Vec<Vec<[f64; 2]>> {
    let Some([x0, y0, x1, y1]) = crop else {
        return rings;
    };
    rings
        .into_iter()
        .filter(|r| {
            let b = bounds_f64(r.iter());
            let (x, y) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            x >= x0 && x <= x1 && y >= y0 && y <= y1
        })
        .collect()
}

fn bounds_f64<'a>(points: impl Iterator<Item = &'a [f64; 2]>) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for &[x, y] in points {
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    b
}

/// Fusionne des polygones voisins : les arêtes communes (parcourues dans les deux sens par deux
/// entités voisines) s'annulent, le reste est rechaîné en anneaux. Natural Earth partage
/// exactement les sommets le long des frontières, ce qui rend la comparaison exacte possible.
/// Point arrondi au dix-millionième de degré, comparable exactement.
type Key = (i64, i64);

fn dissolve(rings: &[Vec<[f64; 2]>]) -> Vec<Vec<[f64; 2]>> {
    let key = |p: [f64; 2]| ((p[0] * 1e7).round() as i64, (p[1] * 1e7).round() as i64);
    let mut count: HashMap<(Key, Key), i32> = HashMap::new();
    let mut edges = Vec::new();
    for ring in rings {
        for w in ring.windows(2) {
            let (a, b) = (key(w[0]), key(w[1]));
            if a == b {
                continue;
            }
            let undirected = if a < b { (a, b) } else { (b, a) };
            *count.entry(undirected).or_default() += 1;
            edges.push((w[0], w[1], a, b));
        }
    }
    // Arêtes de contour, indexées par leur point de départ.
    let mut outgoing: HashMap<Key, Vec<usize>> = HashMap::new();
    let boundary: Vec<_> = edges
        .into_iter()
        .filter(|&(_, _, a, b)| count[&if a < b { (a, b) } else { (b, a) }] == 1)
        .collect();
    for (i, &(_, _, a, _)) in boundary.iter().enumerate() {
        outgoing.entry(a).or_default().push(i);
    }
    let mut used = vec![false; boundary.len()];
    let mut result = Vec::new();
    for start in 0..boundary.len() {
        if used[start] {
            continue;
        }
        let mut ring = vec![boundary[start].0];
        let mut current = start;
        loop {
            used[current] = true;
            let (_, to, _, end) = boundary[current];
            ring.push(to);
            let next = outgoing
                .get(&end)
                .and_then(|list| list.iter().copied().find(|&e| !used[e]));
            match next {
                Some(e) => current = e,
                None => break,
            }
        }
        if ring.len() >= 4 {
            result.push(ring);
        }
    }
    result
}

fn assemble(
    info: Info,
    defs: &[CountryDef],
    zones: &RegionsFile,
    mut source: Source,
    tolerance: Tolerance,
) -> Result<World> {
    let index: HashMap<&str, usize> = defs
        .iter()
        .enumerate()
        .map(|(i, c)| (c.code.as_str(), i))
        .collect();
    // Ordre des régions : monde, continents, sous-régions, groupes.
    let continent_index = |id: &str| -> Result<usize> {
        zones
            .continent
            .iter()
            .position(|c| c.id == id)
            .map(|i| 1 + i)
            .ok_or_else(|| format!("continent inconnu : {id}").into())
    };
    let sub_index = |id: &str| -> Result<usize> {
        zones
            .sub
            .iter()
            .position(|s| s.id == id)
            .map(|i| 1 + zones.continent.len() + i)
            .ok_or_else(|| format!("sous-région inconnue : {id}").into())
    };

    let mut countries = Vec::with_capacity(defs.len());
    for (i, def) in defs.iter().enumerate() {
        if source.rings[i].is_empty() {
            return Err(format!("aucun contour pour {} ({})", def.name, def.code).into());
        }
        let en = def
            .en
            .clone()
            .or_else(|| source.english[i].clone())
            .ok_or_else(|| format!("{} ({}) : nom anglais manquant", def.name, def.code))?;
        let mut answers: Vec<String> = Vec::new();
        let mut spoken: Vec<String> = Vec::new();
        for name in std::iter::once(&def.name).chain(&def.aka) {
            push_unique(&mut answers, normalize(name));
            // Les numéros (« 75 », « 2A ») se tapent mais ne se disent pas.
            if !name.chars().any(|c| c.is_ascii_digit()) {
                for form in spoken_forms(name) {
                    push_unique(&mut spoken, form);
                }
            }
        }
        for form in &def.say {
            push_unique(&mut spoken, form.to_lowercase());
        }
        for form in &spoken {
            push_unique(&mut answers, normalize(form));
        }
        // Anglais : nom (et celui de Natural Earth s'il diffère), formes orales.
        let mut spoken_en: Vec<String> = spoken_forms(&en);
        // Variante sans accents ni apostrophes, plus souvent dans le vocabulaire anglais
        // (« sao tome and principe », « reunion »).
        push_unique(&mut spoken_en, normalize(&en));
        for form in &def.say_en {
            push_unique(&mut spoken_en, form.to_lowercase());
        }
        push_unique(&mut answers, normalize(&en));
        if let Some(ne) = &source.english[i] {
            push_unique(&mut answers, normalize(ne));
        }
        for form in &spoken_en {
            push_unique(&mut answers, normalize(form));
        }
        let mut spoken_by_lang = vec![Vec::new(); Lang::ALL.len()];
        spoken_by_lang[Lang::Fr.index()] = spoken;
        spoken_by_lang[Lang::En.index()] = spoken_en;
        countries.push(Country {
            code: def.code.clone(),
            name: Text::new(&def.name, &en),
            answers,
            spoken: spoken_by_lang,
            sub: sub_index(&def.sub)?,
            label: def.label.or(source.labels[i]).unwrap_or([0.0, 0.0]),
            bubble: def.bubble,
            one_bubble: def.one_bubble,
            shape: make_shape(std::mem::take(&mut source.rings[i]), tolerance),
        });
    }
    check_answers(&countries)?;

    let code_index = |code: &str| -> Result<usize> {
        index
            .get(code)
            .copied()
            .ok_or_else(|| format!("lieu inconnu : {code}").into())
    };
    let members_of_sub = |sub: usize| -> Vec<usize> {
        (0..countries.len())
            .filter(|&c| countries[c].sub == sub)
            .collect()
    };

    let mut regions = Vec::new();
    regions.push(zone(
        &zones.world,
        "monde",
        RegionKind::World,
        None,
        (0..countries.len()).collect(),
        &countries,
    ));
    for (i, c) in zones.continent.iter().enumerate() {
        let continent = 1 + i;
        let members = zones
            .sub
            .iter()
            .enumerate()
            .filter(|(_, s)| s.continent.as_deref() == Some(c.id.as_str()))
            .flat_map(|(j, _)| members_of_sub(1 + zones.continent.len() + j))
            .collect();
        regions.push(zone(
            c,
            &c.id,
            RegionKind::Continent,
            Some(continent),
            members,
            &countries,
        ));
    }
    for (j, s) in zones.sub.iter().enumerate() {
        let parent = continent_index(s.continent.as_deref().ok_or("sous-région sans continent")?)?;
        let members = members_of_sub(1 + zones.continent.len() + j);
        regions.push(zone(
            s,
            &s.id,
            RegionKind::Sub,
            Some(parent),
            members,
            &countries,
        ));
    }
    for g in &zones.group {
        let parent = continent_index(g.continent.as_deref().ok_or("groupe sans continent")?)?;
        let members = g
            .members
            .iter()
            .map(|c| code_index(c))
            .collect::<Result<_>>()?;
        regions.push(zone(
            g,
            &g.id,
            RegionKind::Group,
            Some(parent),
            members,
            &countries,
        ));
    }
    // Les continents se rattachent à eux-mêmes ; ils n'ont pas de parent dans le menu.
    for r in regions
        .iter_mut()
        .filter(|r| r.kind == RegionKind::Continent)
    {
        r.parent = None;
    }
    for r in &regions {
        if r.members.is_empty() {
            return Err(format!("zone vide : {}", r.id).into());
        }
    }

    Ok(World {
        info,
        countries,
        territories: source.territories,
        regions,
        lakes: Vec::new(),
    })
}

fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !value.is_empty() && !list.contains(&value) {
        list.push(value);
    }
}

/// Formes orales d'un nom : minuscules, tirets remplacés par des espaces,
/// avec une variante où l'élision est détachée (« d' ivoire »).
fn spoken_forms(name: &str) -> Vec<String> {
    let base = name.to_lowercase().replace('-', " ").replace('’', "'");
    let base = base.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut forms = vec![base.clone()];
    if base.contains('\'') {
        forms.push(base.replace('\'', "' "));
        forms.push(base.replace('\'', " "));
    }
    forms
}

/// Deux pays ne doivent jamais partager une réponse.
fn check_answers(countries: &[Country]) -> Result<()> {
    let mut seen: HashMap<&str, &str> = HashMap::new();
    for c in countries {
        for a in &c.answers {
            let name = c.name.get(Lang::Fr);
            if let Some(other) = seen.insert(a, name)
                && other != name
            {
                return Err(format!("réponse « {a} » partagée par {other} et {name}").into());
            }
        }
    }
    // Vérifie que chaque pays est retrouvable par son nom.
    let world = World {
        info: Info::default(),
        countries: countries.to_vec(),
        territories: vec![],
        regions: vec![],
        lakes: vec![],
    };
    let matcher = Matcher::new(&world);
    for (i, c) in countries.iter().enumerate() {
        for lang in Lang::ALL {
            let name = c.name.get(lang);
            if matcher.lookup(name) != Some(i) {
                return Err(format!("{name} ne désigne pas {}", c.code).into());
            }
        }
    }
    Ok(())
}

fn zone(
    def: &ZoneDef,
    id: &str,
    kind: RegionKind,
    parent: Option<usize>,
    members: Vec<usize>,
    countries: &[Country],
) -> Region {
    let view = def.view.unwrap_or_else(|| auto_view(&members, countries));
    let center = def.center.unwrap_or((view[0] + view[2]) / 2.0);
    Region {
        id: id.to_owned(),
        name: Text::new(
            &def.name,
            def.en.as_deref().unwrap_or_else(|| {
                eprintln!("zone « {} » sans nom anglais", def.name);
                &def.name
            }),
        ),
        kind,
        parent,
        members,
        center,
        view,
        hidden: def.hidden,
    }
}

/// Cadrage englobant le territoire principal des pays, avec une marge.
fn auto_view(members: &[usize], countries: &[Country]) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for &m in members {
        let shape = &countries[m].shape;
        for (ring, &main) in shape.rings.iter().zip(&shape.main) {
            if !main {
                continue;
            }
            for &[x, y] in ring {
                b[0] = b[0].min(x);
                b[1] = b[1].min(y);
                b[2] = b[2].max(x);
                b[3] = b[3].max(y);
            }
        }
    }
    let pad_x = ((b[2] - b[0]) * 0.12).max(1.5);
    let pad_y = ((b[3] - b[1]) * 0.12).max(1.5);
    [b[0] - pad_x, b[1] - pad_y, b[2] + pad_x, b[3] + pad_y]
}

/// Anneaux d'une géométrie GeoJSON, en double précision (pour la fusion exacte).
fn read_rings_f64(geometry: &Value) -> Result<Vec<Vec<[f64; 2]>>> {
    let coords = &geometry["coordinates"];
    let polygons: Vec<&Value> = match geometry["type"].as_str() {
        Some("Polygon") => vec![coords],
        Some("MultiPolygon") => coords
            .as_array()
            .ok_or("MultiPolygon invalide")?
            .iter()
            .collect(),
        other => return Err(format!("géométrie non gérée : {other:?}").into()),
    };
    let mut rings = Vec::new();
    for polygon in polygons {
        for ring in polygon.as_array().ok_or("polygone invalide")? {
            rings.push(
                ring.as_array()
                    .ok_or("anneau invalide")?
                    .iter()
                    .map(|p| [p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)])
                    .collect(),
            );
        }
    }
    Ok(rings)
}

fn read_rings(geometry: &Value) -> Result<Vec<Ring>> {
    let coords = &geometry["coordinates"];
    let polygons: Vec<&Value> = match geometry["type"].as_str() {
        Some("Polygon") => vec![coords],
        Some("MultiPolygon") => coords
            .as_array()
            .ok_or("MultiPolygon invalide")?
            .iter()
            .collect(),
        other => return Err(format!("géométrie non gérée : {other:?}").into()),
    };
    let mut rings = Vec::new();
    for polygon in polygons {
        for ring in polygon.as_array().ok_or("polygone invalide")? {
            let points: Vec<[f64; 2]> = ring
                .as_array()
                .ok_or("anneau invalide")?
                .iter()
                .map(|p| [p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)])
                .collect();
            rings.push(points.iter().map(|&[x, y]| [x as f32, y as f32]).collect());
        }
    }
    Ok(rings)
}

fn make_shape(rings: Vec<Ring>, tolerance: Tolerance) -> Shape {
    let areas: Vec<f32> = rings.iter().map(|r| area(r)).collect();
    let largest = (0..rings.len())
        .max_by(|&a, &b| areas[a].total_cmp(&areas[b]))
        .unwrap_or(0);
    let anchor = center(&rings[largest]);
    let main = rings
        .iter()
        .zip(&areas)
        .map(|(ring, &a)| {
            let c = center(ring);
            let dx = (c[0] - anchor[0]) * anchor[1].to_radians().cos();
            let dy = c[1] - anchor[1];
            (dx * dx + dy * dy).sqrt() < 25.0 || a > areas[largest] * 0.3
        })
        .collect();
    let hi: Vec<Ring> = rings.iter().map(|r| simplify(r, tolerance.hi)).collect();
    let lo: Vec<Ring> = rings
        .iter()
        .zip(&hi)
        .map(|(r, h)| {
            let s = simplify(r, tolerance.lo);
            if s.len() < 4 { h.clone() } else { s }
        })
        .collect();
    // Les anneaux qui pointent en double (début = fin) sont refermés au dessin.
    let strip = |r: Ring| -> Ring {
        let mut r = r;
        if r.len() > 1 && r.first() == r.last() {
            r.pop();
        }
        r
    };
    Shape {
        rings: hi.into_iter().map(strip).collect(),
        rings_lo: lo.into_iter().map(strip).collect(),
        main,
    }
}

fn area(ring: &[[f32; 2]]) -> f32 {
    let mut a = 0.0;
    for w in ring.windows(2) {
        a += w[0][0] * w[1][1] - w[1][0] * w[0][1];
    }
    let lat = center(ring)[1].to_radians().cos();
    (a / 2.0).abs() * lat
}

fn center(ring: &[[f32; 2]]) -> [f32; 2] {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &[x, y] in ring {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    [(x0 + x1) / 2.0, (y0 + y1) / 2.0]
}

/// Simplification de Douglas-Peucker ; conserve le premier et le dernier point.
fn simplify(ring: &[[f32; 2]], tolerance: f64) -> Ring {
    if ring.len() <= 4 {
        return ring.to_vec();
    }
    let mut keep = vec![false; ring.len()];
    keep[0] = true;
    keep[ring.len() - 1] = true;
    // Un anneau fermé a ses extrémités confondues : on le coupe d'abord au point le plus éloigné.
    let far = (1..ring.len() - 1)
        .max_by(|&a, &b| dist2(ring[a], ring[0]).total_cmp(&dist2(ring[b], ring[0])))
        .unwrap_or(1);
    keep[far] = true;
    let mut stack = vec![(0, far), (far, ring.len() - 1)];
    let tol2 = tolerance * tolerance;
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (mut best, mut best_d) = (a, 0.0);
        for i in a + 1..b {
            let d = segment_dist2(ring[i], ring[a], ring[b]);
            if d > best_d {
                best = i;
                best_d = d;
            }
        }
        if best_d > tol2 {
            keep[best] = true;
            stack.push((a, best));
            stack.push((best, b));
        }
    }
    ring.iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect()
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f64 {
    let dx = f64::from(a[0] - b[0]);
    let dy = f64::from(a[1] - b[1]);
    dx * dx + dy * dy
}

fn segment_dist2(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f64 {
    let (px, py) = (f64::from(p[0]), f64::from(p[1]));
    let (ax, ay) = (f64::from(a[0]), f64::from(a[1]));
    let (bx, by) = (f64::from(b[0]), f64::from(b[1]));
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + t * dx - px, ay + t * dy - py);
    cx * cx + cy * cy
}

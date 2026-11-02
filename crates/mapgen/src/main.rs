//! Génère `assets/world.bin` à partir des contours Natural Earth et des fichiers de `data/`.
//!
//! Usage : `cargo run -p mapgen -- <ne_50m_admin_0_countries.geojson> [sortie]`
//! (voir `scripts/build-map.sh`).

use std::collections::HashMap;
use std::error::Error;

use geo_core::world::{Country, Region, RegionKind, Ring, Shape, Territory, World};
use geo_core::{Matcher, normalize};
use serde::Deserialize;
use serde_json::Value;

/// Tolérances de simplification (en degrés) des deux niveaux de détail.
const TOLERANCE_HI: f64 = 0.01;
const TOLERANCE_LO: f64 = 0.08;
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
    label: Option<[f32; 2]>,
    #[serde(default)]
    bubble: bool,
}

#[derive(Deserialize)]
struct MergeDef {
    from: String,
    into: String,
}

#[derive(Deserialize)]
struct RegionsFile {
    world: ZoneDef,
    continent: Vec<ZoneDef>,
    sub: Vec<ZoneDef>,
    group: Vec<ZoneDef>,
}

#[derive(Deserialize)]
struct ZoneDef {
    #[serde(default)]
    id: String,
    name: String,
    continent: Option<String>,
    view: Option<[f32; 4]>,
    center: Option<f32>,
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    hidden: bool,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let geojson = args.get(1).ok_or("usage : mapgen <geojson> [sortie]")?;
    let output = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("assets/world.bin");

    let countries: CountriesFile =
        toml::from_str(&std::fs::read_to_string("data/countries.toml")?)?;
    let regions: RegionsFile = toml::from_str(&std::fs::read_to_string("data/regions.toml")?)?;
    let features: Value = serde_json::from_str(&std::fs::read_to_string(geojson)?)?;

    let world = build(&countries, &regions, &features)?;
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
    println!(
        "{} pays, {} territoires, {} zones, {} points (détaillé) / {} (simplifié) → {} ({} Ko)",
        world.countries.len(),
        world.territories.len(),
        world.regions.len(),
        points(false),
        points(true),
        output,
        bytes.len() / 1024
    );
    Ok(())
}

fn build(defs: &CountriesFile, zones: &RegionsFile, features: &Value) -> Result<World> {
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

    // Sous-régions : l'ordre des régions est monde, continents, sous-régions, groupes.
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

    let mut rings: Vec<Vec<Ring>> = vec![Vec::new(); defs.countries.len()];
    let mut labels: Vec<Option<[f32; 2]>> = vec![None; defs.countries.len()];
    let mut english: Vec<Option<String>> = vec![None; defs.countries.len()];
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
    let mut territories = Vec::new();

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
                rings[i].extend(polygons);
                if defs.countries[i].code == code {
                    let x = props["LABEL_X"].as_f64().unwrap_or(0.0) as f32;
                    let y = props["LABEL_Y"].as_f64().unwrap_or(0.0) as f32;
                    labels[i] = Some([x, y]);
                    english[i] = props["NAME_EN"].as_str().map(str::to_owned);
                }
            }
            None => {
                let owner = props["SOV_A3"]
                    .as_str()
                    .and_then(|sov| homes.get(sov))
                    .and_then(|home| index.get(home).copied());
                let mut name = props["NAME_FR"].as_str().unwrap_or(code).to_owned();
                if code == "SXM" {
                    // Natural Earth nomme aussi « Saint-Martin » la partie néerlandaise.
                    name = "Sint Maarten".into();
                }
                let name = capitalize(&name);
                let mut answers = vec![normalize(&name)];
                if let Some(en) = props["NAME_EN"].as_str() {
                    push_unique(&mut answers, normalize(en));
                }
                territories.push(Territory {
                    name,
                    answers,
                    owner,
                    label: [
                        props["LABEL_X"].as_f64().unwrap_or(0.0) as f32,
                        props["LABEL_Y"].as_f64().unwrap_or(0.0) as f32,
                    ],
                    shape: make_shape(polygons),
                });
            }
        }
    }

    let mut countries = Vec::with_capacity(defs.countries.len());
    for (i, def) in defs.countries.iter().enumerate() {
        if rings[i].is_empty() {
            return Err(format!("aucun contour pour {} ({})", def.name, def.code).into());
        }
        let mut answers: Vec<String> = Vec::new();
        let mut spoken: Vec<String> = Vec::new();
        for name in std::iter::once(&def.name).chain(&def.aka) {
            push_unique(&mut answers, normalize(name));
            for form in spoken_forms(name) {
                push_unique(&mut spoken, form);
            }
        }
        for form in &def.say {
            push_unique(&mut spoken, form.to_lowercase());
        }
        for form in &spoken {
            push_unique(&mut answers, normalize(form));
        }
        if let Some(en) = &english[i] {
            push_unique(&mut answers, normalize(en));
        }
        countries.push(Country {
            code: def.code.clone(),
            name: def.name.clone(),
            answers,
            spoken,
            sub: sub_index(&def.sub)?,
            label: def.label.or(labels[i]).unwrap_or([0.0, 0.0]),
            bubble: def.bubble,
            shape: make_shape(std::mem::take(&mut rings[i])),
        });
    }
    check_answers(&countries)?;

    let code_index = |code: &str| -> Result<usize> {
        index
            .get(code)
            .copied()
            .ok_or_else(|| format!("pays inconnu : {code}").into())
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
        countries,
        territories,
        regions,
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
            if let Some(other) = seen.insert(a, &c.name)
                && other != c.name
            {
                return Err(format!("réponse « {a} » partagée par {other} et {}", c.name).into());
            }
        }
    }
    // Vérifie que chaque pays est retrouvable par son nom.
    let world = World {
        countries: countries.to_vec(),
        territories: vec![],
        regions: vec![],
    };
    let matcher = Matcher::new(&world);
    for (i, c) in countries.iter().enumerate() {
        assert_eq!(matcher.lookup(&c.name), Some(i), "{}", c.name);
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
        name: def.name.clone(),
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

fn make_shape(rings: Vec<Ring>) -> Shape {
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
    let hi: Vec<Ring> = rings.iter().map(|r| simplify(r, TOLERANCE_HI)).collect();
    let lo: Vec<Ring> = rings
        .iter()
        .zip(&hi)
        .map(|(r, h)| {
            let s = simplify(r, TOLERANCE_LO);
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

//! Drapeaux : SVG (dépôt hampusborgos/country-flags, d'après Wikimedia Commons, domaine public)
//! convertis en calques de polygones colorés, sans courbes ni traits : l'application les peint
//! tels quels, sans moteur SVG.
//!
//! Chaque élément peint (remplissage ou trait, converti en surface) est aplati en polygones,
//! normalisé (`i_overlay`), découpé par ses `clipPath`, puis ramené sur une grille entière et
//! simplifié. Les dégradés prennent leur couleur moyenne ; les masques, motifs et images,
//! absents des drapeaux de pays, sont ignorés.

use std::collections::HashMap;
use std::path::Path;

use geo_core::{Flag, Layer, World};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use resvg::{tiny_skia, usvg};
use serde_json::Value;

use crate::Result;

/// Hauteur d'un drapeau dans l'unité des coordonnées (la largeur suit ses proportions). Assez
/// fine pour les traits les plus minces des armoiries (contours du soleil argentin) : sur une
/// grille de 512, l'arrondi les épaississait ou les effaçait par endroits.
pub const HEIGHT: f64 = 2048.0;
/// Écart maximal toléré en simplifiant un contour, en unités (≈ 0,2 px pour un drapeau de 240 px
/// de haut, 0,5 px agrandi à 480 px) : les rayons ondulés restent ondulés.
const TOLERANCE: f64 = 2.0;
/// Longueur maximale d'un segment de courbe aplatie, en unités.
const CURVE_STEP: f64 = 4.0;
/// Les contours plus petits (en unités²) disparaissent.
const MIN_AREA: f64 = 9.6;

type Contours = Vec<Vec<[f64; 2]>>;
/// Calque d'une couleur, en coordonnées entières.
type GridLayer = ([u8; 4], Vec<Vec<[i32; 2]>>);

/// Codes ISO 3166-1 alpha-2 des pays dont Natural Earth n'en donne pas.
const ISO_OVERRIDES: &[(&str, &str)] = &[("KOS", "XK")];

/// Drapeaux des pays de l'atlas du monde, dans son ordre.
pub fn build(admin0: &Value, dir: &Path, world: &World) -> Result<Vec<Flag>> {
    let mut iso: HashMap<String, String> = HashMap::new();
    for feature in admin0["features"].as_array().ok_or("GeoJSON invalide")? {
        let props = &feature["properties"];
        let code = props["ADM0_A3"].as_str().unwrap_or("");
        let a2 = [props["ISO_A2_EH"].as_str(), props["ISO_A2"].as_str()]
            .into_iter()
            .flatten()
            .find(|c| c.len() == 2 && *c != "-9");
        if let Some(a2) = a2 {
            iso.insert(code.to_owned(), a2.to_lowercase());
        }
    }
    for (code, a2) in ISO_OVERRIDES {
        iso.insert((*code).to_owned(), a2.to_lowercase());
    }
    let mut flags = Vec::with_capacity(world.countries.len());
    for country in &world.countries {
        let a2 = iso
            .get(&country.code)
            .ok_or_else(|| format!("{} : pas de code ISO à deux lettres", country.code))?;
        let file = dir.join(format!("{a2}.svg"));
        let svg = std::fs::read(&file).map_err(|e| format!("{} : {e}", file.display()))?;
        flags.push(convert(&country.code, &svg)?);
    }
    Ok(flags)
}

/// Un drapeau SVG en calques de polygones.
pub fn convert(code: &str, svg: &[u8]) -> Result<Flag> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default())?;
    let size = tree.size();
    let scale = HEIGHT / f64::from(size.height());
    let width = (f64::from(size.width()) * scale).round();
    let base = tiny_skia::Transform::from_scale(scale as f32, scale as f32);
    let mut painted: Vec<([u8; 4], Contours)> = Vec::new();
    walk(tree.root(), base, None, 1.0, code, &mut painted);

    // Les éléments consécutifs de même couleur forment un seul calque : leur union, car ils se
    // chevauchent souvent (étoiles faites de triangles), ce que la règle pair-impair annulerait.
    let mut merged: Vec<([u8; 4], Contours)> = Vec::new();
    for (color, contours) in painted {
        match merged.last_mut() {
            Some((last, list)) if *last == color => list.extend(contours),
            _ => merged.push((color, contours)),
        }
    }
    let frame = vec![vec![
        [0.0, 0.0],
        [width, 0.0],
        [width, HEIGHT],
        [0.0, HEIGHT],
    ]];
    let mut layers: Vec<GridLayer> = Vec::new();
    for (color, contours) in merged {
        // Union des surfaces (normalisées, donc orientées de façon cohérente), sans rien qui
        // dépasse du drapeau.
        let inside =
            flatten_shapes(contours.overlay(&frame, OverlayRule::Intersect, FillRule::NonZero));
        let grid: Vec<Vec<[i32; 2]>> = inside
            .iter()
            .map(|c| quantize(&simplify(c, TOLERANCE)))
            .filter(|c| c.len() >= 3 && area(c).abs() >= MIN_AREA)
            .collect();
        if !grid.is_empty() {
            layers.push((color, grid));
        }
    }
    Ok(Flag {
        code: code.to_owned(),
        width: width as u16,
        height: HEIGHT as u16,
        layers: layers
            .iter()
            .map(|(color, contours)| Layer::new(*color, contours))
            .collect(),
    })
}

/// Parcourt un groupe : chaque élément peint ajoute sa surface (découpée par `clip`) et sa
/// couleur à `out`, dans l'ordre de peinture.
fn walk(
    group: &usvg::Group,
    transform: tiny_skia::Transform,
    clip: Option<&Contours>,
    opacity: f32,
    code: &str,
    out: &mut Vec<([u8; 4], Contours)>,
) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => {
                let t = transform.pre_concat(g.transform());
                if g.mask().is_some() {
                    eprintln!("{code} : masque ignoré");
                }
                let own = g.clip_path().map(|c| clip_area(c, t));
                let combined = match (clip, own) {
                    (Some(a), Some(b)) => Some(intersect(a, &b)),
                    (a, b) => b.or_else(|| a.cloned()),
                };
                walk(
                    g,
                    t,
                    combined.as_ref(),
                    opacity * g.opacity().get(),
                    code,
                    out,
                );
            }
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let fill = p.fill().and_then(|f| {
                    let color = paint_color(f.paint(), f.opacity().get() * opacity, code)?;
                    let rule = match f.rule() {
                        usvg::FillRule::NonZero => FillRule::NonZero,
                        usvg::FillRule::EvenOdd => FillRule::EvenOdd,
                    };
                    Some((color, area_of(p.data(), transform, rule)))
                });
                let stroke = p.stroke().and_then(|s| {
                    let color = paint_color(s.paint(), s.opacity().get() * opacity, code)?;
                    Some((color, stroke_area(p.data(), s, transform)?))
                });
                let parts = match p.paint_order() {
                    usvg::PaintOrder::FillAndStroke => [fill, stroke],
                    usvg::PaintOrder::StrokeAndFill => [stroke, fill],
                };
                for (color, contours) in parts.into_iter().flatten() {
                    let contours = match clip {
                        Some(c) => intersect(&contours, c),
                        None => contours,
                    };
                    if !contours.is_empty() {
                        out.push((color, contours));
                    }
                }
            }
            usvg::Node::Text(t) => walk(t.flattened(), transform, clip, opacity, code, out),
            usvg::Node::Image(_) => eprintln!("{code} : image ignorée"),
        }
    }
}

/// Surface d'un `clipPath`, dans le repère de l'élément qui l'utilise (`transform`).
fn clip_area(clip: &usvg::ClipPath, transform: tiny_skia::Transform) -> Contours {
    let t = transform.pre_concat(clip.transform());
    let mut all: Contours = Vec::new();
    collect_clip(clip.root(), t, &mut all);
    let mut area = flatten_shapes(all.simplify_shape(FillRule::NonZero));
    if let Some(inner) = clip.clip_path() {
        area = intersect(&area, &clip_area(inner, transform));
    }
    area
}

fn collect_clip(group: &usvg::Group, transform: tiny_skia::Transform, out: &mut Contours) {
    for node in group.children() {
        match node {
            usvg::Node::Path(p) if p.is_visible() => {
                let rule = match p.fill().map(|f| f.rule()) {
                    Some(usvg::FillRule::EvenOdd) => FillRule::EvenOdd,
                    _ => FillRule::NonZero,
                };
                out.extend(area_of(p.data(), transform, rule));
            }
            usvg::Node::Group(g) => {
                let t = transform.pre_concat(g.transform());
                let mut inner = Vec::new();
                collect_clip(g, t, &mut inner);
                if let Some(c) = g.clip_path() {
                    inner = intersect(&inner, &clip_area(c, t));
                }
                out.extend(inner);
            }
            usvg::Node::Text(t) => collect_clip(t.flattened(), transform, out),
            _ => {}
        }
    }
}

/// Couleur d'une peinture : un dégradé prend la moyenne de ses arrêts.
fn paint_color(paint: &usvg::Paint, opacity: f32, code: &str) -> Option<[u8; 4]> {
    let (rgb, alpha) = match paint {
        usvg::Paint::Color(c) => ([c.red, c.green, c.blue], 1.0),
        usvg::Paint::LinearGradient(g) => average(g.stops()),
        usvg::Paint::RadialGradient(g) => average(g.stops()),
        usvg::Paint::Pattern(_) => {
            eprintln!("{code} : motif ignoré");
            return None;
        }
    };
    let a = (alpha * opacity * 255.0).round().clamp(0.0, 255.0) as u8;
    (a > 0).then_some([rgb[0], rgb[1], rgb[2], a])
}

fn average(stops: &[usvg::Stop]) -> ([u8; 3], f32) {
    if stops.is_empty() {
        return ([0, 0, 0], 0.0);
    }
    let n = stops.len() as f32;
    let mut sum = [0.0f32; 4];
    for s in stops {
        let c = s.color();
        sum[0] += f32::from(c.red);
        sum[1] += f32::from(c.green);
        sum[2] += f32::from(c.blue);
        sum[3] += s.opacity().get();
    }
    (
        [
            (sum[0] / n).round() as u8,
            (sum[1] / n).round() as u8,
            (sum[2] / n).round() as u8,
        ],
        sum[3] / n,
    )
}

/// Surface remplie d'un tracé, normalisée (contours extérieurs et trous sans chevauchement).
fn area_of(path: &tiny_skia::Path, transform: tiny_skia::Transform, rule: FillRule) -> Contours {
    let Some(path) = path.clone().transform(transform) else {
        return Vec::new();
    };
    flatten_shapes(flatten(&path).simplify_shape(rule))
}

/// Surface couverte par le trait d'un tracé.
fn stroke_area(
    path: &tiny_skia::Path,
    stroke: &usvg::Stroke,
    transform: tiny_skia::Transform,
) -> Option<Contours> {
    let mut s = stroke.to_tiny_skia();
    let res = transform.sx.hypot(transform.ky).max(1.0);
    let dashed;
    let path = match s.dash.take() {
        Some(dash) => {
            dashed = path.dash(&dash, res)?;
            &dashed
        }
        None => path,
    };
    let outline = path.stroke(&s, res)?;
    Some(area_of(&outline, transform, FillRule::NonZero))
}

/// Contours d'un tracé, les courbes aplaties en segments.
fn flatten(path: &tiny_skia::Path) -> Contours {
    let mut contours: Contours = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::new();
    let mut last = [0.0, 0.0];
    let point = |p: tiny_skia::Point| [f64::from(p.x), f64::from(p.y)];
    let finish = |current: &mut Vec<[f64; 2]>, contours: &mut Contours| {
        if current.len() >= 3 {
            contours.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };
    for segment in path.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(p) => {
                finish(&mut current, &mut contours);
                last = point(p);
                current.push(last);
            }
            tiny_skia::PathSegment::LineTo(p) => {
                last = point(p);
                current.push(last);
            }
            tiny_skia::PathSegment::QuadTo(c, p) => {
                let (c, p) = (point(c), point(p));
                let steps = steps(&[last, c, p]);
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let u = 1.0 - t;
                    current.push([
                        u * u * last[0] + 2.0 * u * t * c[0] + t * t * p[0],
                        u * u * last[1] + 2.0 * u * t * c[1] + t * t * p[1],
                    ]);
                }
                last = p;
            }
            tiny_skia::PathSegment::CubicTo(c1, c2, p) => {
                let (c1, c2, p) = (point(c1), point(c2), point(p));
                let steps = steps(&[last, c1, c2, p]);
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    current.push([
                        a * last[0] + b * c1[0] + c * c2[0] + d * p[0],
                        a * last[1] + b * c1[1] + c * c2[1] + d * p[1],
                    ]);
                }
                last = p;
            }
            tiny_skia::PathSegment::Close => finish(&mut current, &mut contours),
        }
    }
    finish(&mut current, &mut contours);
    contours
}

/// Nombre de segments pour aplatir une courbe, d'après la longueur de son polygone de contrôle.
fn steps(points: &[[f64; 2]]) -> usize {
    let length: f64 = points
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum();
    ((length / CURVE_STEP).ceil() as usize).clamp(1, 64)
}

fn intersect(a: &Contours, b: &Contours) -> Contours {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    flatten_shapes(a.overlay(b, OverlayRule::Intersect, FillRule::NonZero))
}

/// Formes (contour extérieur et trous) mises à plat : la règle pair-impair suffit à les peindre.
fn flatten_shapes(shapes: Vec<Vec<Vec<[f64; 2]>>>) -> Contours {
    shapes.into_iter().flatten().collect()
}

fn quantize(contour: &[[f64; 2]]) -> Vec<[i32; 2]> {
    let mut out: Vec<[i32; 2]> = Vec::with_capacity(contour.len());
    for &[x, y] in contour {
        let p = [x.round() as i32, y.round() as i32];
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

fn area(contour: &[[i32; 2]]) -> f64 {
    let n = contour.len();
    (0..n)
        .map(|i| {
            let (a, b) = (contour[i], contour[(i + 1) % n]);
            f64::from(a[0]) * f64::from(b[1]) - f64::from(b[0]) * f64::from(a[1])
        })
        .sum::<f64>()
        / 2.0
}

/// Douglas-Peucker sur un contour fermé, coupé en deux à son point le plus éloigné du premier.
fn simplify(contour: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    let n = contour.len();
    if n <= 4 {
        return contour.to_vec();
    }
    let d2 = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    let far = (1..n)
        .max_by(|&a, &b| d2(contour[a], contour[0]).total_cmp(&d2(contour[b], contour[0])))
        .unwrap_or(1);
    let mut keep = vec![false; n + 1];
    keep[0] = true;
    keep[far] = true;
    keep[n] = true;
    let at = |i: usize| contour[i % n];
    let mut stack = vec![(0, far), (far, n)];
    let tol2 = tolerance * tolerance;
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (at(a), at(b));
        let (mut best, mut best_d) = (a, 0.0);
        for i in a + 1..b {
            let d = segment_dist2(at(i), pa, pb);
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
    (0..n).filter(|&i| keep[i]).map(|i| contour[i]).collect()
}

fn segment_dist2(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    };
    (p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)
}

/// Planche de contrôle : chaque drapeau rendu à partir de ses polygones (à droite) à côté de
/// l'original rendu par resvg (à gauche).
#[allow(non_snake_case)]
pub fn sheet(
    flags: &[Flag],
    dir: &Path,
    iso: &dyn Fn(&str) -> Option<String>,
    out: &Path,
) -> Result<()> {
    // Taille d'une vignette et sélection, réglables pour regarder de près (`SHEET_W=480`,
    // `SHEET_ONLY=ESP,MEX`).
    let w: u32 = std::env::var("SHEET_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(150);
    let only = std::env::var("SHEET_ONLY").unwrap_or_default();
    let flags: Vec<&Flag> = flags
        .iter()
        .filter(|f| only.is_empty() || only.split(',').any(|c| c == f.code))
        .collect();
    let (W, H) = (w, w * 2 / 3);
    let COLS: u32 = (900 / w).max(1);
    let rows = (flags.len() as u32).div_ceil(COLS);
    let mut sheet =
        tiny_skia::Pixmap::new(COLS * (2 * W + 30), rows * (H + 12)).ok_or("planche")?;
    sheet.fill(tiny_skia::Color::from_rgba8(60, 60, 70, 255));
    for (i, flag) in flags.iter().enumerate() {
        let (col, row) = (i as u32 % COLS, i as u32 / COLS);
        let x0 = (col * (2 * W + 30)) as f32;
        let y0 = (row * (H + 12)) as f32;
        let fit = (W as f32 / f32::from(flag.width)).min(H as f32 / f32::from(flag.height));
        // Original.
        if let Some(a2) = iso(&flag.code) {
            let data = std::fs::read(dir.join(format!("{a2}.svg")))?;
            let tree = usvg::Tree::from_data(&data, &usvg::Options::default())?;
            let s = (W as f32 / tree.size().width()).min(H as f32 / tree.size().height());
            resvg::render(
                &tree,
                tiny_skia::Transform::from_row(s, 0.0, 0.0, s, x0, y0),
                &mut sheet.as_mut(),
            );
        }
        // Polygones.
        let t = tiny_skia::Transform::from_row(fit, 0.0, 0.0, fit, x0 + W as f32 + 10.0, y0);
        for layer in &flag.layers {
            let mut pb = tiny_skia::PathBuilder::new();
            for contour in layer.points() {
                for (k, [x, y]) in contour.iter().enumerate() {
                    if k == 0 {
                        pb.move_to(*x, *y);
                    } else {
                        pb.line_to(*x, *y);
                    }
                }
                pb.close();
            }
            let Some(path) = pb.finish() else { continue };
            let mut paint = tiny_skia::Paint::default();
            let [r, g, b, a] = layer.color;
            paint.set_color_rgba8(r, g, b, a);
            paint.anti_alias = true;
            sheet.fill_path(&path, &paint, tiny_skia::FillRule::EvenOdd, t, None);
        }
    }
    sheet.save_png(out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Point dans un calque (règle pair-impair) ?
    fn inside(layer: &Layer, [x, y]: [f32; 2]) -> bool {
        let mut c = false;
        for ring in layer.points() {
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + ring.len() - 1) % ring.len()]);
                if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0]
                {
                    c = !c;
                }
            }
        }
        c
    }

    /// Étoile du Burkina Faso : dix triangles, dont cinq retournés (`scale(-1 1)`).
    #[test]
    fn star_made_of_triangles_is_filled() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 900 600"><path fill="#009e49" d="M0 0h900v600H0z"/><path fill="#ef2b2d" d="M0 0h900v300H0z"/><g transform="translate(450 300)" fill="#fcd116"><g id="b"><path id="a" d="M0-100V0h50" transform="rotate(18 0 -100)"/><use xlink:href="#a" transform="scale(-1 1)"/></g><use xlink:href="#b" transform="rotate(72)"/><use xlink:href="#b" transform="rotate(144)"/><use xlink:href="#b" transform="rotate(216)"/><use xlink:href="#b" transform="rotate(288)"/></g></svg>"##;
        let flag = convert("BFA", svg).unwrap();
        let star = flag.layers.last().unwrap();
        assert_eq!(star.color, [0xfc, 0xd1, 0x16, 255]);
        // Coordonnées du SVG (900 × 600) ramenées à la grille.
        let u = (HEIGHT / 600.0) as f32;
        assert!(inside(star, [450.0 * u, 300.0 * u]), "centre de l'étoile vide");
        assert!(inside(star, [450.0 * u, 218.0 * u]), "branche du haut vide");
    }

    /// Union Jack : les diagonales rouges sont découpées par un `clipPath`.
    #[test]
    fn clip_paths_cut_shapes() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 30"><clipPath id="a"><path d="M0 0v30h60V0z"/></clipPath><clipPath id="b"><path d="M30 15h30v15zv15H0zH0V0zV0h30z"/></clipPath><g clip-path="url(#a)"><path d="M0 0v30h60V0z" fill="#012169"/><path d="M0 0l60 30m0-30L0 30" stroke="#fff" stroke-width="6"/><path d="M0 0l60 30m0-30L0 30" clip-path="url(#b)" stroke="#C8102E" stroke-width="4"/><path d="M30 0v30M0 15h60" stroke="#fff" stroke-width="10"/><path d="M30 0v30M0 15h60" stroke="#C8102E" stroke-width="6"/></g></svg>"##;
        let flag = convert("GBR", svg).unwrap();
        assert_eq!(f64::from(flag.width), 2.0 * HEIGHT);
        let red = flag
            .layers
            .iter()
            .find(|l| l.color == [0xc8, 0x10, 0x2e, 255]);
        let red = red.unwrap();
        // Diagonale rouge du quart supérieur gauche : sous la diagonale blanche (côté bas)…
        let u = f32::from(flag.width) / 60.0;
        assert!(inside(red, [10.0 * u, 5.8 * u]));
        // … mais pas au-dessus (le contre-changement de la croix de Saint-Patrick).
        assert!(!inside(red, [10.0 * u, 4.2 * u]));
    }
}

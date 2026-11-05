//! Carte interactive : projection des contours, caméra (zoom / déplacement), rendu et clics.

use geo_core::projection::{project, project_ring, project_unwrapped, project_view};
use geo_core::{Lang, Mark, Quiz, Shape, World};
use iced::alignment::Vertical;
use iced::mouse::{self, ScrollDelta};
use iced::widget::canvas::fill::{Fill, Rule};
use iced::widget::canvas::path::Builder;
use iced::widget::canvas::{
    self, Action, Cache, Event, Frame, Geometry, LineCap, LineJoin, Path, Stroke, Text,
};
use iced::widget::text::Alignment;
use iced::{Color, Pixels, Point, Rectangle, Renderer, Size, Theme, Vector, color};

use crate::progress::Level;
use crate::settings::MapLabels;
use crate::style;

/// En dessous de cette taille à l'écran (en pixels), un pays est signalé par une pastille.
const TINY_PX: f32 = 7.0;
const MARKER_RADIUS: f32 = 4.5;
/// Au-delà de cette échelle (pixels par unité projetée), on dessine les contours détaillés.
const DETAIL_SCALE: f32 = 650.0;
/// Marge (en pixels) autour des îles d'un archipel pour former sa bulle, réduite (sans
/// descendre sous `BUBBLE_MIN_PX`) quand une autre terre est proche.
const BUBBLE_PX: f32 = 5.0;
const BUBBLE_MIN_PX: f32 = 2.0;
/// Écart maximal (coordonnées projetées, environ 7°) entre deux groupes d'îles d'une même bulle.
const BUBBLE_GAP: f32 = 0.12;
/// Distance (en pixels) jusqu'à laquelle un clic ou le survol désigne un fleuve.
const LINE_HIT_PX: f32 = 8.0;
pub const MIN_ZOOM: f32 = 0.6;
pub const MAX_ZOOM: f32 = 600.0;

/// Contours projetés pour un méridien central donné.
pub struct Projection {
    /// Méridien central.
    center: f32,
    countries: Vec<ProjShape>,
    territories: Vec<ProjShape>,
    /// Grands lacs, peints sur les terres (atlas des fleuves).
    lakes: Vec<ProjShape>,
    /// Bulles des archipels : une ellipse par groupe d'îles (vide pour les autres pays).
    bubbles: Vec<Vec<Bubble>>,
}

struct ProjShape {
    hi: Vec<Vec<Point>>,
    lo: Vec<Vec<Point>>,
    /// Pour chaque anneau (même ordre dans `hi` et `lo`) : copie redessinée de l'autre côté
    /// de la carte, quand le pays déborde d'un bord (îles de la Ligne de Kiribati, Aléoutiennes).
    wrapped: Vec<bool>,
    /// Boîte englobante de tous les anneaux (coordonnées projetées, y vers le haut).
    bbox: Rectangle,
    /// Boîte englobante de la partie principale (l'anneau du point d'étiquette) : mesure la
    /// taille « visible » du pays et sert à le cadrer.
    core: Rectangle,
    label: Point,
}

impl Projection {
    pub fn new(world: &World, center: f32) -> Self {
        let mut countries: Vec<ProjShape> = world
            .countries
            .iter()
            .map(|c| ProjShape::new(&c.shape, c.label, center, world.info.water))
            .collect();
        // Un fleuve se mesure et se cadre sur tout son cours, pas sur un tronçon.
        if world.info.lines {
            for shape in &mut countries {
                shape.core = shape.bbox;
            }
        }
        let lakes = world
            .lakes
            .iter()
            .map(|shape| {
                let anchor = shape.rings.first().and_then(|r| r.first()).copied();
                ProjShape::new(shape, anchor.unwrap_or_default(), center, false)
            })
            .collect();
        let territories: Vec<ProjShape> = world
            .territories
            .iter()
            .map(|t| ProjShape::new(&t.shape, t.label, center, world.info.water))
            .collect();
        let bubbles = world
            .countries
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if !c.bubble {
                    return Vec::new();
                }
                // Toutes les autres terres, pour que la bulle ne les touche pas.
                let others: Vec<&ProjShape> = countries
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .map(|(_, s)| s)
                    .chain(&territories)
                    .collect();
                bubbles(&countries[i], c.one_bubble, &others)
            })
            .collect();
        Self {
            center,
            countries,
            territories,
            lakes,
            bubbles,
        }
    }

    pub fn core(&self, country: usize) -> Rectangle {
        self.countries[country].core
    }

    pub fn label(&self, country: usize) -> Point {
        self.countries[country].label
    }
}

impl ProjShape {
    /// `fixed` (atlas d'eau) : chaque anneau est placé d'après sa propre longitude, pas celle du
    /// point d'étiquette, et sans copie de l'autre côté de la carte (les deux moitiés du
    /// Pacifique restent chacune à leur bord). Un anneau qui fait le tour du globe (Antarctique,
    /// Arctique, Austral) n'est pas replié à ±180° : il ne s'affiche bien qu'avec une carte
    /// centrée sur le méridien de Greenwich.
    fn new(shape: &Shape, [lon, lat]: [f32; 2], center: f32, fixed: bool) -> Self {
        let mut buf = Vec::new();
        let mut project_one = |ring: &[[f32; 2]], shift: f32| {
            if fixed {
                let (min, max) = ring
                    .iter()
                    .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
                if max - min >= 300.0 {
                    let points = around_pole(ring, center)
                        .into_iter()
                        .map(|[l, y]| {
                            let [px, py] = project_unwrapped(l + center, y, center);
                            Point::new(px, py)
                        })
                        .collect();
                    return (points, [-180.0, 180.0]);
                }
                // Longitude moyenne : un point à ±180° ne dit pas de quel côté est l'anneau.
                let anchor = ring.iter().map(|p| p[0]).sum::<f32>() / ring.len().max(1) as f32;
                project_ring(ring, center, anchor, shift, &mut buf);
                let points: Vec<Point> = buf.iter().map(|&[x, y]| Point::new(x, y)).collect();
                return (points, [-180.0, 180.0]);
            }
            let range = project_ring(ring, center, lon, shift, &mut buf);
            let points: Vec<Point> = buf.iter().map(|&[x, y]| Point::new(x, y)).collect();
            (points, range)
        };
        // Décalage (±360°) qui ramène un anneau dépassant un bord de la carte de l'autre côté ;
        // seuls ces anneaux-là sont dupliqués (îles de la Ligne de Kiribati, Aléoutiennes).
        let overflow = |[min, max]: [f32; 2]| {
            if max > 180.0 {
                Some(-360.0)
            } else if min < -180.0 {
                Some(360.0)
            } else {
                None
            }
        };
        let (mut hi, mut lo, mut wrapped) = (Vec::new(), Vec::new(), Vec::new());
        let mut copies = Vec::new();
        for (ring, ring_lo) in shape.rings.iter().zip(&shape.rings_lo) {
            let (points, range) = project_one(ring, 0.0);
            hi.push(points);
            lo.push(project_one(ring_lo, 0.0).0);
            wrapped.push(false);
            if let Some(shift) = overflow(range) {
                copies.push((ring, ring_lo, shift));
            }
        }
        for (ring, ring_lo, shift) in copies {
            hi.push(project_one(ring, shift).0);
            lo.push(project_one(ring_lo, shift).0);
            wrapped.push(true);
        }
        let bbox = bounds(hi.iter().flatten());
        let [x, y] = project(lon, lat, center);
        let label = Point::new(x, y);
        // Partie principale : l'anneau qui contient le point d'étiquette, sinon le plus proche
        // relativement à sa taille (la Nouvelle-Zélande a son point entre ses deux îles, Kiribati
        // sur Tarawa, bien plus petite que l'île Christmas).
        let core = hi
            .iter()
            .zip(&wrapped)
            .filter(|(_, copy)| !**copy)
            .map(|(r, _)| bounds(r.iter()))
            .min_by(|a, b| {
                let score = |r: &Rectangle| {
                    if r.contains(label) {
                        0.0
                    } else {
                        r.center().distance(label) / r.area().sqrt().max(1e-9)
                    }
                };
                score(a).total_cmp(&score(b))
            })
            .unwrap_or(bbox);
        Self {
            hi,
            lo,
            wrapped,
            bbox,
            core,
            label,
        }
    }
}

/// Anneau qui fait le tour d'un pôle (Antarctique, océans Arctique et Austral), recoupé au bord
/// de la carte centrée sur `center` : longitudes relatives au méridien central, ramenées dans
/// [-180, 180], et, à chaque passage d'un bord à l'autre, un détour par le pôle le long des deux
/// bords. Les anciens bords (désormais à l'intérieur de la carte) sont parcourus dans les deux
/// sens et ne délimitent plus rien.
fn around_pole(ring: &[[f32; 2]], center: f32) -> Vec<[f32; 2]> {
    let mean = ring.iter().map(|p| p[1]).sum::<f32>() / ring.len().max(1) as f32;
    let pole = if mean < 0.0 { -90.0 } else { 90.0 };
    let wrap = |lon: f32| (lon - center + 180.0).rem_euclid(360.0) - 180.0;
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(ring.len() + 8);
    for (i, &[lon, lat]) in ring.iter().enumerate() {
        let l = wrap(lon);
        if i > 0 {
            let [prev_lon, prev_lat] = ring[i - 1];
            let p = wrap(prev_lon);
            // Bord franchi : d'un côté à l'autre, alors que le pas réel est petit.
            if (l - p).abs() > 180.0 {
                let (from, to) = if p > l {
                    (180.0, -180.0)
                } else {
                    (-180.0, 180.0)
                };
                // Latitude au bord, par interpolation sur le pas réel.
                let step = (lon - prev_lon + 540.0).rem_euclid(360.0) - 180.0;
                let to_edge = (from - p).abs();
                let t = if step.abs() > 0.0 {
                    (to_edge / step.abs()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cross = prev_lat + (lat - prev_lat) * t;
                out.extend([[from, cross], [from, pole], [to, pole], [to, cross]]);
            }
        }
        out.push([l, lat]);
    }
    out
}

/// Bulle d'un archipel : enveloppe convexe d'un groupe d'îles, lissée et élargie d'une petite
/// marge au dessin. Coordonnées projetées.
#[derive(Debug, Clone)]
struct Bubble {
    hull: Vec<Point>,
    /// Distance à la terre la plus proche d'un autre pays, qui borne la marge.
    clearance: f32,
}

impl Bubble {
    /// Marge en pixels : jamais plus de la moitié de l'écart avec la terre voisine.
    fn margin(&self, t: &Transform) -> f32 {
        (self.clearance * t.scale * 0.45).clamp(BUBBLE_MIN_PX, BUBBLE_PX)
    }

    /// Contour lissé (coins arrondis par l'algorithme de Chaikin), en pixels.
    fn outline(&self, t: &Transform) -> Vec<Point> {
        let mut points: Vec<Point> = self.hull.iter().map(|&p| t.to_screen(p)).collect();
        if points.len() < 3 {
            return points;
        }
        for _ in 0..3 {
            let n = points.len();
            points = (0..n)
                .flat_map(|i| {
                    let (a, b) = (points[i], points[(i + 1) % n]);
                    [
                        Point::new(a.x * 0.75 + b.x * 0.25, a.y * 0.75 + b.y * 0.25),
                        Point::new(a.x * 0.25 + b.x * 0.75, a.y * 0.25 + b.y * 0.75),
                    ]
                })
                .collect();
        }
        points
    }

    /// Le point (en pixels) est-il dans la bulle ?
    fn contains(&self, t: &Transform, p: Point) -> bool {
        let outline = self.outline(t);
        let margin = self.margin(t);
        let n = outline.len();
        (n >= 3 && contains(std::slice::from_ref(&outline), p))
            || (0..n).any(|i| segment_distance(p, outline[i], outline[(i + 1) % n]) <= margin)
    }
}

/// Bulles d'un archipel : une par groupe d'îles proches, ou une seule avec `one`. Une île et
/// sa copie de l'autre côté de la carte ne sont jamais mêlées.
fn bubbles(shape: &ProjShape, one: bool, others: &[&ProjShape]) -> Vec<Bubble> {
    let mut groups: Vec<Vec<&Vec<Point>>> = Vec::new();
    for copy in [false, true] {
        let rings: Vec<&Vec<Point>> = shape
            .lo
            .iter()
            .zip(&shape.wrapped)
            .filter(|(_, w)| **w == copy)
            .map(|(r, _)| r)
            .collect();
        if rings.is_empty() {
            continue;
        }
        if one {
            groups.push(rings);
        } else {
            groups.extend(nearby_groups(rings));
        }
    }
    groups
        .iter()
        .map(|group| {
            let points: Vec<Point> = group.iter().flat_map(|r| r.iter().copied()).collect();
            Bubble {
                clearance: clearance(&points, others),
                hull: convex_hull(points),
            }
        })
        .collect()
}

/// Distance (coordonnées projetées) entre des îles et la terre d'autres pays, plafonnée.
fn clearance(points: &[Point], others: &[&ProjShape]) -> f32 {
    const CAP: f32 = 0.1;
    let area = bounds(points.iter());
    let near = Rectangle::new(
        Point::new(area.x - CAP, area.y - CAP),
        Size::new(area.width + 2.0 * CAP, area.height + 2.0 * CAP),
    );
    let mut best = CAP;
    for other in others.iter().filter(|o| o.bbox.intersects(&near)) {
        for ring in other
            .lo
            .iter()
            .filter(|r| bounds(r.iter()).intersects(&near))
        {
            for q in ring {
                for p in points {
                    best = best.min(p.distance(*q));
                }
            }
        }
    }
    best
}

/// Enveloppe convexe (chaîne monotone d'Andrew).
fn convex_hull(mut points: Vec<Point>) -> Vec<Point> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut hull: Vec<Point> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Vec<Point> = if pass == 0 {
            points.clone()
        } else {
            points.iter().rev().copied().collect()
        };
        for p in ordered {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Distance d'un point au segment [a, b].
fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

/// Regroupe les anneaux dont les boîtes sont à moins de `BUBBLE_GAP` l'une de l'autre.
fn nearby_groups(rings: Vec<&Vec<Point>>) -> Vec<Vec<&Vec<Point>>> {
    let boxes: Vec<Rectangle> = rings.iter().map(|r| bounds(r.iter())).collect();
    let gap = |a: &Rectangle, b: &Rectangle| {
        let dx = (a.x - (b.x + b.width)).max(b.x - (a.x + a.width)).max(0.0);
        let dy = (a.y - (b.y + b.height))
            .max(b.y - (a.y + a.height))
            .max(0.0);
        dx.hypot(dy)
    };
    let mut group: Vec<usize> = (0..rings.len()).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    for i in 0..rings.len() {
        for j in i + 1..rings.len() {
            if gap(&boxes[i], &boxes[j]) < BUBBLE_GAP {
                let (a, b) = (root(&mut group, i), root(&mut group, j));
                group[a] = b;
            }
        }
    }
    let mut groups: Vec<(usize, Vec<&Vec<Point>>)> = Vec::new();
    for (i, ring) in rings.into_iter().enumerate() {
        let r = root(&mut group, i);
        match groups.iter_mut().find(|(k, _)| *k == r) {
            Some((_, members)) => members.push(ring),
            None => groups.push((r, vec![ring])),
        }
    }
    groups.into_iter().map(|(_, members)| members).collect()
}

fn bounds<'a>(points: impl Iterator<Item = &'a Point>) -> Rectangle {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    if x0 > x1 {
        return Rectangle::default();
    }
    Rectangle::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0))
}

/// Cadrage d'une zone, en coordonnées projetées.
pub fn view_rect(view: [f32; 4], center: f32) -> Rectangle {
    let [x0, y0, x1, y1] = project_view(view, center);
    Rectangle::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0))
}

/// Position de la caméra : centre (coordonnées projetées) et zoom relatif au cadrage de la zone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub center: Point,
    pub zoom: f32,
}

impl Camera {
    pub fn fit(view: Rectangle) -> Self {
        Self {
            center: view.center(),
            zoom: 1.0,
        }
    }

    /// Caméra qui montre `area` en entier, `view` étant le cadrage de référence (zoom 1).
    pub fn frame(area: Rectangle, view: Rectangle, size: Size) -> Self {
        Camera {
            center: area.center(),
            zoom: fit_scale(area, size) / fit_scale(view, size),
        }
        .clamped()
    }

    pub fn lerp(self, to: Camera, t: f32) -> Camera {
        Camera {
            center: Point::new(
                self.center.x + (to.center.x - self.center.x) * t,
                self.center.y + (to.center.y - self.center.y) * t,
            ),
            zoom: (self.zoom.ln() + (to.zoom.ln() - self.zoom.ln()) * t).exp(),
        }
    }

    /// Zoom d'un facteur donné en gardant fixe le point de l'écran `anchor`.
    pub fn zoom_at(
        self,
        view: Rectangle,
        size: Size,
        factor: f32,
        anchor: Option<Point>,
    ) -> Camera {
        let t = Transform::new(self, view, size);
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let anchor = anchor.unwrap_or(Point::new(size.width / 2.0, size.height / 2.0));
        let p = t.to_world(anchor);
        let ratio = self.zoom / zoom;
        Camera {
            center: Point::new(
                p.x - (p.x - self.center.x) * ratio,
                p.y - (p.y - self.center.y) * ratio,
            ),
            zoom,
        }
        .clamped()
    }

    fn clamped(self) -> Camera {
        Camera {
            center: Point::new(
                self.center.x.clamp(-3.2, 3.2),
                self.center.y.clamp(-1.6, 1.6),
            ),
            zoom: self.zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        }
    }
}

/// Échelle (pixels par unité projetée) qui fait tenir `area` dans `size`, avec une marge.
fn fit_scale(area: Rectangle, size: Size) -> f32 {
    (size.width / area.width.max(1e-6)).min(size.height / area.height.max(1e-6)) * 0.94
}

/// Marges en pixels.
#[derive(Debug, Clone, Copy)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Passage des coordonnées projetées aux pixels du canvas.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    center: Point,
    pub scale: f32,
    half: Vector,
}

impl Transform {
    pub fn new(camera: Camera, view: Rectangle, size: Size) -> Self {
        Self {
            center: camera.center,
            scale: fit_scale(view, size) * camera.zoom,
            half: Vector::new(size.width / 2.0, size.height / 2.0),
        }
    }

    pub fn to_screen(self, p: Point) -> Point {
        Point::new(
            (p.x - self.center.x) * self.scale + self.half.x,
            (self.center.y - p.y) * self.scale + self.half.y,
        )
    }

    pub fn to_world(self, p: Point) -> Point {
        Point::new(
            (p.x - self.half.x) / self.scale + self.center.x,
            self.center.y - (p.y - self.half.y) / self.scale,
        )
    }

    /// Zone visible, en coordonnées projetées.
    fn visible(&self) -> Rectangle {
        let w = self.half.x * 2.0 / self.scale;
        let h = self.half.y * 2.0 / self.scale;
        Rectangle::new(
            Point::new(self.center.x - w / 2.0, self.center.y - h / 2.0),
            Size::new(w, h),
        )
    }

    /// Déplacement (en coordonnées projetées) à ajouter au centre de la caméra pour que
    /// la boîte `r` tienne dans l'écran privé des marges `insets` ; nul si c'est déjà le cas.
    pub fn nudge(&self, r: Rectangle, insets: Insets) -> Vector {
        let a = self.to_screen(Point::new(r.x, r.y + r.height));
        let b = self.to_screen(Point::new(r.x + r.width, r.y));
        let (left, top) = (insets.left, insets.top);
        let (right, bottom) = (
            self.half.x * 2.0 - insets.right,
            self.half.y * 2.0 - insets.bottom,
        );
        let shift = |low: f32, high: f32, min: f32, max: f32| {
            if high - low > max - min {
                (low + high) / 2.0 - (min + max) / 2.0
            } else if low < min {
                low - min
            } else if high > max {
                high - max
            } else {
                0.0
            }
        };
        let dx = shift(a.x, b.x, left, right);
        let dy = shift(a.y, b.y, top, bottom);
        // L'axe y de l'écran pointe vers le bas, celui de la carte vers le haut.
        Vector::new(dx / self.scale, -dy / self.scale)
    }
}

/// Ce qui se trouve sous le curseur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Country(usize),
    Territory(usize),
}

impl Place {
    pub fn country(self) -> Option<usize> {
        match self {
            Place::Country(c) => Some(c),
            Place::Territory(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MapEvent {
    Camera(Camera),
    Hover(Option<Place>),
    Click(Option<usize>),
    Resized(Size),
    /// Fin d'un déplacement de la carte à la souris.
    DragEnded,
    /// Clic sur la pastille « globe » d'un encart (index dans `info.insets`).
    Globe(usize),
}

/// Mise en évidence temporaire d'un pays (bonne ou mauvaise réponse).
#[derive(Debug, Clone, Copy)]
pub struct Flash {
    pub country: usize,
    pub color: Color,
    /// Intensité entre 0 et 1 (décroît avec le temps).
    pub strength: f32,
}

pub struct MapView<'a> {
    pub world: &'a World,
    pub projection: &'a Projection,
    pub view: Rectangle,
    pub camera: Camera,
    pub quiz: &'a Quiz,
    pub hover: Option<Place>,
    /// Pays à faire deviner (mode Nommer).
    pub target: Option<usize>,
    pub flash: Option<Flash>,
    /// Phase d'animation, entre 0 et 1.
    pub pulse: f32,
    /// Fin de partie : tous les noms sont révélés.
    pub reveal_all: bool,
    /// Les clics désignent un pays (mode Localiser).
    pub clickable: bool,
    /// Aperçu de l'écran d'accueil : pays de la zone mis en évidence, carte figée.
    pub preview: bool,
    /// Carte de situation : terres d'un encart replacées à leur position réelle.
    pub situation: Option<&'a Situation>,
    /// Exploration, sans quiz : lieux de la zone en évidence, tous nommés (ou numérotés).
    pub explore: bool,
    /// Langue des noms affichés.
    pub lang: Lang,
    /// Lieux mis en avant (repères), d'une autre couleur.
    pub emphasis: &'a [usize],
    /// Carte de maîtrise (aperçu, exploration) : niveau de chaque lieu (index dans
    /// `countries`), `None` s'il n'a jamais été joué.
    pub mastery: Option<&'a [Option<Level>]>,
    /// Étiquettes écrites sur tous les lieux de la zone, joués ou non (carte de repère).
    pub labels: MapLabels,
    pub cache: &'a Cache,
}

/// Terres d'un encart (outre-mer, Alaska…) replacées à leur position réelle, projetées.
pub struct Situation {
    pub rings: Vec<Vec<Point>>,
    pub bbox: Rectangle,
}

impl Situation {
    /// Lieux de `atlas` dessinés dans l'encart, ramenés à leur vraie place sur une carte
    /// centrée sur la longitude `center`.
    pub fn new(atlas: &World, inset: &geo_core::Inset, center: f32) -> Self {
        let rings: Vec<Vec<Point>> = atlas
            .countries
            .iter()
            .filter(|c| inset.contains(c.label))
            .flat_map(|c| &c.shape.rings)
            .map(|ring| {
                ring.iter()
                    .map(|&p| {
                        let [lon, lat] = inset.real(p);
                        let [x, y] = project(lon, lat, center);
                        Point::new(x, y)
                    })
                    .collect()
            })
            .collect();
        let bbox = bounds(rings.iter().flatten());
        Self { rings, bbox }
    }
}

#[derive(Default)]
pub struct Interaction {
    drag: Option<Drag>,
    /// Le curseur survole la pastille « globe » d'un encart.
    on_globe: bool,
}

/// Rayon de la pastille « globe » posée au coin de chaque encart.
const GLOBE_RADIUS: f32 = 11.0;

struct Drag {
    start: Point,
    camera: Camera,
    moved: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Tint {
    Outside,
    Pending,
    Perfect,
    Second,
    Third,
    Missed,
    /// Pays de la zone, dans l'aperçu.
    Highlight,
    /// Repère mis en avant pendant l'exploration (départements des dizaines).
    Emphasis,
}

impl Tint {
    const ALL: [Tint; 8] = [
        Tint::Outside,
        Tint::Pending,
        Tint::Perfect,
        Tint::Second,
        Tint::Third,
        Tint::Missed,
        Tint::Highlight,
        Tint::Emphasis,
    ];

    fn color(self) -> Color {
        match self {
            Tint::Outside => style::LAND_OUTSIDE,
            Tint::Pending => style::LAND,
            Tint::Perfect => style::SUCCESS,
            Tint::Second => style::WARNING,
            Tint::Third => style::ORANGE,
            Tint::Missed => style::DANGER,
            Tint::Highlight => style::mix(style::LAND, style::ACCENT, 0.6),
            Tint::Emphasis => style::mix(style::LAND, style::WARNING, 0.7),
        }
    }
}

/// Teinte d'un lieu d'eau (océan) : la couleur de l'état, fondue dans la mer.
fn water_color(k: Tint) -> Color {
    let water = color!(0x8fbfff);
    match k {
        Tint::Outside => style::OCEAN,
        Tint::Pending => style::mix(style::OCEAN, water, 0.22),
        Tint::Highlight => style::mix(style::OCEAN, style::ACCENT, 0.6),
        k => style::mix(style::OCEAN, k.color(), 0.6),
    }
}

/// Couleur du tracé d'un fleuve : cyan à trouver, la couleur de l'état une fois joué.
fn line_color(k: Tint) -> Color {
    let river = color!(0x5cc8e8);
    match k {
        Tint::Outside => style::mix(style::LAND, river, 0.35),
        Tint::Pending => river,
        Tint::Highlight => style::lighten(style::ACCENT, 0.1),
        k => k.color(),
    }
}

/// Épaisseur d'un fleuve, en pixels : plus épais de près.
fn line_width(t: &Transform) -> f32 {
    (1.2 + t.scale / 600.0).clamp(1.6, 3.4)
}

pub fn mark_color(mark: Option<Mark>) -> Color {
    tint(mark).color()
}

fn tint(mark: Option<Mark>) -> Tint {
    match mark {
        None => Tint::Outside,
        Some(Mark::Pending) => Tint::Pending,
        Some(Mark::Found { tries: 1 }) => Tint::Perfect,
        Some(Mark::Found { tries: 2 }) => Tint::Second,
        Some(Mark::Found { .. }) => Tint::Third,
        Some(Mark::Missed) => Tint::Missed,
    }
}

impl MapView<'_> {
    /// Lieux d'eau, dessinés sous les terres (océans).
    fn water(&self) -> bool {
        self.world.info.water
    }

    /// Lieux linéaires, tracés sur les terres (fleuves).
    fn lines(&self) -> bool {
        self.world.info.lines
    }

    /// Lieux ponctuels (capitales) : des pastilles, à toutes les échelles.
    fn points(&self) -> bool {
        self.world.info.points
    }

    /// Eaux intérieures (lacs), peintes par-dessus les terres.
    fn inland(&self) -> bool {
        self.world.info.inland_water
    }

    /// Couleur de remplissage d'une teinte, selon que les lieux sont des terres ou de l'eau.
    fn fill_color(&self, k: Tint) -> Color {
        // Lac à trouver : cyan, comme les fleuves, qui se détache des terres, de l'océan et du
        // bleu de la cible.
        if self.inland() && k == Tint::Pending {
            return style::mix(style::OCEAN, color!(0x5cc8e8), 0.55);
        }
        if self.water() || self.inland() {
            water_color(k)
        } else {
            k.color()
        }
    }

    /// Teinte d'un pays : son état dans la partie, ou sa présence dans la zone (aperçu).
    fn tint_of(&self, country: usize) -> Tint {
        match self.quiz.mark(country) {
            // Océan à nommer, ou choisi pendant l'exploration (où tous sont déjà en bleu) : en
            // évidence dans le fond de carte, puisque les terres recouvriraient un calque.
            Some(Mark::Pending) if self.water() && self.target == Some(country) => {
                if self.explore {
                    Tint::Emphasis
                } else {
                    Tint::Highlight
                }
            }
            Some(Mark::Pending) if self.emphasis.contains(&country) => Tint::Emphasis,
            // Mêmes couleurs que la carte « À revoir » ; gris si jamais joué.
            Some(Mark::Pending) if let Some(levels) = self.mastery => {
                match levels.get(country).copied().flatten() {
                    Some(Level::Known) => Tint::Perfect,
                    Some(Level::Learning) => Tint::Second,
                    Some(Level::Weak) => Tint::Third,
                    None => Tint::Pending,
                }
            }
            Some(Mark::Pending) if self.preview || self.explore => Tint::Highlight,
            // Carte de situation : toutes les terres en gris neutre, aucune n'est à trouver.
            None if self.situation.is_some() => Tint::Pending,
            mark => tint(mark),
        }
    }

    fn rings(&self, country: usize, t: &Transform) -> &Vec<Vec<Point>> {
        let shape = &self.projection.countries[country];
        if t.scale >= DETAIL_SCALE {
            &shape.hi
        } else {
            &shape.lo
        }
    }

    /// Lieu trop petit à l'écran, signalé par une pastille (jamais un fleuve : son tracé suffit).
    fn is_tiny(&self, country: usize, t: &Transform) -> bool {
        if self.lines() {
            return false;
        }
        if self.points() {
            return true;
        }
        let core = self.projection.countries[country].core;
        core.width.max(core.height) * t.scale < TINY_PX
    }

    /// Le nom d'un pays peut-il être affiché ?
    pub fn reveals(&self, country: usize) -> bool {
        self.reveal_all || !matches!(self.quiz.mark(country), Some(Mark::Pending))
    }

    fn hit(&self, t: &Transform, screen: Point) -> Option<Place> {
        // Pastille la plus proche (les capitales d'Europe ou des Antilles sont serrées).
        let marker = self
            .quiz
            .members()
            .iter()
            .filter(|&&c| self.projection.bubbles[c].is_empty() && self.is_tiny(c, t))
            .map(|&c| (c, t.to_screen(self.projection.label(c)).distance(screen)))
            .filter(|&(_, d)| d <= MARKER_RADIUS + 5.0)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((c, _)) = marker {
            return Some(Place::Country(c));
        }
        let p = t.to_world(screen);
        let inside = |shape: &ProjShape| shape.bbox.contains(p) && contains(&shape.hi, p);
        // Fleuves : le plus proche du curseur, à quelques pixels près ; sinon la terre survolée.
        if self.lines() {
            return self.line_at(t, screen).map(Place::Country).or_else(|| {
                self.projection
                    .territories
                    .iter()
                    .position(inside)
                    .map(Place::Territory)
            });
        }
        // Océans : ils passent sous les terres, qui l'emportent donc au clic.
        if self.water()
            && let Some(land) = self.projection.territories.iter().position(inside)
        {
            return Some(Place::Territory(land));
        }
        self.projection
            .countries
            .iter()
            .position(inside)
            .map(Place::Country)
            .or_else(|| self.bubble_at(t, screen).map(Place::Country))
            .or_else(|| {
                self.projection
                    .territories
                    .iter()
                    .position(inside)
                    .map(Place::Territory)
            })
    }

    /// Fleuve (index dans `countries`) le plus proche d'un point de l'écran, s'il en passe un à
    /// moins de `LINE_HIT_PX`.
    fn line_at(&self, t: &Transform, screen: Point) -> Option<usize> {
        let p = t.to_world(screen);
        let margin = LINE_HIT_PX / t.scale;
        let mut best: Option<(usize, f32)> = None;
        for (c, shape) in self.projection.countries.iter().enumerate() {
            let b = shape.bbox;
            if p.x < b.x - margin
                || p.x > b.x + b.width + margin
                || p.y < b.y - margin
                || p.y > b.y + b.height + margin
            {
                continue;
            }
            for line in self.rings(c, t) {
                for w in line.windows(2) {
                    let d = segment_distance(screen, t.to_screen(w[0]), t.to_screen(w[1]));
                    if d <= LINE_HIT_PX && best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((c, d));
                    }
                }
            }
        }
        best.map(|(c, _)| c)
    }

    /// Couleur d'un territoire : celle de son pays une fois celui-ci joué, un peu assombrie.
    fn territory_color(&self, territory: usize) -> Color {
        if self.water() || self.lines() || self.points() || self.inland() {
            // Fond de carte : les terres du pays (départements sous les préfectures) claires,
            // les pays voisins plus sombres.
            return if self.world.territories[territory].context {
                style::TERRITORY
            } else {
                style::LAND
            };
        }
        let owner = self.world.territories[territory].owner;
        match owner.and_then(|o| self.quiz.mark(o)) {
            Some(mark @ (Mark::Found { .. } | Mark::Missed)) => {
                style::mix(style::TERRITORY, mark_color(Some(mark)), 0.75)
            }
            _ => style::TERRITORY,
        }
    }

    fn place_rings(&self, place: Place, t: &Transform) -> &Vec<Vec<Point>> {
        let shape = match place {
            Place::Country(c) => &self.projection.countries[c],
            Place::Territory(i) => &self.projection.territories[i],
        };
        if t.scale >= DETAIL_SCALE {
            &shape.hi
        } else {
            &shape.lo
        }
    }

    /// Nom affiché au survol, s'il peut être révélé.
    fn place_name(&self, place: Place) -> Option<String> {
        match place {
            Place::Country(c) if self.reveals(c) => {
                let country = &self.world.countries[c];
                Some(
                    match self
                        .world
                        .info
                        .numbers
                        .then(|| crate::number_of(country))
                        .flatten()
                    {
                        Some(number) => format!("{number} · {}", country.name.get(self.lang)),
                        None => country.name.get(self.lang).to_owned(),
                    },
                )
            }
            // Pas encore joué : seulement ce que les étiquettes montrent déjà.
            Place::Country(c) => self.label_of(c),
            Place::Territory(i) => {
                let territory = &self.world.territories[i];
                Some(match territory.owner {
                    Some(owner) if self.reveals(owner) => {
                        format!(
                            "{} · {}",
                            territory.name.get(self.lang),
                            self.world.countries[owner].name.get(self.lang)
                        )
                    }
                    _ => territory.name.get(self.lang).to_owned(),
                })
            }
        }
    }

    /// Étiquette d'un lieu sur la carte de repère, selon le choix de l'utilisateur.
    fn label_of(&self, c: usize) -> Option<String> {
        let country = &self.world.countries[c];
        match self.labels {
            MapLabels::None => None,
            MapLabels::Names => Some(country.name.get(self.lang).to_owned()),
            MapLabels::Numbers => crate::number_of(country),
        }
    }

    /// Archipel dont une bulle contient le point (en pixels). Les bulles se chevauchent
    /// parfois (Tuvalu sur Kiribati) : la dernière dessinée, donc visible, l'emporte.
    fn bubble_at(&self, t: &Transform, screen: Point) -> Option<usize> {
        self.projection
            .bubbles
            .iter()
            .rposition(|bubbles| bubbles.iter().any(|b| b.contains(t, screen)))
    }

    /// Bulles d'un archipel : contour lissé de chaque groupe d'îles, élargi de sa marge.
    fn draw_bubble(&self, frame: &mut Frame, country: usize, t: &Transform, color: Color) {
        for bubble in &self.projection.bubbles[country] {
            let outline = bubble.outline(t);
            let margin = bubble.margin(t);
            let path = Path::new(|b| {
                let mut points = outline.iter().copied();
                if let Some(first) = points.next() {
                    b.move_to(first);
                    for p in points {
                        b.line_to(p);
                    }
                    b.close();
                }
            });
            frame.fill(&path, color);
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(color)
                    .with_width(margin * 2.0)
                    .with_line_cap(LineCap::Round)
                    .with_line_join(LineJoin::Round),
            );
            // Île minuscule : contour dégénéré, un disque garantit la bulle.
            if outline.len() < 3
                && let Some(&p) = outline.first()
            {
                frame.fill(&Path::circle(p, margin), color);
            }
        }
    }

    /// Couleur de la bulle d'un archipel selon son état dans la partie (opaque, sur l'océan).
    fn bubble_color(&self, country: usize) -> Color {
        let water = color!(0x8fbfff);
        match self.quiz.mark(country) {
            None => style::mix(style::OCEAN, water, 0.10),
            // Carte de maîtrise : la couleur du niveau, comme les terres et les pastilles.
            Some(Mark::Pending) if self.mastery.is_some() => {
                style::mix(style::OCEAN, self.tint_of(country).color(), 0.45)
            }
            Some(Mark::Pending) if self.preview || self.explore => {
                style::mix(style::OCEAN, style::ACCENT, 0.5)
            }
            Some(Mark::Pending) => style::mix(style::OCEAN, water, 0.32),
            mark => style::mix(style::OCEAN, mark_color(mark), 0.45),
        }
    }

    /// Contour d'un lieu d'eau, sans ses côtés posés sur l'antiméridien : ce n'est qu'une coupure
    /// des données (mer de Ross, de Béring) ou le bord de la carte, pas une limite.
    fn add_water_outline(
        builder: &mut Builder,
        rings: &[Vec<Point>],
        raw: &[geo_core::world::Ring],
        t: &Transform,
    ) {
        for (i, ring) in rings.iter().enumerate() {
            let source = raw.get(i).filter(|r| r.len() == ring.len());
            let Some(source) = source else {
                // Anneau recoupé autour d'un pôle : tracé tel quel.
                Self::add_rings(builder, std::slice::from_ref(ring), t);
                continue;
            };
            let n = ring.len();
            let mut open = false;
            for k in 0..n {
                let (a, b) = (k, (k + 1) % n);
                let seam = source[a][0].abs() >= 179.999 && source[b][0].abs() >= 179.999;
                if seam {
                    open = false;
                    continue;
                }
                if !open {
                    builder.move_to(t.to_screen(ring[a]));
                    open = true;
                }
                builder.line_to(t.to_screen(ring[b]));
            }
        }
    }

    /// Tracés ouverts (fleuves).
    fn add_lines(builder: &mut Builder, lines: &[Vec<Point>], t: &Transform) {
        for line in lines {
            let mut points = line.iter().map(|&p| t.to_screen(p));
            if let Some(first) = points.next() {
                builder.move_to(first);
                for p in points {
                    builder.line_to(p);
                }
            }
        }
    }

    /// Trait d'un fleuve, arrondi aux extrémités et aux coudes.
    fn line_stroke(color: Color, width: f32) -> Stroke<'static> {
        Stroke::default()
            .with_color(color)
            .with_width(width)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round)
    }

    /// Grands lacs (couleur de la mer, sur les terres), puis tous les fleuves, groupés par teinte.
    fn draw_lines(&self, frame: &mut Frame, t: &Transform, visible: &Rectangle) {
        let detailed = t.scale >= DETAIL_SCALE;
        let mut lakes = Builder::new();
        for shape in &self.projection.lakes {
            if shape.bbox.intersects(visible) {
                Self::add_rings(&mut lakes, if detailed { &shape.hi } else { &shape.lo }, t);
            }
        }
        let lakes = lakes.build();
        frame.fill(
            &lakes,
            Fill {
                style: style::OCEAN.into(),
                rule: Rule::EvenOdd,
            },
        );
        frame.stroke(
            &lakes,
            Stroke::default().with_color(style::COAST).with_width(0.8),
        );
        let mut buckets: Vec<(Tint, Builder)> =
            Tint::ALL.iter().map(|&k| (k, Builder::new())).collect();
        for (i, shape) in self.projection.countries.iter().enumerate() {
            if !shape.bbox.intersects(visible) {
                continue;
            }
            let k = self.tint_of(i);
            let bucket = &mut buckets.iter_mut().find(|(b, _)| *b == k).expect("teinte").1;
            Self::add_lines(bucket, self.rings(i, t), t);
        }
        let width = line_width(t);
        for (k, builder) in buckets {
            let w = if k == Tint::Outside {
                width * 0.7
            } else {
                width
            };
            frame.stroke(&builder.build(), Self::line_stroke(line_color(k), w));
        }
    }

    fn add_rings(builder: &mut Builder, rings: &[Vec<Point>], t: &Transform) {
        for ring in rings {
            let mut points = ring.iter().map(|&p| t.to_screen(p));
            if let Some(first) = points.next() {
                builder.move_to(first);
                for p in points {
                    builder.line_to(p);
                }
                builder.close();
            }
        }
    }

    fn draw_base(&self, frame: &mut Frame, t: &Transform) {
        let visible = t.visible();
        let detailed = t.scale >= DETAIL_SCALE;
        let mut coast = Builder::new();

        // Bulles des archipels, sous les terres.
        for (c, shape) in self.projection.countries.iter().enumerate() {
            // Pas de bulles sur la carte de situation : seules les terres de l'encart comptent.
            if self.situation.is_none()
                && !self.projection.bubbles[c].is_empty()
                && shape.bbox.intersects(&visible)
            {
                self.draw_bubble(frame, c, t, self.bubble_color(c));
            }
        }

        // Territoires, regroupés par couleur (après les océans, qu'ils recouvrent).
        let draw_land = |frame: &mut Frame, coast: &mut Builder| {
            let mut groups: Vec<(Color, Builder)> = Vec::new();
            for (i, shape) in self.projection.territories.iter().enumerate() {
                if !shape.bbox.intersects(&visible) {
                    continue;
                }
                let color = self.territory_color(i);
                let index = match groups.iter().position(|(c, _)| *c == color) {
                    Some(index) => index,
                    None => {
                        groups.push((color, Builder::new()));
                        groups.len() - 1
                    }
                };
                let rings = if detailed { &shape.hi } else { &shape.lo };
                Self::add_rings(&mut groups[index].1, rings, t);
                Self::add_rings(coast, rings, t);
            }
            for (color, builder) in groups {
                frame.fill(
                    &builder.build(),
                    Fill {
                        style: color.into(),
                        rule: Rule::EvenOdd,
                    },
                );
            }
        };
        if !self.water() {
            draw_land(frame, &mut coast);
        }
        if self.lines() {
            // Frontières d'abord : les fleuves passent par-dessus.
            frame.stroke(
                &std::mem::replace(&mut coast, Builder::new()).build(),
                Stroke::default().with_color(style::COAST).with_width(0.8),
            );
            self.draw_lines(frame, t, &visible);
        }

        let mut buckets: Vec<(Tint, Builder)> =
            Tint::ALL.iter().map(|&k| (k, Builder::new())).collect();
        for (i, shape) in self.projection.countries.iter().enumerate() {
            // Fleuves tracés à part ; capitales : seulement leur pastille.
            if self.lines() || self.points() || !shape.bbox.intersects(&visible) {
                continue;
            }
            let rings = self.rings(i, t);
            let k = self.tint_of(i);
            let bucket = &mut buckets.iter_mut().find(|(b, _)| *b == k).expect("teinte").1;
            Self::add_rings(bucket, rings, t);
            if self.water() {
                let shape = &self.world.countries[i].shape;
                let raw = if detailed {
                    &shape.rings
                } else {
                    &shape.rings_lo
                };
                Self::add_water_outline(&mut coast, rings, raw, t);
            } else {
                Self::add_rings(&mut coast, rings, t);
            }
        }
        for (k, builder) in buckets {
            frame.fill(
                &builder.build(),
                Fill {
                    style: self.fill_color(k).into(),
                    rule: Rule::EvenOdd,
                },
            );
        }
        if self.water() {
            // Limites entre océans, discrètes, puis les terres et leurs côtes par-dessus.
            frame.stroke(
                &coast.build(),
                Stroke::default()
                    .with_color(style::with_alpha(style::MUTED, 0.35))
                    .with_width(1.0),
            );
            coast = Builder::new();
            draw_land(frame, &mut coast);
        }
        frame.stroke(
            &coast.build(),
            Stroke::default().with_color(style::COAST).with_width(0.8),
        );

        // Cadres des encarts (outre-mer, Alaska, Hawaï) : ces terres ont été déplacées.
        for inset in &self.world.info.insets {
            let r = view_rect(inset.frame, self.projection.center);
            let a = t.to_screen(Point::new(r.x, r.y + r.height));
            let b = t.to_screen(Point::new(r.x + r.width, r.y));
            let path = Path::rounded_rectangle(a, Size::new(b.x - a.x, b.y - a.y), 6.0.into());
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(style::with_alpha(style::MUTED, 0.45))
                    .with_width(1.0),
            );
        }

        // Pastilles des micro-États (les archipels ont leur bulle).
        for &c in self.quiz.members() {
            if !self.projection.bubbles[c].is_empty()
                || !self.is_tiny(c, t)
                || !visible.contains(self.projection.label(c))
            {
                continue;
            }
            let center = t.to_screen(self.projection.label(c));
            let color = self.fill_color(self.tint_of(c));
            // Plus discrètes sur la petite carte d'aperçu.
            let radius = if self.preview {
                MARKER_RADIUS * 0.6
            } else {
                MARKER_RADIUS
            };
            let dot = Path::circle(center, radius);
            // Capitale à trouver : plus claire que les terres, sur lesquelles elle est posée.
            let fill = if self.points() && self.tint_of(c) == Tint::Pending {
                style::lighten(style::LAND, 0.45)
            } else {
                style::lighten(color, 0.15)
            };
            frame.fill(&dot, fill);
            frame.stroke(
                &dot,
                Stroke::default().with_color(style::COAST).with_width(1.2),
            );
        }

        // Noms des pays déjà joués (tous en exploration), quand ils tiennent dans leur contour.
        // Sans carte dans le quiz, ce sont les numéros qui s'affichent. Les capitales ont leur
        // nom à droite de leur pastille, s'il ne chevauche pas un nom déjà posé.
        let mut placed: Vec<Rectangle> = Vec::new();
        for &c in self.quiz.members() {
            let played = matches!(self.quiz.mark(c), Some(Mark::Found { .. } | Mark::Missed));
            let core = self.projection.countries[c].core;
            let country = &self.world.countries[c];
            let name = match self.label_of(c) {
                Some(label) => label,
                None if !self.explore && !played => continue,
                None => match self.world.info.numbers.then(|| crate::number_of(country)) {
                    Some(Some(number)) => number,
                    _ => country.name.get(self.lang).to_owned(),
                },
            };
            let name = &name;
            let size = if t.scale > 2000.0 { 13.0 } else { 11.0 };
            let width = name.chars().count() as f32 * size * 0.56;
            let at = self.projection.label(c);
            if !visible.contains(at) {
                continue;
            }
            let mut position = t.to_screen(at);
            let align_x = if self.points() {
                position.x += MARKER_RADIUS + 4.0;
                let room = Rectangle::new(
                    Point::new(position.x - 2.0, position.y - size * 0.7),
                    Size::new(width + 4.0, size * 1.4),
                );
                if placed.iter().any(|r| r.intersects(&room)) {
                    continue;
                }
                placed.push(room);
                Alignment::Left
            } else {
                if core.width * t.scale < width * 0.9 || core.height * t.scale < size + 2.0 {
                    continue;
                }
                Alignment::Center
            };
            let text = |color: Color, offset: Vector| Text {
                content: name.clone(),
                position: position + offset,
                color,
                size: Pixels(size),
                font: style::FONT_SEMIBOLD,
                align_x,
                align_y: Vertical::Center,
                ..Text::default()
            };
            // Liseré sombre pour rester lisible sur toutes les couleurs.
            let halo = style::with_alpha(style::LABEL_DARK, 0.75);
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                frame.fill_text(text(halo, Vector::new(dx, dy)));
            }
            frame.fill_text(text(Color::WHITE, Vector::ZERO));
        }
    }

    /// Pastilles « globe » des encarts (coin supérieur droit du cadre), en pixels.
    fn globes(&self, t: &Transform) -> Vec<Point> {
        if self.preview {
            return Vec::new();
        }
        self.world
            .info
            .insets
            .iter()
            .map(|inset| {
                let r = view_rect(inset.frame, self.projection.center);
                t.to_screen(Point::new(r.x + r.width, r.y + r.height))
            })
            .collect()
    }

    fn globe_at(&self, t: &Transform, p: Point) -> Option<usize> {
        self.globes(t)
            .iter()
            .position(|c| c.distance(p) <= GLOBE_RADIUS + 3.0)
    }

    fn draw_globes(&self, frame: &mut Frame, t: &Transform, cursor: Option<Point>) {
        let hovered = cursor.and_then(|p| self.globe_at(t, p));
        for (i, center) in self.globes(t).into_iter().enumerate() {
            let on = hovered == Some(i);
            let badge = Path::circle(center, GLOBE_RADIUS);
            frame.fill(&badge, if on { style::ACCENT } else { style::SURFACE_3 });
            frame.stroke(
                &badge,
                Stroke::default()
                    .with_color(if on { style::ACCENT } else { style::BORDER })
                    .with_width(1.0),
            );
            // Globe : contour, équateur et méridien.
            let r = GLOBE_RADIUS * 0.55;
            let globe = Path::new(|b| {
                b.circle(center, r);
                b.move_to(Point::new(center.x - r, center.y));
                b.line_to(Point::new(center.x + r, center.y));
                b.ellipse(canvas::path::arc::Elliptical {
                    center,
                    radii: Vector::new(r * 0.45, r),
                    rotation: iced::Radians(0.0),
                    start_angle: iced::Radians(0.0),
                    end_angle: iced::Radians(std::f32::consts::TAU),
                });
            });
            frame.stroke(
                &globe,
                Stroke::default()
                    .with_color(if on { Color::WHITE } else { style::TEXT })
                    .with_width(1.2),
            );
        }
        if let (Some(_), Some(p)) = (hovered, cursor) {
            tag(
                frame,
                &crate::tr!(
                    self.lang,
                    "Voir sur la carte du monde",
                    "Show on the world map"
                ),
                Point::new(p.x, p.y - 24.0),
                style::SURFACE_3,
                1.0,
            );
        }
    }

    /// Terres de l'encart à leur vraie place, entourées si elles sont minuscules à l'écran.
    fn draw_situation(&self, frame: &mut Frame, t: &Transform) {
        let Some(situation) = self.situation else {
            return;
        };
        let mut b = Builder::new();
        Self::add_rings(&mut b, &situation.rings, t);
        let path = b.build();
        frame.fill(
            &path,
            Fill {
                style: style::ACCENT.into(),
                rule: Rule::EvenOdd,
            },
        );
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(style::lighten(style::ACCENT, 0.6))
                .with_width(1.2),
        );
        let r = situation.bbox;
        let a = t.to_screen(Point::new(r.x, r.y + r.height));
        let b = t.to_screen(Point::new(r.x + r.width, r.y));
        let size = (b.x - a.x).max(b.y - a.y);
        if size < 18.0 {
            let center = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            frame.stroke(
                &Path::circle(center, size / 2.0 + 9.0),
                Stroke::default()
                    .with_color(style::lighten(style::ACCENT, 0.3))
                    .with_width(2.0),
            );
        }
    }

    fn draw_overlay(&self, frame: &mut Frame, t: &Transform, cursor: Option<Point>) {
        self.draw_situation(frame, t);
        // Lieux d'eau : la cible est mise en évidence dans le fond de carte (un calque passerait
        // sur les terres) ; seuls les cercles des petites cibles sont dessinés ici.
        if let Some(target) = self.target {
            let pulse = 0.5 + 0.5 * (self.pulse * std::f32::consts::TAU).sin();
            let bubbled = !self.projection.bubbles[target].is_empty();
            if bubbled {
                let color = style::mix(style::OCEAN, style::ACCENT, 0.35 + 0.25 * pulse);
                self.draw_bubble(frame, target, t, color);
            }
            if self.lines() {
                // Fleuve à nommer : halo clair qui pulse sous un trait épais.
                let mut b = Builder::new();
                Self::add_lines(&mut b, self.rings(target, t), t);
                let path = b.build();
                let width = line_width(t);
                frame.stroke(
                    &path,
                    Self::line_stroke(
                        style::with_alpha(Color::WHITE, 0.25 + 0.35 * pulse),
                        width * 2.0 + 5.0,
                    ),
                );
                frame.stroke(&path, Self::line_stroke(style::ACCENT, width + 2.0));
            } else if !self.water() {
                let mut b = Builder::new();
                Self::add_rings(&mut b, self.rings(target, t), t);
                let path = b.build();
                frame.fill(
                    &path,
                    Fill {
                        style: style::with_alpha(style::ACCENT, 0.55 + 0.4 * pulse).into(),
                        rule: Rule::EvenOdd,
                    },
                );
                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_color(style::lighten(style::ACCENT, 0.5))
                        .with_width(1.5),
                );
            }
            // Une mer, même moyenne, se perd entre les côtes : cercles jusqu'à une taille plus grande.
            let small = if self.water() { 60.0 } else { 24.0 };
            let core = self.projection.core(target);
            if !bubbled && core.width.max(core.height) * t.scale < small {
                let center = t.to_screen(self.projection.label(target));
                for (r, a) in [(10.0 + 10.0 * pulse, 1.0 - pulse), (6.0, 1.0)] {
                    frame.stroke(
                        &Path::circle(center, r),
                        Stroke::default()
                            .with_color(style::with_alpha(style::ACCENT, a))
                            .with_width(2.5),
                    );
                }
            }
        }

        if let Some(flash) = self.flash {
            let bubbled = !self.projection.bubbles[flash.country].is_empty();
            if bubbled {
                let color = style::mix(
                    self.bubble_color(flash.country),
                    flash.color,
                    0.6 * flash.strength,
                );
                self.draw_bubble(frame, flash.country, t, color);
            }
            if self.lines() {
                let mut b = Builder::new();
                Self::add_lines(&mut b, self.rings(flash.country, t), t);
                frame.stroke(
                    &b.build(),
                    Self::line_stroke(
                        style::with_alpha(flash.color, flash.strength),
                        line_width(t) + 3.0,
                    ),
                );
            }
            // Océans : pas de remplissage, il passerait sur les terres ; la teinte du fond suffit.
            else if !self.water() {
                let mut b = Builder::new();
                Self::add_rings(&mut b, self.rings(flash.country, t), t);
                let path = b.build();
                frame.fill(
                    &path,
                    Fill {
                        style: style::with_alpha(flash.color, 0.9 * flash.strength).into(),
                        rule: Rule::EvenOdd,
                    },
                );
            }
            let center = t.to_screen(self.projection.label(flash.country));
            if !bubbled && self.is_tiny(flash.country, t) {
                frame.stroke(
                    &Path::circle(center, 12.0),
                    Stroke::default()
                        .with_color(style::with_alpha(flash.color, flash.strength))
                        .with_width(3.0),
                );
            }
            let name = self.world.countries[flash.country].name.get(self.lang);
            tag(
                frame,
                name,
                Point::new(center.x, center.y - 22.0),
                flash.color,
                flash.strength,
            );
        }

        if let Some(place) = self.hover {
            if let (true, Place::Country(c)) = (self.lines(), place) {
                // Fleuve survolé : liseré blanc sous sa couleur.
                let mut b = Builder::new();
                Self::add_lines(&mut b, self.rings(c, t), t);
                let path = b.build();
                let width = line_width(t);
                frame.stroke(
                    &path,
                    Self::line_stroke(style::with_alpha(Color::WHITE, 0.85), width + 3.0),
                );
                frame.stroke(&path, Self::line_stroke(line_color(self.tint_of(c)), width));
                if let (Some(cursor), Some(name)) = (cursor, self.place_name(place)) {
                    tag(
                        frame,
                        &name,
                        Point::new(cursor.x, cursor.y - 24.0),
                        style::SURFACE_3,
                        1.0,
                    );
                }
                self.draw_globes(frame, t, cursor);
                return;
            }
            let mut b = Builder::new();
            Self::add_rings(&mut b, self.place_rings(place, t), t);
            let path = b.build();
            // Archipel : bulle éclaircie, puis ses îles redessinées par-dessus.
            if let Place::Country(c) = place
                && !self.projection.bubbles[c].is_empty()
            {
                self.draw_bubble(frame, c, t, style::lighten(self.bubble_color(c), 0.12));
                frame.fill(
                    &path,
                    Fill {
                        style: mark_color(self.quiz.mark(c)).into(),
                        rule: Rule::EvenOdd,
                    },
                );
            }
            // Le contour d'un océan passe sous les terres : pas de liseré.
            if !(self.water() && matches!(place, Place::Country(_))) {
                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_color(style::with_alpha(Color::WHITE, 0.85))
                        .with_width(1.6),
                );
            }
            if let Place::Country(hover) = place
                && self.projection.bubbles[hover].is_empty()
                && self.is_tiny(hover, t)
            {
                let center = t.to_screen(self.projection.label(hover));
                frame.stroke(
                    &Path::circle(center, MARKER_RADIUS + 2.5),
                    Stroke::default().with_color(Color::WHITE).with_width(1.5),
                );
            }
            if let (Some(cursor), Some(name)) = (cursor, self.place_name(place)) {
                tag(
                    frame,
                    &name,
                    Point::new(cursor.x, cursor.y - 24.0),
                    style::SURFACE_3,
                    1.0,
                );
            }
        }
        self.draw_globes(frame, t, cursor);
    }
}

/// Étiquette arrondie centrée sur `at`.
fn tag(frame: &mut Frame, text: &str, at: Point, background: Color, alpha: f32) {
    let size = 13.0;
    let width = text.chars().count() as f32 * size * 0.58 + 20.0;
    let height = 26.0;
    let x = (at.x - width / 2.0).clamp(4.0, (frame.width() - width - 4.0).max(4.0));
    let y = (at.y - height / 2.0).clamp(4.0, (frame.height() - height - 4.0).max(4.0));
    let rect = Path::rounded_rectangle(Point::new(x, y), Size::new(width, height), 8.0.into());
    frame.fill(
        &rect,
        style::with_alpha(style::lighten(background, 0.0), 0.95 * alpha),
    );
    frame.stroke(
        &rect,
        Stroke::default()
            .with_color(style::with_alpha(Color::BLACK, 0.35 * alpha))
            .with_width(1.0),
    );
    frame.fill_text(Text {
        content: text.to_owned(),
        position: Point::new(x + width / 2.0, y + height / 2.0),
        color: style::with_alpha(Color::WHITE, alpha),
        size: Pixels(size),
        font: style::FONT_SEMIBOLD,
        align_x: Alignment::Center,
        align_y: Vertical::Center,
        ..Text::default()
    });
}

/// Test pair-impair sur l'ensemble des anneaux.
fn contains(rings: &[Vec<Point>], p: Point) -> bool {
    let mut inside = false;
    for ring in rings {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (ring[i], ring[j]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

impl canvas::Program<MapEvent> for MapView<'_> {
    type State = Interaction;

    fn update(
        &self,
        state: &mut Interaction,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<MapEvent>> {
        if self.preview {
            return None;
        }
        let t = Transform::new(self.camera, self.view, bounds.size());
        let Event::Mouse(event) = event else {
            return None;
        };
        match event {
            mouse::Event::WheelScrolled { delta } => {
                let anchor = cursor.position_in(bounds)?;
                let steps = match delta {
                    ScrollDelta::Lines { y, .. } => *y,
                    ScrollDelta::Pixels { y, .. } => y / 60.0,
                };
                let camera =
                    self.camera
                        .zoom_at(self.view, bounds.size(), 1.3f32.powf(steps), Some(anchor));
                Some(Action::publish(MapEvent::Camera(camera)).and_capture())
            }
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let start = cursor.position_in(bounds)?;
                if let Some(inset) = self.globe_at(&t, start) {
                    return Some(Action::publish(MapEvent::Globe(inset)).and_capture());
                }
                state.drag = Some(Drag {
                    start,
                    camera: self.camera,
                    moved: false,
                });
                Some(Action::capture())
            }
            mouse::Event::CursorMoved { position } => {
                if let Some(drag) = &mut state.drag {
                    let pos = *position - Vector::new(bounds.x, bounds.y);
                    if !drag.moved && pos.distance(drag.start) > 4.0 {
                        drag.moved = true;
                    }
                    if drag.moved {
                        let s = t.scale;
                        let camera = Camera {
                            center: Point::new(
                                drag.camera.center.x - (pos.x - drag.start.x) / s,
                                drag.camera.center.y + (pos.y - drag.start.y) / s,
                            ),
                            zoom: drag.camera.zoom,
                        }
                        .clamped();
                        return Some(Action::publish(MapEvent::Camera(camera)));
                    }
                    return None;
                }
                let on_globe = cursor
                    .position_in(bounds)
                    .and_then(|p| self.globe_at(&t, p))
                    .is_some();
                if on_globe != state.on_globe {
                    state.on_globe = on_globe;
                    if !on_globe {
                        return Some(Action::request_redraw());
                    }
                }
                if on_globe {
                    // Info-bulle de la pastille, à la place du nom du lieu survolé.
                    return Some(match self.hover {
                        Some(_) => Action::publish(MapEvent::Hover(None)),
                        None => Action::request_redraw(),
                    });
                }
                let hit = cursor.position_in(bounds).and_then(|p| self.hit(&t, p));
                if hit != self.hover {
                    Some(Action::publish(MapEvent::Hover(hit)))
                } else if hit.is_some() {
                    // L'étiquette suit le curseur.
                    Some(Action::request_redraw())
                } else {
                    None
                }
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                let drag = state.drag.take()?;
                if drag.moved {
                    return Some(Action::publish(MapEvent::DragEnded).and_capture());
                }
                let country = self.hit(&t, drag.start).and_then(Place::country);
                Some(Action::publish(MapEvent::Click(country)).and_capture())
            }
            mouse::Event::CursorLeft => self.hover.map(|_| Action::publish(MapEvent::Hover(None))),
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Interaction,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let t = Transform::new(self.camera, self.view, bounds.size());
        let base = self
            .cache
            .draw(renderer, bounds.size(), |frame| self.draw_base(frame, &t));
        let mut overlay = Frame::new(renderer, bounds.size());
        self.draw_overlay(&mut overlay, &t, cursor.position_in(bounds));
        vec![base, overlay.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Interaction,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match &state.drag {
            Some(drag) if drag.moved => mouse::Interaction::Grabbing,
            _ if state.on_globe && cursor.is_over(bounds) => mouse::Interaction::Pointer,
            _ if self.clickable
                && self.hover.and_then(Place::country).is_some()
                && cursor.is_over(bounds) =>
            {
                mouse::Interaction::Pointer
            }
            _ => mouse::Interaction::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    /// Un clic sur la pastille « globe » d'un encart ouvre sa carte de situation ; un clic à
    /// côté reste un clic sur la carte.
    #[test]
    fn globe_badges_are_clickable() {
        let world = crate::ATLASES
            .iter()
            .find(|a| a.info.id == "fr-departements")
            .unwrap();
        let zone = &world.regions[0];
        let projection = Projection::new(world, zone.center);
        let view = view_rect(zone.view, zone.center);
        let quiz = Quiz::new(
            geo_core::Mode::Locate,
            world.countries.len(),
            zone.members.clone(),
            Vec::new(),
        );
        let cache = Cache::new();
        let map = MapView {
            world,
            projection: &projection,
            view,
            camera: Camera::fit(view),
            quiz: &quiz,
            hover: None,
            target: None,
            flash: None,
            pulse: 0.0,
            reveal_all: false,
            clickable: true,
            preview: false,
            situation: None,
            explore: false,
            emphasis: &[],
            mastery: None,
            labels: crate::settings::MapLabels::None,
            lang: Lang::Fr,
            cache: &cache,
        };
        let bounds = rect(0.0, 0.0, 1000.0, 700.0);
        let t = Transform::new(map.camera, view, bounds.size());
        let badges = map.globes(&t);
        assert_eq!(badges.len(), 5);
        let press = |at: Point| {
            let mut state = Interaction::default();
            let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
            canvas::Program::update(
                &map,
                &mut state,
                &event,
                bounds,
                mouse::Cursor::Available(at),
            )
            .and_then(|action| action.into_inner().0)
        };
        assert!(matches!(press(badges[3]), Some(MapEvent::Globe(3))));
        assert!(press(Point::new(badges[3].x + 40.0, badges[3].y)).is_none());
    }

    #[test]
    fn screen_and_world_round_trip() {
        let view = rect(-2.0, -1.0, 4.0, 2.0);
        let t = Transform::new(
            Camera {
                center: Point::new(0.3, -0.2),
                zoom: 2.5,
            },
            view,
            Size::new(800.0, 600.0),
        );
        let p = Point::new(0.7, 0.1);
        let back = t.to_world(t.to_screen(p));
        assert!(back.distance(p) < 1e-5);
    }

    #[test]
    fn frame_shows_the_whole_area() {
        let view = rect(-2.0, -1.0, 4.0, 2.0);
        let size = Size::new(800.0, 600.0);
        let area = rect(0.5, 0.2, 0.8, 0.4);
        let camera = Camera::frame(area, view, size);
        let t = Transform::new(camera, view, size);
        let zero = Insets {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        };
        assert_eq!(t.nudge(area, zero), Vector::ZERO);
        // Et pas beaucoup plus : la zone occupe presque toute la largeur.
        assert!(area.width * t.scale > 0.9 * 800.0);
    }

    #[test]
    fn nudge_moves_boxes_out_of_the_margins() {
        let view = rect(-2.0, -1.0, 4.0, 2.0);
        let size = Size::new(800.0, 600.0);
        let insets = Insets {
            top: 50.0,
            right: 50.0,
            bottom: 150.0,
            left: 50.0,
        };
        let camera = Camera::fit(view);
        let t = Transform::new(camera, view, size);
        // Une boîte au centre ne bouge pas.
        assert_eq!(t.nudge(rect(-0.1, -0.1, 0.2, 0.2), insets), Vector::ZERO);
        // Une boîte tout en bas remonte au-dessus de la marge du bas.
        let low = rect(-0.1, -0.98, 0.1, 0.05);
        let moved = Camera {
            center: camera.center + t.nudge(low, insets),
            ..camera
        };
        let t = Transform::new(moved, view, size);
        assert_eq!(t.nudge(low, insets), Vector::ZERO);
        let bottom = t.to_screen(Point::new(low.x, low.y)).y;
        assert!(bottom <= 600.0 - 150.0 + 0.5, "{bottom}");
    }

    #[test]
    fn bubble_wraps_its_islands_and_keeps_clear_of_other_land() {
        let island = |x: f32, y: f32| {
            vec![
                Point::new(x, y),
                Point::new(x + 0.01, y),
                Point::new(x, y + 0.01),
            ]
        };
        let (a, b) = (island(0.0, 0.0), island(0.05, 0.02));
        let points: Vec<Point> = a.iter().chain(&b).copied().collect();
        let view = rect(-2.0, -1.0, 4.0, 2.0);
        let t = Transform::new(Camera::fit(view), view, Size::new(800.0, 600.0));
        // Loin de toute terre : marge pleine.
        let free = Bubble {
            hull: convex_hull(points.clone()),
            clearance: 1.0,
        };
        assert_eq!(free.margin(&t), BUBBLE_PX);
        assert!(free.contains(&t, t.to_screen(Point::new(0.03, 0.01))));
        assert!(!free.contains(&t, t.to_screen(Point::new(0.3, 0.3))));
        // Terre voisine à 0,01 unité (environ 2 px) : marge réduite au minimum.
        let tight = Bubble {
            hull: convex_hull(points),
            clearance: 0.01,
        };
        assert!(tight.margin(&t) < BUBBLE_PX);
    }

    #[test]
    fn nearby_islands_share_a_bubble() {
        let island = |x: f32| {
            vec![
                Point::new(x, 0.0),
                Point::new(x + 0.01, 0.0),
                Point::new(x, 0.01),
            ]
        };
        let (a, b, c) = (island(0.0), island(0.05), island(1.0));
        // Deux îles voisines, une troisième loin : deux groupes.
        assert_eq!(nearby_groups(vec![&a, &b, &c]).len(), 2);
    }

    #[test]
    fn even_odd_contains_handles_holes() {
        let outer = vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 4.0),
            Point::new(0.0, 4.0),
        ];
        let hole = vec![
            Point::new(1.0, 1.0),
            Point::new(3.0, 1.0),
            Point::new(3.0, 3.0),
            Point::new(1.0, 3.0),
        ];
        let rings = vec![outer, hole];
        assert!(contains(&rings, Point::new(0.5, 0.5)));
        assert!(!contains(&rings, Point::new(2.0, 2.0)));
        assert!(!contains(&rings, Point::new(5.0, 2.0)));
    }
}

#[cfg(test)]
mod sizes {
    use super::*;
    use crate::ATLASES;

    /// Taille à l'écran (plus grande dimension, en pixels) de quelques pays sur la vue Monde.
    #[test]
    #[ignore]
    fn world_sizes() {
        let atlas = &ATLASES[0];
        let world = &atlas.regions[0];
        let projection = Projection::new(atlas, world.center);
        let view = view_rect(world.view, world.center);
        for size in [Size::new(1070.0, 800.0), Size::new(1630.0, 1020.0)] {
            let t = Transform::new(Camera::fit(view), view, size);
            let mut line = format!("{}x{} :", size.width, size.height);
            for code in [
                "SVN", "GNB", "BEL", "NLD", "CHE", "PRT", "HUN", "GHA", "FRA", "DEU", "ESP", "ITA",
                "UKR", "SEN", "KOR", "IRQ", "NZL", "JPN",
            ] {
                let c = atlas.countries.iter().position(|x| x.code == code).unwrap();
                let core = projection.core(c);
                line += &format!(" {code}={:.0}", core.width.max(core.height) * t.scale);
            }
            println!("{line}");
        }
    }
}

//! Carte interactive : projection des contours, caméra (zoom / déplacement), rendu et clics.

use geo_core::projection::{project, project_ring, project_view};
use geo_core::{Mark, Quiz, Shape, World};
use iced::alignment::Vertical;
use iced::mouse::{self, ScrollDelta};
use iced::widget::canvas::fill::{Fill, Rule};
use iced::widget::canvas::path::Builder;
use iced::widget::canvas::{self, Action, Cache, Event, Frame, Geometry, Path, Stroke, Text};
use iced::widget::text::Alignment;
use iced::{Color, Pixels, Point, Rectangle, Renderer, Size, Theme, Vector};

use crate::style;

/// En dessous de cette taille à l'écran (en pixels), un pays est signalé par une pastille.
const TINY_PX: f32 = 7.0;
const MARKER_RADIUS: f32 = 4.5;
/// Au-delà de cette échelle (pixels par unité projetée), on dessine les contours détaillés.
const DETAIL_SCALE: f32 = 650.0;
pub const MIN_ZOOM: f32 = 0.6;
pub const MAX_ZOOM: f32 = 600.0;

/// Contours projetés pour un méridien central donné.
pub struct Projection {
    countries: Vec<ProjShape>,
    territories: Vec<ProjShape>,
}

struct ProjShape {
    hi: Vec<Vec<Point>>,
    lo: Vec<Vec<Point>>,
    /// Boîte englobante de tous les anneaux (coordonnées projetées, y vers le haut).
    bbox: Rectangle,
    /// Boîte englobante du plus grand anneau : mesure la taille « visible » du pays.
    core: Rectangle,
    label: Point,
}

impl Projection {
    pub fn new(world: &World, center: f32) -> Self {
        let countries = world
            .countries
            .iter()
            .map(|c| ProjShape::new(&c.shape, Some(c.label), center))
            .collect();
        let territories = world
            .territories
            .iter()
            .map(|t| ProjShape::new(&t.shape, Some(t.label), center))
            .collect();
        Self {
            countries,
            territories,
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
    fn new(shape: &Shape, label: Option<[f32; 2]>, center: f32) -> Self {
        let mut buf = Vec::new();
        let mut convert = |rings: &[Vec<[f32; 2]>]| -> Vec<Vec<Point>> {
            rings
                .iter()
                .map(|r| {
                    project_ring(r, center, &mut buf);
                    buf.iter().map(|&[x, y]| Point::new(x, y)).collect()
                })
                .collect()
        };
        let hi = convert(&shape.rings);
        let lo = convert(&shape.rings_lo);
        let bbox = bounds(hi.iter().flatten());
        let core = hi
            .iter()
            .map(|r| bounds(r.iter()))
            .max_by(|a, b| a.area().total_cmp(&b.area()))
            .unwrap_or(bbox);
        let label = match label {
            Some([lon, lat]) => {
                let [x, y] = project(lon, lat, center);
                Point::new(x, y)
            }
            None => bbox.center(),
        };
        Self {
            hi,
            lo,
            bbox,
            core,
            label,
        }
    }
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

    /// Caméra centrée sur une boîte. Le zoom actuel est conservé s'il convient ; sinon il est
    /// ajusté pour que la boîte mesure au moins `min_px` pixels (sans dépasser `max_zoom`)
    /// et tienne dans l'écran.
    pub fn focus(
        self,
        target: Rectangle,
        view: Rectangle,
        size: Size,
        min_px: f32,
        max_zoom: f32,
    ) -> Camera {
        let base = Transform::new(Camera { zoom: 1.0, ..self }, view, size).scale;
        let extent = target.width.max(target.height).max(1e-6) * base;
        let smallest = (min_px / extent).min(max_zoom);
        let largest = (0.6 * size.width.min(size.height) / extent).max(smallest);
        Camera {
            center: target.center(),
            zoom: self.zoom.clamp(smallest, largest),
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

/// Passage des coordonnées projetées aux pixels du canvas.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    center: Point,
    pub scale: f32,
    half: Vector,
}

impl Transform {
    pub fn new(camera: Camera, view: Rectangle, size: Size) -> Self {
        let fit =
            (size.width / view.width.max(1e-6)).min(size.height / view.height.max(1e-6)) * 0.94;
        Self {
            center: camera.center,
            scale: fit * camera.zoom,
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

    /// La boîte (projetée) est-elle entièrement visible, avec une marge en pixels ?
    pub fn shows(&self, r: Rectangle, margin: f32) -> bool {
        let a = self.to_screen(Point::new(r.x, r.y + r.height));
        let b = self.to_screen(Point::new(r.x + r.width, r.y));
        a.x >= margin
            && a.y >= margin
            && b.x <= self.half.x * 2.0 - margin
            && b.y <= self.half.y * 2.0 - margin
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
    pub cache: &'a Cache,
}

#[derive(Default)]
pub struct Interaction {
    drag: Option<Drag>,
}

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
}

impl Tint {
    const ALL: [Tint; 6] = [
        Tint::Outside,
        Tint::Pending,
        Tint::Perfect,
        Tint::Second,
        Tint::Third,
        Tint::Missed,
    ];

    fn color(self) -> Color {
        match self {
            Tint::Outside => style::LAND_OUTSIDE,
            Tint::Pending => style::LAND,
            Tint::Perfect => style::SUCCESS,
            Tint::Second => style::WARNING,
            Tint::Third => style::ORANGE,
            Tint::Missed => style::DANGER,
        }
    }
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
    fn rings(&self, country: usize, t: &Transform) -> &Vec<Vec<Point>> {
        let shape = &self.projection.countries[country];
        if t.scale >= DETAIL_SCALE {
            &shape.hi
        } else {
            &shape.lo
        }
    }

    fn is_tiny(&self, country: usize, t: &Transform) -> bool {
        let core = self.projection.countries[country].core;
        core.width.max(core.height) * t.scale < TINY_PX
    }

    /// Le nom d'un pays peut-il être affiché ?
    pub fn reveals(&self, country: usize) -> bool {
        self.reveal_all || !matches!(self.quiz.mark(country), Some(Mark::Pending))
    }

    fn hit(&self, t: &Transform, screen: Point) -> Option<Place> {
        for &c in self.quiz.members() {
            if self.is_tiny(c, t)
                && t.to_screen(self.projection.label(c)).distance(screen) <= MARKER_RADIUS + 5.0
            {
                return Some(Place::Country(c));
            }
        }
        let p = t.to_world(screen);
        let inside = |shape: &ProjShape| shape.bbox.contains(p) && contains(&shape.hi, p);
        self.projection
            .countries
            .iter()
            .position(inside)
            .map(Place::Country)
            .or_else(|| {
                self.projection
                    .territories
                    .iter()
                    .position(inside)
                    .map(Place::Territory)
            })
    }

    /// Couleur d'un territoire : celle de son pays une fois celui-ci joué, un peu assombrie.
    fn territory_color(&self, territory: usize) -> Color {
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
            Place::Country(c) => self
                .reveals(c)
                .then(|| self.world.countries[c].name.clone()),
            Place::Territory(i) => {
                let territory = &self.world.territories[i];
                Some(match territory.owner {
                    Some(owner) if self.reveals(owner) => {
                        format!("{} · {}", territory.name, self.world.countries[owner].name)
                    }
                    _ => territory.name.clone(),
                })
            }
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

        // Territoires, regroupés par couleur.
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
            Self::add_rings(&mut coast, rings, t);
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

        let mut buckets: Vec<(Tint, Builder)> =
            Tint::ALL.iter().map(|&k| (k, Builder::new())).collect();
        for (i, shape) in self.projection.countries.iter().enumerate() {
            if !shape.bbox.intersects(&visible) {
                continue;
            }
            let rings = self.rings(i, t);
            let k = tint(self.quiz.mark(i));
            let bucket = &mut buckets.iter_mut().find(|(b, _)| *b == k).expect("teinte").1;
            Self::add_rings(bucket, rings, t);
            Self::add_rings(&mut coast, rings, t);
        }
        for (k, builder) in buckets {
            frame.fill(
                &builder.build(),
                Fill {
                    style: k.color().into(),
                    rule: Rule::EvenOdd,
                },
            );
        }
        frame.stroke(
            &coast.build(),
            Stroke::default().with_color(style::COAST).with_width(0.8),
        );

        // Pastilles des micro-États.
        for &c in self.quiz.members() {
            if !self.is_tiny(c, t) || !visible.contains(self.projection.label(c)) {
                continue;
            }
            let center = t.to_screen(self.projection.label(c));
            let color = mark_color(self.quiz.mark(c));
            let dot = Path::circle(center, MARKER_RADIUS);
            frame.fill(&dot, style::lighten(color, 0.15));
            frame.stroke(
                &dot,
                Stroke::default().with_color(style::COAST).with_width(1.2),
            );
        }

        // Noms des pays déjà joués, quand ils tiennent dans leur contour.
        for &c in self.quiz.members() {
            if !matches!(self.quiz.mark(c), Some(Mark::Found { .. } | Mark::Missed)) {
                continue;
            }
            let core = self.projection.countries[c].core;
            let name = &self.world.countries[c].name;
            let size = if t.scale > 2000.0 { 13.0 } else { 11.0 };
            let width = name.chars().count() as f32 * size * 0.56;
            if core.width * t.scale < width * 0.9 || core.height * t.scale < size + 2.0 {
                continue;
            }
            let at = self.projection.label(c);
            if !visible.contains(at) {
                continue;
            }
            let position = t.to_screen(at);
            let text = |color: Color, offset: Vector| Text {
                content: name.clone(),
                position: position + offset,
                color,
                size: Pixels(size),
                font: style::FONT_SEMIBOLD,
                align_x: Alignment::Center,
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

    fn draw_overlay(&self, frame: &mut Frame, t: &Transform, cursor: Option<Point>) {
        if let Some(target) = self.target {
            let pulse = 0.5 + 0.5 * (self.pulse * std::f32::consts::TAU).sin();
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
            let core = self.projection.core(target);
            if core.width.max(core.height) * t.scale < 24.0 {
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
            let center = t.to_screen(self.projection.label(flash.country));
            if self.is_tiny(flash.country, t) {
                frame.stroke(
                    &Path::circle(center, 12.0),
                    Stroke::default()
                        .with_color(style::with_alpha(flash.color, flash.strength))
                        .with_width(3.0),
                );
            }
            let name = &self.world.countries[flash.country].name;
            tag(
                frame,
                name,
                Point::new(center.x, center.y - 22.0),
                flash.color,
                flash.strength,
            );
        }

        if let Some(place) = self.hover {
            let mut b = Builder::new();
            Self::add_rings(&mut b, self.place_rings(place, t), t);
            frame.stroke(
                &b.build(),
                Stroke::default()
                    .with_color(style::with_alpha(Color::WHITE, 0.85))
                    .with_width(1.6),
            );
            if let Place::Country(hover) = place
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
                    return Some(Action::capture());
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
    fn focus_zooms_in_on_tiny_targets_and_out_on_large_ones() {
        let view = rect(-2.0, -1.0, 4.0, 2.0);
        let size = Size::new(800.0, 600.0);
        let tiny = rect(0.5, 0.5, 0.001, 0.001);
        let zoomed = Camera::fit(view).focus(tiny, view, size, 30.0, 10.0);
        assert_eq!(zoomed.zoom, 10.0);
        assert_eq!(zoomed.center, tiny.center());

        let large = rect(-1.0, -0.5, 1.5, 0.8);
        let back = zoomed.focus(large, view, size, 30.0, 10.0);
        let t = Transform::new(back, view, size);
        assert!(
            t.shows(large, 0.0),
            "le grand pays doit tenir à l'écran (zoom {})",
            back.zoom
        );
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

//! Carte de situation d'un encart (outre-mer, Alaska, Hawaï) : le monde entier, puis un zoom
//! animé sur ces terres, dessinées à leur vraie place. On peut ensuite déplacer la carte et
//! zoomer à la molette.

use std::time::{Duration, Instant};

use geo_core::{Mode, Quiz, World};
use iced::widget::canvas::Cache;
use iced::widget::{button, canvas, column, container, row, text};
use iced::{Center, Element, Fill, Point, Rectangle, Size, Theme, Vector};

use super::Message;
use crate::ATLASES;
use crate::icons::{Icon, icon};
use crate::map::{self, Camera, MapEvent, MapView, Projection, Situation};
use crate::style;

/// Taille de la carte dans la vignette, en pixels.
const MAP_SIZE: Size = Size::new(420.0, 270.0);
/// Pause sur le monde entier avant de zoomer, puis durée du zoom.
const DELAY: Duration = Duration::from_millis(400);
const FLIGHT: f32 = 1.6;

pub struct Locator {
    /// Encart montré (index dans `info.insets`).
    pub inset: usize,
    projection: Projection,
    /// Le monde entier, centré sur ces terres (zoom 1).
    view: Rectangle,
    camera: Camera,
    /// Cadrage sur ces terres, assez serré pour bien voir une petite île.
    focus: Camera,
    flight: Option<Flight>,
    situation: Situation,
    /// Partie vide : aucun pays mis en évidence ni nommé (pas d'indice).
    quiz: Quiz,
    cache: Cache,
}

struct Flight {
    from: Camera,
    to: Camera,
    start: Instant,
}

impl Locator {
    pub fn new(atlas: &World, inset: usize) -> Option<Self> {
        let frame = atlas.info.insets.get(inset)?;
        let world = &ATLASES[0];
        let lon = (frame.origin[0] + frame.origin[2]) / 2.0;
        let view = map::view_rect([lon - 180.0, -58.0, lon + 180.0, 84.0], lon);
        let situation = Situation::new(atlas, frame, lon);
        let b = situation.bbox;
        // Marge proportionnelle à la plus grande dimension, avec un minimum pour les petites
        // îles (environ 7° sur 5°).
        let margin = 1.2 * b.width.max(b.height);
        let (w, h) = ((b.width + margin).max(0.05), (b.height + margin).max(0.035));
        let area = Rectangle::new(
            Point::new(b.center_x() - w / 2.0, b.center_y() - h / 2.0),
            Size::new(w, h),
        );
        let focus = Camera::frame(area, view, MAP_SIZE);
        let camera = Camera::fit(view);
        Some(Self {
            inset,
            projection: Projection::new(world, lon),
            view,
            camera,
            focus,
            flight: Some(Flight {
                from: camera,
                to: focus,
                start: Instant::now() + DELAY,
            }),
            situation,
            quiz: Quiz::new(Mode::Free, world.countries.len(), Vec::new(), Vec::new()),
            cache: Cache::new(),
        })
    }

    /// Développement : fige l'animation à l'instant `t` (0 à 1), pour une capture.
    #[cfg(debug_assertions)]
    pub fn freeze(&mut self, t: f32) {
        if let Some(flight) = self.flight.take() {
            self.camera = fly(flight.from, flight.to, self.situation.bbox.center(), t);
        }
    }

    pub fn animating(&self) -> bool {
        self.flight.is_some()
    }

    pub fn tick(&mut self, now: Instant) {
        let Some(flight) = &self.flight else { return };
        let t = (now.saturating_duration_since(flight.start).as_secs_f32() / FLIGHT).min(1.0);
        // Accélération puis décélération.
        let eased = if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (2.0 - 2.0 * t).powi(3) / 2.0
        };
        self.camera = fly(flight.from, flight.to, self.situation.bbox.center(), eased);
        self.cache.clear();
        if t >= 1.0 {
            self.flight = None;
        }
    }

    /// Déplacement ou zoom à la souris : l'animation s'arrête.
    pub fn on_map(&mut self, event: MapEvent) {
        if let MapEvent::Camera(camera) = event {
            self.camera = camera;
            self.flight = None;
            self.cache.clear();
        }
    }

    /// Revient sur ces terres, en douceur.
    pub fn recenter(&mut self) {
        self.flight = Some(Flight {
            from: self.camera,
            to: self.focus,
            start: Instant::now(),
        });
    }

    pub fn view(&self) -> Element<'_, Message> {
        let map_view = MapView {
            world: &ATLASES[0],
            projection: &self.projection,
            view: self.view,
            camera: self.camera,
            quiz: &self.quiz,
            hover: None,
            target: None,
            flash: None,
            pulse: 0.0,
            reveal_all: false,
            clickable: false,
            preview: false,
            situation: Some(&self.situation),
            explore: false,
            emphasis: &[],
            cache: &self.cache,
        };
        let map: Element<'_, MapEvent> = canvas(map_view)
            .width(MAP_SIZE.width)
            .height(MAP_SIZE.height)
            .into();
        let control = |i: Icon, message: Message| {
            button(container(icon(i, 13.0, style::TEXT)).center(Fill))
                .width(28)
                .height(28)
                .padding(0)
                .style(style::map_control)
                .on_press(message)
        };
        let card = column![
            row![
                icon(Icon::Globe, 16.0, style::TEXT),
                text("Sur la carte du monde")
                    .size(14)
                    .font(style::FONT_SEMIBOLD)
                    .width(Fill),
                control(Icon::Target, Message::LocatorRecenter),
                control(Icon::Close, Message::CloseLocator),
            ]
            .spacing(8)
            .align_y(Center),
            container(map.map(Message::LocatorMap))
                .clip(true)
                .style(|_: &Theme| container::Style {
                    background: Some(style::OCEAN.into()),
                    border: iced::Border {
                        color: style::BORDER,
                        width: 1.0,
                        radius: 10.0.into(),
                    },
                    ..Default::default()
                }),
            text("Molette pour zoomer, glisser pour déplacer.")
                .size(12)
                .color(style::MUTED),
        ]
        .spacing(10);
        container(card)
            .padding(14)
            .width(MAP_SIZE.width + 28.0)
            .style(|theme: &Theme| container::Style {
                background: Some(style::SURFACE.into()),
                ..style::floating(theme)
            })
            .into()
    }
}

/// Caméra à l'instant `t` (0 à 1) d'un zoom de `from` vers `to` qui garde `anchor` à l'écran :
/// le zoom progresse de façon géométrique et la position de `anchor` à l'écran glisse
/// linéairement de son point de départ à son point d'arrivée. Interpoler le centre de la carte
/// à vitesse constante ferait sortir `anchor` du cadre pendant le zoom.
pub(super) fn fly(from: Camera, to: Camera, anchor: Point, t: f32) -> Camera {
    let zoom = (from.zoom.ln() + (to.zoom.ln() - from.zoom.ln()) * t).exp();
    // Écart entre `anchor` et le centre de l'écran, en unités proportionnelles aux pixels.
    let offset = |c: Camera| {
        Vector::new(
            (anchor.x - c.center.x) * c.zoom,
            (anchor.y - c.center.y) * c.zoom,
        )
    };
    let (a, b) = (offset(from), offset(to));
    let d = a + (b - a) * t;
    Camera {
        center: Point::new(anchor.x - d.x / zoom, anchor.y - d.y / zoom),
        zoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pendant tout le zoom, le point visé reste entre sa position de départ et le centre :
    /// il ne sort jamais du cadre.
    #[test]
    fn flight_keeps_the_anchor_in_view() {
        let anchor = Point::new(0.9, -0.4);
        let from = Camera {
            center: Point::new(0.0, 0.2),
            zoom: 1.0,
        };
        let to = Camera {
            center: anchor,
            zoom: 80.0,
        };
        let start = (anchor.y - from.center.y) * from.zoom;
        for i in 0..=20 {
            let c = fly(from, to, anchor, i as f32 / 20.0);
            let offset = (anchor.y - c.center.y) * c.zoom;
            assert!(offset.abs() <= start.abs() + 1e-4, "{i} : {offset}");
        }
        let end = fly(from, to, anchor, 1.0);
        assert!((end.zoom - 80.0).abs() < 1e-3 && end.center.distance(anchor) < 1e-5);
    }
}

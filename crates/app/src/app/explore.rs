//! Exploration de la carte, sans quiz : tous les lieux de la zone nommés (numérotés pour les
//! numéros des départements), une liste à côté pour les situer, avant de se tester.

use std::time::Instant;

use geo_core::{Mode, Quiz, RegionKind, World};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::Cache;
use iced::widget::{
    Column, button, canvas, column, container, mouse_area, opaque, row, scrollable, sensor, stack,
    text,
};
use iced::{Center, Element, Fill, Point, Rectangle, Size, Theme};

use super::locator::{Locator, fly};
use super::{Message, section_title};
use crate::ATLASES;
use crate::icons::{Icon, icon};
use crate::map::{self, Camera, MapEvent, MapView, Place, Projection};
use crate::style;

/// Durée du déplacement vers un lieu choisi dans la liste.
const FLIGHT: f32 = 0.6;

pub struct Explorer {
    world: &'static World,
    region: usize,
    projection: Projection,
    view: Rectangle,
    camera: Camera,
    flight: Option<Flight>,
    map_size: Size,
    hover: Option<Place>,
    selected: Option<usize>,
    /// Partie fictive dont les lieux « à trouver » sont ceux de la zone (mis en évidence).
    quiz: Quiz,
    cache: Cache,
    pub locator: Option<Locator>,
}

struct Flight {
    from: Camera,
    to: Camera,
    anchor: Point,
    start: Instant,
}

impl Explorer {
    pub fn new(atlas: usize, region: usize) -> Self {
        let world = &ATLASES[atlas];
        let zone = &world.regions[region];
        let view = map::view_rect(zone.view, zone.center);
        Self {
            world,
            region,
            projection: Projection::new(world, zone.center),
            view,
            camera: Camera::fit(view),
            flight: None,
            map_size: Size::ZERO,
            hover: None,
            selected: None,
            quiz: Quiz::new(
                Mode::Free,
                world.countries.len(),
                zone.members.clone(),
                Vec::new(),
            ),
            cache: Cache::new(),
            locator: None,
        }
    }

    pub fn animating(&self) -> bool {
        self.flight.is_some() || self.locator.as_ref().is_some_and(Locator::animating)
    }

    pub fn tick(&mut self, now: Instant) {
        if let Some(locator) = &mut self.locator {
            locator.tick(now);
        }
        let Some(flight) = &self.flight else { return };
        let t = (now.saturating_duration_since(flight.start).as_secs_f32() / FLIGHT).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        self.camera = fly(flight.from, flight.to, flight.anchor, eased);
        self.cache.clear();
        if t >= 1.0 {
            self.flight = None;
        }
    }

    pub fn on_map(&mut self, event: MapEvent) {
        match event {
            MapEvent::Camera(camera) => {
                self.camera = camera;
                self.flight = None;
                self.cache.clear();
            }
            MapEvent::Hover(hover) => self.hover = hover,
            MapEvent::Resized(size) => {
                self.map_size = size;
                self.cache.clear();
            }
            MapEvent::Click(Some(country)) => self.selected = Some(country),
            MapEvent::Click(None) => self.selected = None,
            MapEvent::Globe(inset) => {
                self.locator = match &self.locator {
                    Some(l) if l.inset == inset => None,
                    _ => Locator::new(self.world, inset),
                };
            }
            MapEvent::DragEnded => {}
        }
    }

    /// Survol d'un nom dans la liste : le lieu est souligné sur la carte.
    /// Lieu de l'atlas désigné par son code, s'il existe.
    #[cfg(debug_assertions)]
    pub fn find(&self, code: &str) -> Option<usize> {
        self.world.countries.iter().position(|c| c.code == code)
    }

    pub fn hover_list(&mut self, country: Option<usize>) {
        self.hover = country.map(Place::Country);
    }

    /// Choix d'un nom dans la liste : la carte se déplace vers ce lieu.
    pub fn select(&mut self, country: usize) {
        self.selected = Some(country);
        if self.map_size == Size::ZERO {
            return;
        }
        let core = self.projection.core(country);
        let (w, h) = (
            (core.width * 4.0).max(self.view.width * 0.18),
            (core.height * 4.0).max(self.view.height * 0.18),
        );
        let area = Rectangle::new(
            Point::new(core.center_x() - w / 2.0, core.center_y() - h / 2.0),
            Size::new(w, h),
        );
        let mut to = Camera::frame(area, self.view, self.map_size);
        // Jamais plus loin que la vue d'ensemble.
        if to.zoom < 1.0 {
            to = Camera::fit(self.view);
        }
        self.flight = Some(Flight {
            from: self.camera,
            to,
            anchor: core.center(),
            start: Instant::now(),
        });
    }

    pub fn zoom(&mut self, factor: f32) {
        self.flight = None;
        self.camera = self.camera.zoom_at(self.view, self.map_size, factor, None);
        self.cache.clear();
    }

    pub fn reset(&mut self) {
        self.flight = None;
        self.camera = Camera::fit(self.view);
        self.cache.clear();
    }

    /// Nom affiché d'un lieu, précédé de son numéro s'il en a un (« 75 · Paris »).
    fn label(&self, country: usize) -> String {
        let c = &self.world.countries[country];
        match crate::number_of(c) {
            Some(number) => format!("{number} · {}", c.name),
            None => c.name.clone(),
        }
    }

    /// Groupe d'un lieu dans la liste : sa sous-région, ou son continent si les sous-régions
    /// sont masquées (régions françaises, Canada…).
    fn group_of(&self, country: usize) -> usize {
        let sub = self.world.countries[country].sub;
        match self.world.regions[sub] {
            ref r if r.hidden => r.parent.unwrap_or(sub),
            _ => sub,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let zone = &self.world.regions[self.region];
        let info = &self.world.info;
        let top = container(
            row![
                button(
                    row![
                        icon(Icon::Back, 16.0, style::TEXT),
                        text(info.title.clone()).size(14)
                    ]
                    .spacing(8)
                    .align_y(Center)
                )
                .padding([8, 12])
                .style(style::ghost)
                .on_press(Message::CloseExplore),
                column![
                    text(format!("Explorer · {}", zone.name))
                        .size(16)
                        .font(style::FONT_SEMIBOLD),
                    text(format!("{} {}", zone.members.len(), info.many))
                        .size(12)
                        .color(style::MUTED),
                ]
                .width(Fill),
                button(
                    row![
                        icon(Icon::Play, 14.0, iced::Color::WHITE),
                        text("Commencer le quiz")
                            .size(14)
                            .font(style::FONT_SEMIBOLD)
                    ]
                    .spacing(8)
                    .align_y(Center)
                )
                .padding([9, 18])
                .style(style::primary)
                .on_press(Message::Start),
            ]
            .spacing(16)
            .align_y(Center),
        )
        .padding([10, 16])
        .style(style::top_bar);
        column![top, row![self.map(), self.panel()]].into()
    }

    fn map(&self) -> Element<'_, Message> {
        let map_view = MapView {
            world: self.world,
            projection: &self.projection,
            view: self.view,
            camera: self.camera,
            quiz: &self.quiz,
            hover: self.hover,
            target: self.selected,
            flash: None,
            pulse: 0.25,
            reveal_all: true,
            clickable: true,
            preview: false,
            situation: None,
            explore: true,
            cache: &self.cache,
        };
        let map: Element<'_, MapEvent> = canvas(map_view).width(Fill).height(Fill).into();
        let map = sensor(
            container(map.map(Message::ExploreMap))
                .width(Fill)
                .height(Fill)
                .style(style::map_frame),
        )
        .on_show(|size| Message::ExploreMap(MapEvent::Resized(size)))
        .on_resize(|size| Message::ExploreMap(MapEvent::Resized(size)));
        let control = |i: Icon, message: Message| {
            button(container(icon(i, 18.0, style::TEXT)).center(Fill))
                .width(40)
                .height(40)
                .padding(0)
                .style(style::map_control)
                .on_press(message)
        };
        let controls = container(
            column![
                control(Icon::Plus, Message::ExploreZoom(true)),
                control(Icon::Minus, Message::ExploreZoom(false)),
                control(Icon::Target, Message::ExploreReset),
            ]
            .spacing(8),
        )
        .width(Fill)
        .height(Fill)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Bottom)
        .padding(16);
        let mut layers = stack![map, controls];
        if let Some(locator) = &self.locator {
            layers = layers.push(
                container(opaque(locator.view()))
                    .width(Fill)
                    .align_x(Horizontal::Right)
                    .padding(16),
            );
        }
        layers.width(Fill).height(Fill).into()
    }

    /// Liste des lieux de la zone, par sous-région ; le lieu choisi est détaillé en haut.
    fn panel(&self) -> Element<'_, Message> {
        let zone = &self.world.regions[self.region];
        let mut body = Column::new().spacing(16);
        body = body.push(
            text("Survolez la carte ou cliquez un nom pour le situer.")
                .size(13)
                .color(style::MUTED),
        );
        if let Some(c) = self.selected {
            let group = &self.world.regions[self.group_of(c)];
            body = body.push(
                container(
                    column![
                        text(self.label(c)).size(20).font(style::FONT_BOLD),
                        text(group.name.clone()).size(13).color(style::MUTED),
                    ]
                    .spacing(4),
                )
                .padding([12, 14])
                .width(Fill)
                .style(|_: &Theme| container::Style {
                    background: Some(style::ACCENT_SOFT.into()),
                    border: iced::Border {
                        color: style::ACCENT,
                        width: 1.0,
                        radius: 12.0.into(),
                    },
                    ..Default::default()
                }),
            );
        }

        // Groupes dans l'ordre de l'atlas ; lieux par numéro s'ils en ont, sinon par nom.
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
        for &c in &zone.members {
            let g = self.group_of(c);
            match groups.iter_mut().find(|(id, _)| *id == g) {
                Some((_, list)) => list.push(c),
                None => groups.push((g, vec![c])),
            }
        }
        groups.sort_by_key(|(g, _)| *g);
        let numbered = self
            .world
            .countries
            .first()
            .is_some_and(|c| crate::number_of(c).is_some());
        let single = groups.len() == 1
            || matches!(zone.kind, RegionKind::Sub)
            || groups.iter().all(|(g, _)| *g == self.region);
        for (g, mut list) in groups {
            if !numbered {
                list.sort_by_key(|&c| geo_core::normalize(&self.world.countries[c].name));
            }
            let mut group = Column::new().spacing(2);
            if !single {
                group = group.push(section_title(&self.world.regions[g].name));
            }
            for c in list {
                let selected = self.selected == Some(c);
                let item = button(text(self.label(c)).size(14))
                    .padding([5, 10])
                    .width(Fill)
                    .style(move |theme: &Theme, status| {
                        if selected {
                            style::pill(true)(theme, status)
                        } else {
                            style::ghost(theme, status)
                        }
                    })
                    .on_press(Message::ExploreSelect(c));
                group = group.push(
                    mouse_area(item)
                        .on_enter(Message::ExploreHover(Some(c)))
                        .on_exit(Message::ExploreHover(None)),
                );
            }
            body = body.push(group);
        }
        container(scrollable(container(body).padding(18)).height(Fill))
            .width(320)
            .height(Fill)
            .style(style::sidebar)
            .into()
    }
}

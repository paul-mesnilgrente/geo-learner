//! Exploration de la carte, sans quiz : tous les lieux de la zone nommés (numérotés pour les
//! numéros des départements), une liste à côté pour les situer, avant de se tester.

use std::time::Instant;

use geo_core::{Mode, Quiz, RegionKind, World};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::Cache;
use iced::widget::{
    Column, Row, button, canvas, column, container, mouse_area, opaque, row, scrollable, sensor,
    stack, text, tooltip,
};
use iced::{Center, Element, Fill, Point, Rectangle, Size, Theme};

use super::locator::{Locator, fly};
use super::{Message, section_title};
use crate::i18n::Lang;
use crate::icons::{Icon, icon};
use crate::map::{self, Camera, MapEvent, MapView, Place, Projection};
use crate::progress::Level;
use crate::style;
use crate::{ATLASES, tr};

/// Durée du déplacement vers un lieu choisi dans la liste.
const FLIGHT: f32 = 0.6;

/// Un repère par dizaine : les numéros suivent l'ordre alphabétique des noms d'origine.
const ANCHORS: [&str; 10] = ["01", "10", "21", "30", "40", "50", "60", "70", "80", "90"];

pub struct Explorer {
    lang: Lang,
    world: &'static World,
    region: usize,
    projection: Projection,
    view: Rectangle,
    camera: Camera,
    flight: Option<Flight>,
    map_size: Size,
    hover: Option<Place>,
    selected: Option<usize>,
    /// Drapeau du lieu choisi montré en grand, par-dessus l'exploration.
    pub big_flag: bool,
    /// Partie fictive dont les lieux « à trouver » sont ceux de la zone (mis en évidence).
    quiz: Quiz,
    cache: Cache,
    pub locator: Option<Locator>,
    /// Carte de maîtrise reprise de l'aperçu : niveau de chaque lieu.
    mastery: Option<Vec<Option<Level>>>,
    /// Départements repères (un par dizaine), s'il s'agit des départements.
    anchors: Vec<usize>,
    show_anchors: bool,
}

struct Flight {
    from: Camera,
    to: Camera,
    anchor: Point,
    start: Instant,
}

impl Explorer {
    pub fn new(
        atlas: usize,
        region: usize,
        lang: Lang,
        mastery: Option<Vec<Option<Level>>>,
    ) -> Self {
        let world = &ATLASES[atlas];
        let zone = &world.regions[region];
        let view = map::view_rect(zone.view, zone.center);
        Self {
            lang,
            world,
            region,
            projection: Projection::new(world, zone.center),
            view,
            camera: Camera::fit(view),
            flight: None,
            map_size: Size::ZERO,
            hover: None,
            selected: None,
            big_flag: false,
            quiz: Quiz::new(
                Mode::Free,
                world.countries.len(),
                zone.members.clone(),
                Vec::new(),
            ),
            cache: Cache::new(),
            mastery,
            locator: None,
            anchors: ANCHORS
                .iter()
                .filter_map(|&n| {
                    world
                        .countries
                        .iter()
                        .position(|c| crate::number_of(c).as_deref() == Some(n))
                })
                .collect(),
            show_anchors: true,
        }
    }

    /// Repères des dizaines affichés ou non sur la carte.
    pub fn toggle_anchors(&mut self) {
        self.show_anchors = !self.show_anchors;
        self.cache.clear();
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
            MapEvent::Click(selected) => {
                self.selected = selected;
                // Océans : le choix se voit dans le fond de carte, à redessiner.
                if self.world.info.water {
                    self.cache.clear();
                }
            }
            MapEvent::Globe(inset) => {
                self.locator = match &self.locator {
                    Some(l) if l.inset == inset => None,
                    _ => Locator::new(self.world, inset, self.lang),
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
        if self.world.info.water {
            self.cache.clear();
        }
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
            Some(number) => format!("{number} · {}", c.name.get(self.lang)),
            None => c.name.get(self.lang).to_owned(),
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
                        text(info.title.get(self.lang)).size(14)
                    ]
                    .spacing(8)
                    .align_y(Center)
                )
                .padding([8, 12])
                .style(style::ghost)
                .on_press(Message::CloseExplore),
                column![
                    text(tr!(
                        self.lang,
                        "Explorer · {}",
                        "Explore · {}",
                        zone.name.get(self.lang)
                    ))
                    .size(16)
                    .font(style::FONT_SEMIBOLD),
                    text(format!(
                        "{} {}",
                        zone.members.len(),
                        info.many.get(self.lang)
                    ))
                    .size(12)
                    .color(style::MUTED),
                ]
                .width(Fill),
                button(
                    row![
                        icon(Icon::Play, 14.0, iced::Color::WHITE),
                        text(tr!(self.lang, "Commencer le quiz", "Start the quiz"))
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
        let page = column![top, row![self.map(), self.panel()]];
        match self.big_flag().filter(|_| self.big_flag) {
            Some((c, flag)) => stack![page, self.flag_viewer(c, flag)].into(),
            None => page.into(),
        }
    }

    /// Drapeau du lieu choisi, s'il en a un (pays du monde, quiz des drapeaux).
    fn big_flag(&self) -> Option<(usize, usize)> {
        let c = self.selected?;
        (self.world.info.id == "monde" || self.world.info.flags)
            .then(|| crate::flags::find(&self.world.countries[c].code))
            .flatten()
            .map(|flag| (c, flag))
    }

    /// Le drapeau en grand ; un clic n'importe où (ou Échap) le referme.
    fn flag_viewer(&self, c: usize, flag: usize) -> Element<'_, Message> {
        let group = &self.world.regions[self.group_of(c)];
        crate::flags::viewer(
            flag,
            self.label(c),
            group.name.get(self.lang).to_owned(),
            Message::ExploreFlag(false),
        )
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
            emphasis: if self.show_anchors {
                &self.anchors
            } else {
                &[]
            },
            mastery: self.mastery.as_deref(),
            labels: crate::settings::MapLabels::None,
            lang: self.lang,
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
    /// Légende de la carte de maîtrise (mêmes couleurs que la carte « À revoir »).
    fn mastery_legend(&self) -> Element<'_, Message> {
        let lang = self.lang;
        let entry = |label: String, color: iced::Color| -> Element<'_, Message> {
            row![
                container(iced::widget::space())
                    .width(8)
                    .height(8)
                    .style(move |_: &Theme| container::Style {
                        background: Some(color.into()),
                        border: iced::border::rounded(4),
                        ..container::Style::default()
                    }),
                text(label).size(12).color(style::MUTED),
            ]
            .spacing(6)
            .align_y(Center)
            .into()
        };
        Row::with_children([
            entry(tr!(lang, "acquis", "known"), style::SUCCESS),
            entry(tr!(lang, "en cours", "learning"), style::WARNING),
            entry(tr!(lang, "à revoir", "to review"), style::ORANGE),
            entry(tr!(lang, "jamais joué", "not played yet"), style::LAND),
        ])
        .spacing(14)
        .wrap()
        .vertical_spacing(6)
        .into()
    }

    fn panel(&self) -> Element<'_, Message> {
        let zone = &self.world.regions[self.region];
        let mut body = Column::new().spacing(16);
        body = body.push(
            text(tr!(
                self.lang,
                "Survolez la carte ou cliquez un nom pour le situer.",
                "Hover over the map, or click a name to find it."
            ))
            .size(13)
            .color(style::MUTED),
        );
        if self.mastery.is_some() {
            body = body.push(self.mastery_legend());
        }
        if let Some(c) = self.selected {
            let group = &self.world.regions[self.group_of(c)];
            let names = column![
                text(self.label(c)).size(20).font(style::FONT_BOLD),
                text(group.name.get(self.lang)).size(13).color(style::MUTED),
            ]
            .spacing(4);
            // Pays du monde : son drapeau à côté de son nom ; un clic l'affiche en grand.
            let card: Element<'_, Message> = match self.big_flag() {
                Some((_, flag)) => {
                    let flag = tooltip(
                        mouse_area(crate::flags::view(flag, 72.0, 48.0))
                            .on_press(Message::ExploreFlag(true))
                            .interaction(iced::mouse::Interaction::Pointer),
                        container(
                            text(tr!(self.lang, "Agrandir le drapeau", "Enlarge the flag"))
                                .size(12),
                        )
                        .padding([4, 8])
                        .style(style::floating),
                        tooltip::Position::Bottom,
                    );
                    row![flag, names].spacing(14).align_y(Center).into()
                }
                None => names.into(),
            };
            body = body.push(
                container(card)
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

        if self.anchors.len() == ANCHORS.len() {
            body = body.push(self.anchors_tip());
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
        // Une seule liste pour une petite zone, ou une dizaine (dans l'ordre des numéros).
        let single = groups.len() == 1
            || matches!(zone.kind, RegionKind::Sub | RegionKind::Group)
            || groups.iter().all(|(g, _)| *g == self.region);
        if single {
            groups = vec![(self.region, zone.members.clone())];
        }
        for (g, mut list) in groups {
            if !numbered {
                list.sort_by_key(|&c| {
                    geo_core::normalize(self.world.countries[c].name.get(self.lang))
                });
            }
            let mut group = Column::new().spacing(2);
            if !single {
                group = group.push(section_title(self.world.regions[g].name.get(self.lang)));
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

    /// Astuce pour les numéros des départements : l'ordre alphabétique et ses repères.
    fn anchors_tip(&self) -> Element<'_, Message> {
        let warm = style::WARNING;
        let chips = self.anchors.iter().map(|&c| {
            button(text(self.label(c)).size(12))
                .padding([4, 10])
                .style(move |_: &Theme, status| {
                    let hovered =
                        matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: Some(
                            style::mix(style::SURFACE, warm, if hovered { 0.28 } else { 0.16 })
                                .into(),
                        ),
                        text_color: style::lighten(warm, 0.3),
                        border: iced::Border {
                            color: style::with_alpha(warm, 0.4),
                            width: 1.0,
                            radius: 999.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::ExploreSelect(c))
                .into()
        });
        container(
            column![
                row![
                    text(tr!(
                        self.lang,
                        "Astuce : l'ordre alphabétique",
                        "Tip: alphabetical order"
                    ))
                    .size(14)
                    .font(style::FONT_SEMIBOLD)
                    .width(Fill),
                    button(
                        text(if self.show_anchors {
                            tr!(self.lang, "Masquer", "Hide")
                        } else {
                            tr!(self.lang, "Montrer", "Show")
                        })
                        .size(12)
                    )
                    .padding([3, 8])
                    .style(style::ghost)
                    .on_press(Message::ExploreAnchors),
                ]
                .align_y(Center),
                text(tr!(
                    self.lang,
                    "Les numéros suivent l'ordre alphabétique des noms d'origine : 01 Ain, \
                     02 Aisne, 03 Allier… Retenez un repère par dizaine (en jaune sur la \
                     carte) : le reste se déduit à quelques unités près.",
                    "Numbers follow the alphabetical order of the original French names: \
                     01 Ain, 02 Aisne, 03 Allier… Remember one landmark per ten (in yellow on \
                     the map) and work out the rest to within a few."
                ))
                .size(13),
                Row::with_children(chips)
                    .spacing(6)
                    .wrap()
                    .vertical_spacing(6),
                text(tr!(
                    self.lang,
                    "Exceptions : « Haute- » se classe au nom principal (42 Loire, \
                     43 Haute-Loire). Noms changés depuis : 22 Côtes-d'Armor (Côtes-du-Nord), \
                     64 Pyrénées-Atlantiques (Basses-Pyrénées), 76 Seine-Maritime \
                     (Seine-Inférieure), 78 Yvelines (Seine-et-Oise). Les 91 à 95, autour de \
                     Paris, datent de 1968. La Corse (20) est devenue 2A et 2B en 1976, et \
                     l'outre-mer va de 971 à 976.",
                    "Exceptions: « Haute- » (upper) sorts under the main name (42 Loire, \
                     43 Haute-Loire). Renamed since: 22 Côtes-d'Armor (Côtes-du-Nord), \
                     64 Pyrénées-Atlantiques (Basses-Pyrénées), 76 Seine-Maritime \
                     (Seine-Inférieure), 78 Yvelines (Seine-et-Oise). 91 to 95, around Paris, \
                     date from 1968. Corsica (20) became 2A and 2B in 1976, and the overseas \
                     departments run from 971 to 976."
                ))
                .size(12)
                .color(style::MUTED),
            ]
            .spacing(10),
        )
        .padding([12, 14])
        .width(Fill)
        .style(|_: &Theme| container::Style {
            background: Some(style::SURFACE_2.into()),
            border: iced::Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    }
}

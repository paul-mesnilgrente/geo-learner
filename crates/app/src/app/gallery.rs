//! Galerie des drapeaux d'une zone, chacun avec le nom de son pays, par sous-région : pour les
//! apprendre avant de se tester. Un clic sur un drapeau l'affiche en grand.

use geo_core::{RegionKind, World};
use iced::widget::{
    Column, Row, button, column, container, row, scrollable, stack, text, text_input,
};
use iced::{Center, Element, Fill};

use super::{Message, section_title};
use crate::i18n::Lang;
use crate::icons::{Icon, icon};
use crate::style;
use crate::{ATLASES, tr};

/// Taille des drapeaux de la galerie (cadre fixe : les tuiles sont alignées quelles que soient
/// les proportions).
const FLAG_W: f32 = 132.0;
const FLAG_H: f32 = 84.0;
/// Largeur de la grille : sept tuiles et leurs espaces, plus les marges. Fixe, pour que la
/// recherche ne fasse pas bouger le champ en réduisant la liste.
const GRID_WIDTH: f32 = 7.0 * (FLAG_W + 28.0) + 6.0 * 12.0 + 2.0 * 28.0;

pub struct Gallery {
    lang: Lang,
    world: &'static World,
    region: usize,
    query: String,
    /// Pays dont le drapeau est montré en grand.
    pub big: Option<usize>,
}

impl Gallery {
    pub fn new(atlas: usize, region: usize, lang: Lang) -> Self {
        Self {
            lang,
            world: &ATLASES[atlas],
            region,
            query: String::new(),
            big: None,
        }
    }

    pub fn search(&mut self, query: String) {
        self.query = query;
    }

    /// Pays de l'atlas désigné par son code, s'il existe.
    #[cfg(debug_assertions)]
    pub fn find(&self, code: &str) -> Option<usize> {
        self.world.countries.iter().position(|c| c.code == code)
    }

    /// Groupe d'un pays : sa sous-région, ou son continent si les sous-régions sont masquées.
    fn group_of(&self, country: usize) -> usize {
        let sub = self.world.countries[country].sub;
        match self.world.regions[sub] {
            ref r if r.hidden => r.parent.unwrap_or(sub),
            _ => sub,
        }
    }

    /// Pays de la zone qui correspondent à la recherche (nom français ou anglais), par groupe.
    fn groups(&self) -> Vec<(usize, Vec<usize>)> {
        let zone = &self.world.regions[self.region];
        let query = geo_core::normalize(&self.query);
        let single = matches!(zone.kind, RegionKind::Sub | RegionKind::Group);
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
        for &c in &zone.members {
            let country = &self.world.countries[c];
            if !query.is_empty() && !country.answers.iter().any(|a| a.contains(&query)) {
                continue;
            }
            let g = if single {
                self.region
            } else {
                self.group_of(c)
            };
            match groups.iter_mut().find(|(id, _)| *id == g) {
                Some((_, list)) => list.push(c),
                None => groups.push((g, vec![c])),
            }
        }
        // Par continent, puis par sous-région : l'Amérique du Sud (sans sous-régions) entre
        // l'Amérique du Nord et l'Asie, pas avant toutes les sous-régions.
        groups.sort_by_key(|&(g, _)| {
            let region = &self.world.regions[g];
            match region.kind {
                RegionKind::Sub => (region.parent.unwrap_or(g), g),
                _ => (g, g),
            }
        });
        for (_, list) in &mut groups {
            list.sort_by_key(|&c| geo_core::normalize(self.world.countries[c].name.get(self.lang)));
        }
        groups
    }

    fn tile(&self, c: usize) -> Option<Element<'_, Message>> {
        let flag = crate::flags::find(&self.world.countries[c].code)?;
        let content = column![
            container(crate::flags::view(flag, FLAG_W, FLAG_H))
                .center_x(FLAG_W)
                .center_y(FLAG_H),
            text(self.world.countries[c].name.get(self.lang))
                .size(13)
                .width(Fill)
                .center(),
        ]
        .spacing(8)
        .align_x(Center);
        Some(
            button(content)
                .width(FLAG_W + 28.0)
                .padding([12, 14])
                .style(style::option_card(false))
                .on_press(Message::GalleryFlag(Some(c)))
                .into(),
        )
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
                .on_press(Message::CloseGallery),
                column![
                    text(tr!(
                        self.lang,
                        "Drapeaux · {}",
                        "Flags · {}",
                        zone.name.get(self.lang)
                    ))
                    .size(16)
                    .font(style::FONT_SEMIBOLD),
                    text(tr!(
                        self.lang,
                        "{} drapeaux",
                        "{} flags",
                        zone.members.len()
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

        let search = row![
            text_input(
                &tr!(self.lang, "Rechercher un pays…", "Search for a country…"),
                &self.query,
            )
            .on_input(Message::GalleryQuery)
            .size(15)
            .padding([10, 14])
            .width(320)
            .style(style::answer_input(None)),
            text(tr!(
                self.lang,
                "Cliquez sur un drapeau pour l'agrandir.",
                "Click a flag to enlarge it."
            ))
            .size(13)
            .color(style::MUTED),
        ]
        .spacing(16)
        .align_y(Center);

        let groups = self.groups();
        let mut body = Column::new().spacing(26).push(search);
        if groups.is_empty() {
            body = body.push(
                text(tr!(
                    self.lang,
                    "Aucun pays ne correspond à « {} ».",
                    "No country matches “{}”.",
                    self.query.trim()
                ))
                .size(14)
                .color(style::MUTED),
            );
        }
        let titled = groups.len() > 1 || groups.first().is_some_and(|(g, _)| *g != self.region);
        for (g, list) in groups {
            let tiles = Row::with_children(list.into_iter().filter_map(|c| self.tile(c)))
                .spacing(12)
                .wrap()
                .vertical_spacing(12);
            let mut group = Column::new().spacing(12);
            if titled {
                group = group.push(section_title(self.world.regions[g].name.get(self.lang)));
            }
            body = body.push(group.push(tiles));
        }
        let list = scrollable(
            container(
                container(body)
                    .width(Fill)
                    .max_width(GRID_WIDTH)
                    .padding(28),
            )
            .width(Fill)
            .center_x(Fill),
        )
        .height(Fill);
        let page = column![
            top,
            container(list).width(Fill).height(Fill).style(style::page)
        ];
        match self
            .big
            .and_then(|c| Some((c, crate::flags::find(&self.world.countries[c].code)?)))
        {
            Some((c, flag)) => stack![
                page,
                crate::flags::viewer(
                    flag,
                    self.world.countries[c].name.get(self.lang).to_owned(),
                    self.world.regions[self.group_of(c)]
                        .name
                        .get(self.lang)
                        .to_owned(),
                    Message::GalleryFlag(None),
                )
            ]
            .into(),
            None => page.into(),
        }
    }
}

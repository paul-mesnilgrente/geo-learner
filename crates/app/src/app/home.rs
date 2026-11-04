//! Accueil : les quiz en cartes, groupés par catégorie, et les dernières parties.

use std::time::Duration;

use iced::alignment::Horizontal;
use iced::widget::{Column, Row, button, column, container, row, scrollable, space, text};
use iced::{Center, Element, Fill};

use super::{App, HOME_SCROLL, Message, clock, mode_title, percent, plural, score_color};
use crate::dates::DateTime;
use crate::i18n::{self, Lang};
use crate::style;
use crate::{ATLASES, tr};

/// Largeur d'une carte de quiz : trois par ligne sur une fenêtre normale.
const CARD_WIDTH: f32 = 272.0;

impl App {
    pub(super) fn home_page(&self) -> Element<'_, Message> {
        let mut main = column![
            row![
                column![
                    text("GeoLearner").size(32).font(style::FONT_BOLD),
                    text(tr!(
                        self.lang,
                        "La géographie, carte en main. Choisissez un quiz.",
                        "Geography, map in hand. Pick a quiz."
                    ))
                    .size(15)
                    .color(style::MUTED),
                ]
                .spacing(4)
                .width(Fill),
                self.language_picker(),
            ]
            .align_y(Center)
        ]
        .spacing(28)
        .width(Fill);

        // Une seule grille, dans l'ordre du catalogue ; la catégorie est notée sur chaque carte.
        let cards = (0..ATLASES.len()).map(|i| self.quiz_card(i));
        main = main.push(
            Row::with_children(cards)
                .spacing(14)
                .wrap()
                .vertical_spacing(14),
        );

        let content = container(row![main, container(self.recent_card()).width(320)].spacing(28))
            .padding(32)
            .max_width(1280);
        container(
            scrollable(container(content).center_x(Fill))
                .id(HOME_SCROLL)
                .height(Fill),
        )
        .style(style::page)
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Choix de la langue (gardé pour les lancements suivants).
    fn language_picker(&self) -> Element<'_, Message> {
        let choices = Lang::ALL.into_iter().map(|lang| {
            button(text(i18n::native_name(lang)).size(13))
                .padding([6, 14])
                .style(style::pill(self.lang == lang))
                .on_press(Message::SetLanguage(lang))
                .into()
        });
        container(Row::with_children(choices).spacing(4))
            .padding(4)
            .style(|_: &iced::Theme| container::Style {
                background: Some(style::SURFACE.into()),
                border: iced::Border {
                    color: style::BORDER,
                    width: 1.0,
                    radius: 999.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Carte d'un quiz : miniature, nom, nombre de lieux et dernière partie.
    fn quiz_card(&self, index: usize) -> Element<'_, Message> {
        let atlas = &ATLASES[index];
        let played: Vec<_> = self
            .progress
            .sessions
            .iter()
            .filter(|s| s.atlas == atlas.info.id)
            .collect();
        let summary = match played.iter().max_by_key(|s| s.when) {
            None => tr!(self.lang, "Pas encore joué", "Not played yet"),
            Some(last) => tr!(
                self.lang,
                "{} partie{} · dernière le {}",
                "{} game{} · last on {}",
                played.len(),
                plural(self.lang, played.len()),
                DateTime::local(last.when).short(self.lang)
            ),
        };
        button(
            column![
                if atlas.info.numbers {
                    numbers_thumbnail()
                } else if atlas.info.flags {
                    flags_thumbnail()
                } else {
                    self.thumbnails[index].view(130.0, self.lang)
                },
                column![
                    text(atlas.info.category.get(self.lang).to_uppercase())
                        .size(11)
                        .font(style::FONT_SEMIBOLD)
                        .color(style::MUTED),
                    text(atlas.info.title.get(self.lang))
                        .size(16)
                        .font(style::FONT_SEMIBOLD),
                    text(format!(
                        "{} {}",
                        atlas.countries.len(),
                        atlas.info.many.get(self.lang)
                    ))
                    .size(13)
                    .color(style::MUTED),
                    text(summary).size(12).color(style::MUTED),
                ]
                .spacing(3)
                .padding([0, 4]),
            ]
            .spacing(12),
        )
        .padding(10)
        .width(CARD_WIDTH)
        .style(style::option_card(false))
        .on_press(Message::OpenQuiz(index))
        .into()
    }

    /// Dernières parties, tous quiz confondus ; un clic rouvre le quiz réglé sur cette série.
    fn recent_card(&self) -> Element<'_, Message> {
        let mut list = Column::new().spacing(4);
        let mut empty = true;
        for session in self.progress.recent(8) {
            let Some(index) = ATLASES.iter().position(|a| a.info.id == session.atlas) else {
                continue;
            };
            let atlas = &ATLASES[index];
            let Some(region) = atlas.region(&session.region) else {
                continue;
            };
            empty = false;
            let row = row![
                container(space())
                    .width(8)
                    .height(8)
                    .style(style::dot(score_color(session.score))),
                column![
                    text({
                        let zone = atlas.regions[region].name.get(self.lang);
                        if session.review {
                            tr!(self.lang, "Révision · {zone}", "Review · {zone}")
                        } else {
                            zone.to_owned()
                        }
                    })
                    .size(14),
                    text(format!(
                        "{} · {}",
                        atlas.info.title.get(self.lang),
                        mode_title(session.mode, &atlas.info, self.lang)
                    ))
                    .size(12)
                    .color(style::MUTED),
                ]
                .width(Fill),
                column![
                    text(percent(self.lang, session.score))
                        .size(14)
                        .font(style::FONT_SEMIBOLD),
                    text(clock(Duration::from_secs(u64::from(session.seconds))))
                        .size(12)
                        .color(style::MUTED),
                ]
                .align_x(Horizontal::Right),
            ]
            .spacing(12)
            .align_y(Center);
            list = list.push(
                button(row)
                    .padding([6, 8])
                    .width(Fill)
                    .style(style::ghost)
                    .on_press(Message::OpenSeries(index, session.mode, region)),
            );
        }
        if empty {
            list = list.push(
                text(tr!(
                    self.lang,
                    "Aucune partie pour l'instant.",
                    "No games yet."
                ))
                .size(13)
                .color(style::MUTED),
            );
        }
        container(
            column![
                text(tr!(self.lang, "Dernières parties", "Recent games"))
                    .size(16)
                    .font(style::FONT_SEMIBOLD),
                list
            ]
            .spacing(10),
        )
        .padding(18)
        .width(Fill)
        .style(style::card)
        .into()
    }
}

/// Miniature du quiz des drapeaux : quelques drapeaux.
fn flags_thumbnail<'a>() -> Element<'a, Message> {
    let flags = ["FRA", "BRA", "JPN", "CAN"]
        .iter()
        .filter_map(|code| crate::flags::find(code))
        .map(|flag| crate::flags::view(flag, 52.0, 36.0));
    thumbnail_frame(Row::with_children(flags).spacing(10).align_y(Center).into())
}

/// Fond et cadre des miniatures sans carte.
fn thumbnail_frame(content: Element<'_, Message>) -> Element<'_, Message> {
    container(content)
        .center_x(Fill)
        .center_y(130)
        .style(|_: &iced::Theme| container::Style {
            background: Some(style::OCEAN.into()),
            border: iced::Border {
                color: style::BORDER,
                width: 1.0,
                radius: 14.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Miniature d'un quiz sans carte : quelques numéros en grand.
fn numbers_thumbnail<'a>() -> Element<'a, Message> {
    let number = |n: &'a str, alpha: f32| {
        text(n)
            .size(34)
            .font(style::FONT_BOLD)
            .color(style::with_alpha(style::lighten(style::ACCENT, 0.2), alpha))
    };
    thumbnail_frame(
        row![
            number("01", 0.55),
            number("2A", 0.8),
            number("75", 1.0),
            number("971", 0.65),
        ]
        .spacing(16)
        .align_y(Center)
        .into(),
    )
}

//! Accueil : les quiz en cartes, groupés par catégorie, et les dernières parties.

use std::time::Duration;

use iced::alignment::Horizontal;
use iced::widget::{Column, Row, button, column, container, row, scrollable, space, text};
use iced::{Center, Element, Fill};

use super::{App, HOME_SCROLL, Message, clock, mode_title, percent, plural, score_color};
use crate::ATLASES;
use crate::dates::DateTime;
use crate::style;
use crate::words::Words;

/// Largeur d'une carte de quiz : trois par ligne sur une fenêtre normale.
const CARD_WIDTH: f32 = 272.0;

impl App {
    pub(super) fn home_page(&self) -> Element<'_, Message> {
        let mut main = column![
            column![
                text("GeoLearner").size(32).font(style::FONT_BOLD),
                text("La géographie, carte en main. Choisissez un quiz.")
                    .size(15)
                    .color(style::MUTED),
            ]
            .spacing(4)
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
            None => "Pas encore joué".to_owned(),
            Some(last) => format!(
                "{} partie{} · dernière le {}",
                played.len(),
                plural(played.len()),
                DateTime::local(last.when).short()
            ),
        };
        button(
            column![
                self.thumbnails[index].view(130.0),
                column![
                    text(atlas.info.category.to_uppercase())
                        .size(11)
                        .font(style::FONT_SEMIBOLD)
                        .color(style::MUTED),
                    text(atlas.info.title.clone())
                        .size(16)
                        .font(style::FONT_SEMIBOLD),
                    text(format!("{} {}", atlas.countries.len(), atlas.info.many))
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
                    text(atlas.regions[region].name.clone()).size(14),
                    text(format!(
                        "{} · {}",
                        atlas.info.title,
                        mode_title(session.mode, &Words::of(&atlas.info))
                    ))
                    .size(12)
                    .color(style::MUTED),
                ]
                .width(Fill),
                column![
                    text(percent(session.score))
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
                text("Aucune partie pour l'instant.")
                    .size(13)
                    .color(style::MUTED),
            );
        }
        container(
            column![
                text("Dernières parties")
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

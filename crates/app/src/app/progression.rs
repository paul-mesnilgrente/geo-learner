//! Progression sur la page du quiz : évolution du score et du temps d'une série de parties
//! (quiz, mode et zone), et suppression des parties à écarter.

use std::time::Duration;

use iced::widget::{Row, button, canvas, column, container, row, space, text};
use iced::{Center, Element, Fill, Theme};

use super::{App, Message, clock, mode_title, percent, plural, score_color, section_title};
use crate::chart::{Chart, Unit};
use crate::dates::DateTime;
use crate::icons::{Icon, icon};
use crate::progress::Session;
use crate::style;
use crate::words::{Words, capitalize};

/// Nombre de parties prises en compte dans la moyenne récente.
const RECENT: usize = 5;
/// Parties listées avant « Afficher les N parties ».
const RECENT_RUNS: usize = 5;

impl App {
    /// Section « Progression » de la page du quiz, pour le mode et la zone choisis.
    pub(super) fn progress_section(&self) -> Element<'_, Message> {
        let world = self.world();
        let id = &world.info.id;
        let zone = &world.regions[self.region];
        let words = Words::of(&world.info);
        let runs = self.progress.runs(id, self.mode, &zone.id);

        let title = row![
            section_title("Progression"),
            text(format!("{} · {}", zone.name, mode_title(self.mode, &words)))
                .size(12)
                .color(style::MUTED),
        ]
        .spacing(10)
        .align_y(Center);
        let mut content = column![title].spacing(12);
        if runs.is_empty() {
            return content
                .push(
                    container(
                        row![
                            icon(Icon::Chart, 22.0, style::MUTED),
                            text(
                                "Pas encore de partie pour ce mode et cette zone. Chaque partie \
                                 terminée s'ajoutera ici, avec son score et son temps."
                            )
                            .size(13)
                            .color(style::MUTED),
                        ]
                        .spacing(14)
                        .align_y(Center),
                    )
                    .padding([18, 20])
                    .width(Fill)
                    .style(style::card),
                )
                .into();
        }

        let now = DateTime::local(crate::progress::now());
        let dates: Vec<DateTime> = runs.iter().map(|(_, s)| DateTime::local(s.when)).collect();
        let ticks: Vec<String> = dates.iter().map(DateTime::short).collect();
        let details: Vec<String> = dates.iter().map(|d| d.long(&now)).collect();
        let score = Chart {
            unit: Unit::Percent,
            values: runs.iter().map(|(_, s)| s.score).collect(),
            ticks: ticks.clone(),
            details: details.clone(),
        };
        let time = Chart {
            unit: Unit::Seconds,
            values: runs.iter().map(|(_, s)| s.seconds as f32).collect(),
            ticks,
            details,
        };
        content = content
            .push(stats(&runs))
            .push(
                row![
                    chart_card(
                        "Score",
                        format!(
                            "{} {}, moins les erreurs",
                            capitalize(words.many()),
                            words.agree("trouvés")
                        ),
                        score,
                    ),
                    chart_card("Temps", "Durée de chaque partie".into(), time),
                ]
                .spacing(12),
            )
            .push(self.runs_card(&runs, &now));
        content.into()
    }

    /// Liste des parties de la série, la plus récente en premier, avec suppression.
    fn runs_card<'a>(
        &'a self,
        runs: &[(usize, &'a Session)],
        now: &DateTime,
    ) -> Element<'a, Message> {
        let world = self.world();
        let words = Words::of(&world.info);
        let zone = &world.regions[self.region];
        let mut body = column![
            row![
                text("Parties")
                    .size(16)
                    .font(style::FONT_SEMIBOLD)
                    .width(Fill),
                text("Écartez celles qui ne reflètent pas votre niveau.")
                    .size(12)
                    .color(style::MUTED),
            ]
            .align_y(Center)
        ]
        .spacing(10);

        // Annulation de la dernière suppression, si elle concerne cette série.
        if let Some((_, session)) = self
            .deleted
            .as_ref()
            .filter(|(_, s)| s.is(&world.info.id, self.mode, &zone.id))
        {
            body = body.push(
                container(
                    row![
                        text(format!(
                            "Partie du {} supprimée.",
                            DateTime::local(session.when).long(now)
                        ))
                        .size(13)
                        .width(Fill),
                        button(text("Annuler").size(13).font(style::FONT_SEMIBOLD))
                            .padding([5, 12])
                            .style(style::secondary)
                            .on_press(Message::UndoDelete),
                    ]
                    .spacing(12)
                    .align_y(Center),
                )
                .padding(iced::Padding::from([8, 8]).left(14))
                .style(|_: &Theme| container::Style {
                    background: Some(style::SURFACE_2.into()),
                    border: iced::Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            );
        }

        let found = capitalize(&words.agree("trouvés"));
        let head =
            |label: String, width: f32| text(label).size(12).color(style::MUTED).width(width);
        body = body.push(
            row![
                text("Date").size(12).color(style::MUTED).width(Fill),
                head("Score".into(), 80.0),
                head(found, 90.0),
                head("Temps".into(), 90.0),
                space().width(30),
            ]
            .spacing(8)
            .padding([0, 8]),
        );
        let mut timed_out = false;
        let shown = if self.all_runs {
            runs.len()
        } else {
            RECENT_RUNS
        };
        for &(index, session) in runs.iter().rev().take(shown) {
            timed_out |= session.timed_out();
            let time_color = if session.timed_out() {
                style::ORANGE
            } else {
                style::TEXT
            };
            let mut time = row![
                text(clock(Duration::from_secs(u64::from(session.seconds))))
                    .size(14)
                    .color(time_color)
            ]
            .spacing(6)
            .align_y(Center);
            if session.timed_out() {
                time = time.push(icon(Icon::Timer, 13.0, style::ORANGE));
            }
            let date = text(DateTime::local(session.when).long(now))
                .size(14)
                .width(Fill);
            let confirming = self.confirm_delete == Some(index);
            let content = if confirming {
                row![
                    date,
                    text(format!(
                        "Supprimer cette partie ({}, {}) ?",
                        percent(session.score),
                        clock(Duration::from_secs(u64::from(session.seconds)))
                    ))
                    .size(13)
                    .color(style::lighten(style::DANGER, 0.3)),
                    button(text("Annuler").size(13))
                        .padding([5, 12])
                        .style(style::secondary)
                        .on_press(Message::CancelDelete),
                    button(text("Supprimer").size(13).font(style::FONT_SEMIBOLD))
                        .padding([5, 12])
                        .style(style::danger_soft)
                        .on_press(Message::DeleteRun(index)),
                ]
            } else {
                row![
                    date,
                    row![
                        container(space())
                            .width(8)
                            .height(8)
                            .style(style::dot(score_color(session.score))),
                        text(percent(session.score))
                            .size(14)
                            .font(style::FONT_SEMIBOLD),
                    ]
                    .spacing(8)
                    .align_y(Center)
                    .width(80),
                    text(format!("{}/{}", session.found, session.total))
                        .size(14)
                        .width(90),
                    container(time).width(90),
                    button(container(icon(Icon::Trash, 15.0, style::MUTED)).center(Fill))
                        .width(30)
                        .height(30)
                        .padding(0)
                        .style(style::ghost)
                        .on_press(Message::AskDelete(index)),
                ]
            };
            body = body.push(
                container(content.spacing(8).align_y(Center).height(30))
                    .padding([4, 8])
                    .style(move |_: &Theme| container::Style {
                        border: iced::Border {
                            color: style::with_alpha(style::DANGER, 0.45),
                            width: if confirming { 1.0 } else { 0.0 },
                            radius: 8.0.into(),
                        },
                        background: Some(if confirming {
                            style::mix(style::SURFACE, style::DANGER, 0.10).into()
                        } else {
                            style::with_alpha(style::SURFACE_2, 0.35).into()
                        }),
                        ..Default::default()
                    }),
            );
        }
        if runs.len() > RECENT_RUNS {
            let label = if self.all_runs {
                "Afficher moins".to_owned()
            } else {
                format!("Afficher les {} parties", runs.len())
            };
            body = body.push(
                button(text(label).size(13))
                    .padding([6, 12])
                    .style(style::ghost)
                    .on_press(Message::ToggleAllRuns),
            );
        }
        if timed_out {
            body = body.push(
                row![
                    icon(Icon::Timer, 12.0, style::ORANGE),
                    text("Partie arrêtée par le minuteur.")
                        .size(12)
                        .color(style::MUTED),
                ]
                .spacing(6)
                .align_y(Center),
            );
        }
        container(body.spacing(6))
            .padding(18)
            .width(Fill)
            .style(style::card)
            .into()
    }

    /// Lieux les plus souvent ratés dans la série choisie.
    pub(super) fn series_missed_card(&self) -> Element<'_, Message> {
        let world = self.world();
        let zone = &world.regions[self.region];
        let id = &world.info.id;
        // Lieux encore présents dans l'atlas (les codes inconnus sont ignorés).
        let missed: Vec<(&str, usize)> = self
            .progress
            .most_missed(|s| s.is(id, self.mode, &zone.id), 12)
            .into_iter()
            .filter_map(|(code, n)| {
                let place = world.countries.iter().find(|c| c.code == code)?;
                Some((place.name.as_str(), n))
            })
            .collect();
        let mut body = column![
            text("À revoir").size(16).font(style::FONT_SEMIBOLD),
            text("Les plus souvent ratés dans cette série.")
                .size(12)
                .color(style::MUTED),
        ]
        .spacing(4);
        if missed.is_empty() {
            body = body.push(
                container(
                    text(if self.progress.runs(id, self.mode, &zone.id).is_empty() {
                        "Rien pour l'instant."
                    } else {
                        "Aucune erreur, bravo !"
                    })
                    .size(13)
                    .color(style::MUTED),
                )
                .padding(iced::Padding::ZERO.top(8)),
            );
        } else {
            let chips = missed.into_iter().map(|(name, n)| {
                container(
                    row![
                        text(name).size(12),
                        text(format!("×{n}")).size(11).color(style::MUTED),
                    ]
                    .spacing(6),
                )
                .padding([4, 10])
                .style(style::chip(style::ORANGE))
                .into()
            });
            body = body.push(
                container(
                    Row::with_children(chips)
                        .spacing(6)
                        .wrap()
                        .vertical_spacing(6),
                )
                .padding(iced::Padding::ZERO.top(8)),
            );
        }
        container(body)
            .padding(18)
            .width(Fill)
            .style(style::card)
            .into()
    }
}

/// Chiffres clés de la série : nombre de parties, record, dernière partie, moyenne récente.
fn stats<'a>(runs: &[(usize, &Session)]) -> Element<'a, Message> {
    let tile = |value: String, label: String| {
        container(
            column![
                text(value).size(20).font(style::FONT_BOLD),
                text(label).size(12).color(style::MUTED)
            ]
            .spacing(2),
        )
        .padding([12, 16])
        .width(Fill)
        .style(style::card)
    };
    let with_time = |s: &Session| {
        format!(
            "{} · {}",
            percent(s.score),
            clock(Duration::from_secs(u64::from(s.seconds)))
        )
    };
    let best = runs
        .iter()
        .map(|(_, s)| *s)
        .max_by(|a, b| a.score.total_cmp(&b.score).then(b.seconds.cmp(&a.seconds)));
    let last = runs.last().map(|(_, s)| *s);
    let recent: Vec<f32> = runs
        .iter()
        .rev()
        .take(RECENT)
        .map(|(_, s)| s.score)
        .collect();
    let average = recent.iter().sum::<f32>() / recent.len().max(1) as f32;
    row![
        tile(
            runs.len().to_string(),
            format!("partie{}", plural(runs.len()))
        ),
        tile(best.map(with_time).unwrap_or_default(), "record".into()),
        tile(
            last.map(with_time).unwrap_or_default(),
            "dernière partie".into()
        ),
        tile(
            percent(average),
            if recent.len() > 1 {
                format!("moyenne récente ({})", recent.len())
            } else {
                "moyenne".into()
            }
        ),
    ]
    .spacing(10)
    .into()
}

fn chart_card<'a>(title: &'a str, subtitle: String, chart: Chart) -> Element<'a, Message> {
    container(
        column![
            row![
                text(title).size(16).font(style::FONT_SEMIBOLD),
                text(subtitle).size(12).color(style::MUTED),
            ]
            .spacing(10)
            .align_y(iced::Alignment::End),
            canvas(chart).width(Fill).height(170),
        ]
        .spacing(12),
    )
    .padding(18)
    .width(Fill)
    .style(style::card)
    .into()
}

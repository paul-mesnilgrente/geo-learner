//! Progression sur la page du quiz : évolution du score et du temps d'une série de parties
//! (quiz, mode et zone), et suppression des parties à écarter.

use std::time::Duration;

use iced::widget::{Row, button, canvas, column, container, row, space, text};
use iced::{Center, Element, Fill, Theme};

use super::{App, Message, clock, mode_title, percent, plural, score_color, section_title};
use crate::chart::{Chart, Unit};
use crate::dates::DateTime;
use crate::i18n::Lang;
use crate::icons::{Icon, icon};
use crate::progress::{Level, Mastery, Session};
use crate::style;
use crate::tr;
use crate::words::{Words, capitalize};

/// Nombre de parties prises en compte dans la moyenne récente.
const RECENT: usize = 5;
/// Parties listées avant « Afficher les N parties ».
const RECENT_RUNS: usize = 5;
/// Lieux « à revoir » affichés au plus.
const MAX_WEAK: usize = 12;
/// Lieux d'une partie de révision au plus.
const MAX_REVIEW: usize = 20;

impl App {
    /// Section « Progression » de la page du quiz, pour le mode et la zone choisis.
    pub(super) fn progress_section(&self) -> Element<'_, Message> {
        let world = self.world();
        let id = &world.info.id;
        let zone = &world.regions[self.region];
        let words = Words::of(&world.info, self.lang);
        let runs = self.progress.runs(id, self.mode, &zone.id);

        let title = row![
            section_title(&tr!(self.lang, "Progression", "Progress")),
            text(format!(
                "{} · {}",
                zone.name.get(self.lang),
                mode_title(self.mode, &self.world().info, self.lang)
            ))
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
                            text(tr!(
                                self.lang,
                                "Pas encore de partie pour ce mode et cette zone. Chaque partie \
                                 terminée s'ajoutera ici, avec son score et son temps.",
                                "No games yet for this mode and area. Each finished game will \
                                 show up here, with its score and time."
                            ))
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
        let ticks: Vec<String> = dates.iter().map(|d| d.short(self.lang)).collect();
        let details: Vec<String> = dates.iter().map(|d| d.long(&now, self.lang)).collect();
        let score = Chart {
            unit: Unit::Percent,
            lang: self.lang,
            values: runs.iter().map(|(_, s)| s.score).collect(),
            ticks: ticks.clone(),
            details: details.clone(),
        };
        let time = Chart {
            unit: Unit::Seconds,
            lang: self.lang,
            values: runs.iter().map(|(_, s)| s.seconds as f32).collect(),
            ticks,
            details,
        };
        content = content
            .push(stats(&runs, self.lang))
            .push(
                row![
                    chart_card(
                        "Score",
                        tr!(
                            self.lang,
                            "{} {}, moins les erreurs",
                            "{} {}, minus mistakes",
                            capitalize(words.many()),
                            words.agree("trouvés", "found")
                        ),
                        score,
                    ),
                    chart_card(
                        tr!(self.lang, "Temps", "Time"),
                        tr!(self.lang, "Durée de chaque partie", "Length of each game"),
                        time
                    ),
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
        let words = Words::of(&world.info, self.lang);
        let zone = &world.regions[self.region];
        let mut body = column![
            row![
                text(tr!(self.lang, "Parties", "Games"))
                    .size(16)
                    .font(style::FONT_SEMIBOLD)
                    .width(Fill),
                text(tr!(
                    self.lang,
                    "Écartez celles qui ne reflètent pas votre niveau.",
                    "Remove the ones that don't reflect your level."
                ))
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
                        text(tr!(
                            self.lang,
                            "Partie du {} supprimée.",
                            "Game of {} deleted.",
                            DateTime::local(session.when).long(now, self.lang)
                        ))
                        .size(13)
                        .width(Fill),
                        button(
                            text(tr!(self.lang, "Annuler", "Undo"))
                                .size(13)
                                .font(style::FONT_SEMIBOLD)
                        )
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

        let found = capitalize(&words.agree("trouvés", "found"));
        let head =
            |label: String, width: f32| text(label).size(12).color(style::MUTED).width(width);
        body = body.push(
            row![
                text("Date").size(12).color(style::MUTED).width(Fill),
                head("Score".into(), 80.0),
                head(found, 90.0),
                head(tr!(self.lang, "Temps", "Time"), 90.0),
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
            let date = text(DateTime::local(session.when).long(now, self.lang))
                .size(14)
                .width(Fill);
            let confirming = self.confirm_delete == Some(index);
            let content = if confirming {
                row![
                    date,
                    text(tr!(
                        self.lang,
                        "Supprimer cette partie ({}, {}) ?",
                        "Delete this game ({}, {})?",
                        percent(self.lang, session.score),
                        clock(Duration::from_secs(u64::from(session.seconds)))
                    ))
                    .size(13)
                    .color(style::lighten(style::DANGER, 0.3)),
                    button(text(tr!(self.lang, "Annuler", "Cancel")).size(13))
                        .padding([5, 12])
                        .style(style::secondary)
                        .on_press(Message::CancelDelete),
                    button(
                        text(tr!(self.lang, "Supprimer", "Delete"))
                            .size(13)
                            .font(style::FONT_SEMIBOLD)
                    )
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
                        text(percent(self.lang, session.score))
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
                tr!(self.lang, "Afficher moins", "Show fewer")
            } else {
                tr!(
                    self.lang,
                    "Afficher les {} parties",
                    "Show all {} games",
                    runs.len()
                )
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
                    text(tr!(
                        self.lang,
                        "Partie arrêtée par le minuteur.",
                        "Game stopped by the timer."
                    ))
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
        // Maîtrise dans ce mode, toutes zones confondues, restreinte aux lieux de la zone.
        let mastery = self.progress.mastery(&world.info.id, self.mode);
        let levels: Vec<Option<&Mastery>> = zone
            .members
            .iter()
            .map(|&c| mastery.get(&world.countries[c].code))
            .collect();
        let count = |level: Option<Level>| {
            levels
                .iter()
                .filter(|m| m.map(|m| m.level) == level)
                .count()
        };
        // Lieux ratés la dernière fois : les plus souvent ratés d'abord, puis les plus récents.
        let mut weak: Vec<(usize, &str, &Mastery)> = zone
            .members
            .iter()
            .zip(&levels)
            .filter_map(|(&c, m)| {
                let m = m.filter(|m| m.level == Level::Weak)?;
                Some((c, world.countries[c].name.get(self.lang), m))
            })
            .collect();
        weak.sort_by(|a, b| {
            (b.2.missed.cmp(&a.2.missed))
                .then(b.2.last.cmp(&a.2.last))
                .then(a.1.cmp(b.1))
        });
        // Révision : les plus souvent ratés, en nombre raisonnable pour une partie courte.
        let review: Vec<usize> = weak.iter().take(MAX_REVIEW).map(|w| w.0).collect();
        let more = weak.len().saturating_sub(MAX_WEAK);
        weak.truncate(MAX_WEAK);

        let lang = self.lang;
        let tally = |n: usize, label: String, color: iced::Color| {
            row![
                container(space())
                    .width(8)
                    .height(8)
                    .style(move |_: &Theme| container::Style {
                        background: Some(color.into()),
                        border: iced::border::rounded(4),
                        ..container::Style::default()
                    }),
                text(format!("{n} {label}")).size(12).color(style::MUTED),
            ]
            .spacing(6)
            .align_y(Center)
            .into()
        };
        let mut tallies: Vec<Element<'_, Message>> = vec![
            tally(
                count(Some(Level::Known)),
                tr!(lang, "acquis", "known"),
                style::SUCCESS,
            ),
            tally(
                count(Some(Level::Learning)),
                tr!(lang, "en cours", "learning"),
                style::WARNING,
            ),
            tally(
                count(Some(Level::Weak)),
                tr!(lang, "à revoir", "to review"),
                style::ORANGE,
            ),
        ];
        if count(None) > 0 {
            tallies.push(tally(
                count(None),
                tr!(lang, "jamais joués", "not played yet"),
                style::BORDER,
            ));
        }
        let mut body = column![
            text(tr!(lang, "À revoir", "To review"))
                .size(16)
                .font(style::FONT_SEMIBOLD),
            text(tr!(
                lang,
                "Ratés la dernière fois, dans ce mode.",
                "Missed last time, in this mode."
            ))
            .size(12)
            .color(style::MUTED),
            container(
                Row::with_children(tallies)
                    .spacing(14)
                    .wrap()
                    .vertical_spacing(6)
            )
            .padding(iced::Padding::ZERO.top(8)),
        ]
        .spacing(4);
        if weak.is_empty() {
            body = body.push(
                container(
                    text(if count(None) == levels.len() {
                        tr!(lang, "Rien pour l'instant.", "Nothing yet.")
                    } else {
                        tr!(
                            lang,
                            "Rien à revoir, bravo !",
                            "Nothing to review, well done!"
                        )
                    })
                    .size(13)
                    .color(style::MUTED),
                )
                .padding(iced::Padding::ZERO.top(8)),
            );
        } else {
            let mut chips: Vec<Element<'_, Message>> = weak
                .into_iter()
                .map(|(_, name, m)| {
                    container(
                        row![
                            text(name).size(12),
                            text(format!("×{}", m.missed)).size(11).color(style::MUTED),
                        ]
                        .spacing(6),
                    )
                    .padding([4, 10])
                    .style(style::chip(style::ORANGE))
                    .into()
                })
                .collect();
            if more > 0 {
                chips.push(
                    container(text(format!("+{more}")).size(12).color(style::MUTED))
                        .padding([4, 6])
                        .into(),
                );
            }
            body = body.push(
                container(
                    Row::with_children(chips)
                        .spacing(6)
                        .wrap()
                        .vertical_spacing(6),
                )
                .padding(iced::Padding::ZERO.top(8)),
            );
            let words = Words::of(&world.info, lang);
            let label = match (lang, review.len()) {
                (Lang::Fr, 1) => format!("Réviser {}", words.this()),
                (Lang::Fr, n) => format!("Réviser ces {n} {}", words.many()),
                (Lang::En, 1) => format!("Review {}", words.this()),
                (Lang::En, n) => format!("Review these {n} {}", words.many()),
            };
            body = body.push(
                container(
                    button(
                        row![icon(Icon::Target, 14.0, style::TEXT), text(label).size(14)]
                            .spacing(8)
                            .align_y(Center),
                    )
                    .padding([9, 14])
                    .style(style::secondary)
                    .on_press(Message::Review(review)),
                )
                .padding(iced::Padding::ZERO.top(10)),
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
fn stats<'a>(runs: &[(usize, &Session)], lang: Lang) -> Element<'a, Message> {
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
            percent(lang, s.score),
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
            tr!(lang, "partie{}", "game{}", plural(lang, runs.len()))
        ),
        tile(
            best.map(with_time).unwrap_or_default(),
            tr!(lang, "record", "best")
        ),
        tile(
            last.map(with_time).unwrap_or_default(),
            tr!(lang, "dernière partie", "last game")
        ),
        tile(
            percent(lang, average),
            if recent.len() > 1 {
                tr!(
                    lang,
                    "moyenne récente ({})",
                    "recent average ({})",
                    recent.len()
                )
            } else {
                tr!(lang, "moyenne", "average")
            }
        ),
    ]
    .spacing(10)
    .into()
}

fn chart_card<'a>(
    title: impl Into<String>,
    subtitle: String,
    chart: Chart,
) -> Element<'a, Message> {
    container(
        column![
            row![
                text(title.into()).size(16).font(style::FONT_SEMIBOLD),
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

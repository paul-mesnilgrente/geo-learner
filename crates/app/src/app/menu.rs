//! Menu d'aide de l'accueil (bouton « ? » ou F1) : à propos, raccourcis clavier, signaler un
//! problème, exporter et importer la progression. Les fenêtres s'ouvrent par-dessus la page.

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    Column, button, column, container, mouse_area, opaque, responsive, row, rule, space, stack,
    text,
};
use iced::{Center, Color, Element, Fill, Padding, Task, Theme};

use super::home::{HOME_PADDING, HOME_WIDTH, SIDE_GAP, SIDE_WIDTH};
use super::{App, Message};
use crate::i18n::Lang;
use crate::icons::{Icon, icon};
use crate::links;
use crate::progress::Progress;
use crate::style;
use crate::tr;

/// Fenêtre ouverte depuis le menu.
#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    About,
    Shortcuts,
    Report,
    /// Résultat d'un export ou d'un import.
    Notice {
        title: String,
        body: String,
    },
}

/// Haut du menu déroulant, sous le bouton « ? » de l'en-tête de l'accueil.
const MENU_TOP: f32 = 104.0;
const MENU_WIDTH: f32 = 270.0;

impl App {
    /// Bouton « ? » de l'en-tête de l'accueil.
    pub(super) fn help_button(&self) -> Element<'_, Message> {
        let color = if self.menu {
            style::ACCENT
        } else {
            style::TEXT
        };
        button(container(icon(Icon::Help, 20.0, color)).center(Fill))
            .width(38)
            .height(38)
            .padding(0)
            .style(style::map_control)
            .on_press(Message::Menu(!self.menu))
            .into()
    }

    /// Menu ou fenêtre ouverts par-dessus l'accueil.
    pub(super) fn menu_layer(&self) -> Option<Element<'_, Message>> {
        if let Some(dialog) = &self.dialog {
            return Some(self.dialog_view(dialog));
        }
        self.menu.then(|| self.menu_view())
    }

    fn menu_view(&self) -> Element<'_, Message> {
        let lang = self.lang;
        // Sous le bouton, aligné sur le bord droit de la colonne principale de l'accueil.
        let placed = responsive(move |space| {
            let width = space.width.min(HOME_WIDTH);
            let right = (space.width - width) / 2.0 + HOME_PADDING + SIDE_WIDTH + SIDE_GAP;
            container(opaque(menu_card(lang)))
                .width(Fill)
                .height(Fill)
                .align_x(Horizontal::Right)
                .align_y(Vertical::Top)
                .padding(Padding::ZERO.top(MENU_TOP).right(right.max(16.0)))
                .into()
        });
        // Un clic à côté referme le menu.
        stack![
            mouse_area(container(space()).width(Fill).height(Fill)).on_press(Message::Menu(false)),
            placed
        ]
        .into()
    }

    fn dialog_view(&self, dialog: &Dialog) -> Element<'_, Message> {
        let lang = self.lang;
        let (title, body): (String, Element<'_, Message>) = match dialog {
            Dialog::About => (
                tr!(lang, "À propos de GeoLearner", "About GeoLearner"),
                about(lang),
            ),
            Dialog::Shortcuts => (
                tr!(lang, "Raccourcis clavier", "Keyboard shortcuts"),
                shortcuts(lang),
            ),
            Dialog::Report => (
                tr!(lang, "Signaler un problème", "Report a problem"),
                report(lang),
            ),
            Dialog::Notice { title, body } => (
                title.clone(),
                text(body.clone()).size(14).color(style::MUTED).into(),
            ),
        };
        let close = button(container(icon(Icon::Close, 16.0, style::MUTED)).center(Fill))
            .width(32)
            .height(32)
            .padding(0)
            .style(style::ghost)
            .on_press(Message::Dialog(None));
        let card = container(
            column![
                row![
                    text(title).size(20).font(style::FONT_BOLD).width(Fill),
                    close
                ]
                .align_y(Center),
                body,
            ]
            .spacing(18),
        )
        .width(520)
        .padding(24)
        .style(|_: &Theme| container::Style {
            background: Some(style::SURFACE.into()),
            border: iced::Border {
                color: style::BORDER,
                width: 1.0,
                radius: 16.0.into(),
            },
            ..Default::default()
        });
        // Fond assombri : un clic à côté de la fenêtre la referme.
        stack![
            opaque(
                mouse_area(
                    container(space())
                        .width(Fill)
                        .height(Fill)
                        .style(|_: &Theme| container::Style {
                            background: Some(style::with_alpha(Color::BLACK, 0.6).into()),
                            ..Default::default()
                        })
                )
                .on_press(Message::Dialog(None))
            ),
            container(opaque(card)).center(Fill).padding(24),
        ]
        .into()
    }

    /// Exporte la progression dans un fichier choisi par l'utilisateur (portail de fichiers
    /// sous Linux : marche aussi dans Flatpak et Snap, sans accès au dossier personnel).
    pub(super) fn export_progress(&mut self) -> Task<Message> {
        self.menu = false;
        let bytes = self.progress.to_json();
        let today = crate::dates::DateTime::local(crate::progress::now());
        let name = format!(
            "geolearner-{:04}-{:02}-{:02}.json",
            today.year, today.month, today.day
        );
        Task::perform(
            async move {
                let Some(file) = rfd::AsyncFileDialog::new()
                    .set_file_name(name)
                    .add_filter("JSON", &["json"])
                    .save_file()
                    .await
                else {
                    return Ok(None);
                };
                file.write(&bytes)
                    .await
                    .map(|()| Some(file.file_name()))
                    .map_err(|e| e.to_string())
            },
            Message::ProgressExported,
        )
    }

    pub(super) fn import_progress(&mut self) -> Task<Message> {
        self.menu = false;
        Task::perform(
            async {
                let file = rfd::AsyncFileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                    .await?;
                Some(file.read().await)
            },
            Message::ProgressPicked,
        )
    }

    pub(super) fn on_exported(&mut self, result: Result<Option<String>, String>) {
        let lang = self.lang;
        self.dialog = match result {
            Ok(None) => None,
            Ok(Some(name)) => Some(Dialog::Notice {
                title: tr!(lang, "Progression exportée", "Progress exported"),
                body: tr!(
                    lang,
                    "{} parties enregistrées dans « {name} ». Pour les retrouver sur un autre \
                     ordinateur : « Importer une progression… ».",
                    "{} games saved to “{name}”. To get them back on another computer: \
                     “Import progress…”.",
                    self.progress.sessions.len()
                ),
            }),
            Err(e) => Some(Dialog::Notice {
                title: tr!(lang, "Export impossible", "Export failed"),
                body: e,
            }),
        };
    }

    pub(super) fn on_imported(&mut self, bytes: Option<Vec<u8>>) {
        let Some(bytes) = bytes else { return };
        let lang = self.lang;
        self.dialog = Some(match Progress::from_json(&bytes) {
            Ok(imported) => {
                let (added, known) = self.progress.merge(imported);
                self.sync_preview();
                Dialog::Notice {
                    title: tr!(lang, "Progression importée", "Progress imported"),
                    body: match (added, known) {
                        (0, _) => tr!(
                            lang,
                            "Toutes les parties de ce fichier sont déjà là.",
                            "Every game in this file is already here."
                        ),
                        (added, 0) => {
                            tr!(lang, "{added} parties ajoutées.", "{added} games added.")
                        }
                        (added, known) => tr!(
                            lang,
                            "{added} parties ajoutées ; {known} étaient déjà là.",
                            "{added} games added; {known} were already here."
                        ),
                    },
                }
            }
            Err(e) => Dialog::Notice {
                title: tr!(lang, "Import impossible", "Import failed"),
                body: tr!(
                    lang,
                    "Ce fichier n'est pas une progression GeoLearner ({e}).",
                    "This file isn't GeoLearner progress ({e})."
                ),
            },
        });
    }
}

fn menu_card<'a>(lang: Lang) -> Element<'a, Message> {
    let item = |i: Icon, label: String, message: Message| {
        button(
            row![icon(i, 18.0, style::TEXT), text(label).size(14)]
                .spacing(12)
                .align_y(Center),
        )
        .width(Fill)
        .padding([9, 12])
        .style(style::ghost)
        .on_press(message)
    };
    container(
        column![
            item(
                Icon::Info,
                tr!(lang, "À propos de GeoLearner", "About GeoLearner"),
                Message::Dialog(Some(Dialog::About)),
            ),
            item(
                Icon::Keyboard,
                tr!(lang, "Raccourcis clavier", "Keyboard shortcuts"),
                Message::Dialog(Some(Dialog::Shortcuts)),
            ),
            item(
                Icon::Message,
                tr!(lang, "Signaler un problème", "Report a problem"),
                Message::Dialog(Some(Dialog::Report)),
            ),
            container(rule::horizontal(1)).padding([4, 8]),
            item(
                Icon::Download,
                tr!(lang, "Exporter la progression…", "Export progress…"),
                Message::ExportProgress,
            ),
            item(
                Icon::Upload,
                tr!(lang, "Importer une progression…", "Import progress…"),
                Message::ImportProgress,
            ),
        ]
        .spacing(2),
    )
    .width(MENU_WIDTH)
    .padding(6)
    // Fond plein : les miniatures de l'accueil ne doivent pas transparaître.
    .style(|theme: &Theme| container::Style {
        background: Some(style::SURFACE.into()),
        ..style::floating(theme)
    })
    .into()
}

/// Lien cliquable, souligné par la couleur d'accent.
fn link<'a>(label: String, url: String) -> Element<'a, Message> {
    button(
        text(label)
            .size(14)
            .color(style::lighten(style::ACCENT, 0.15)),
    )
    .padding(0)
    .style(|_: &Theme, _| button::Style::default())
    .on_press(Message::OpenUrl(url))
    .into()
}

fn about<'a>(lang: Lang) -> Element<'a, Message> {
    let muted = |s: String| text(s).size(13).color(style::MUTED);
    column![
        row![
            icon(Icon::Globe, 40.0, style::lighten(style::ACCENT, 0.2)),
            column![
                text("GeoLearner").size(24).font(style::FONT_BOLD),
                muted(tr!(lang, "Version {}", "Version {}", links::VERSION)),
            ]
            .spacing(2),
        ]
        .spacing(14)
        .align_y(Center),
        text(tr!(
            lang,
            "La géographie, carte en main : pays, drapeaux, océans, mers, fleuves, régions et \
             départements. Tout fonctionne hors ligne ; la progression reste sur cet ordinateur.",
            "Geography, map in hand: countries, flags, oceans, seas, rivers, regions and \
             departments. Everything works offline; your progress stays on this computer."
        ))
        .size(14),
        column![
            link(
                tr!(lang, "Code source (licence MIT)", "Source code (MIT licence)"),
                links::REPOSITORY.into()
            ),
            link(
                tr!(lang, "Politique de confidentialité", "Privacy policy"),
                links::PRIVACY.into()
            ),
            link(
                tr!(lang, "Contact : {}", "Contact: {}", links::CONTACT),
                format!("mailto:{}", links::CONTACT)
            ),
        ]
        .spacing(6),
        column![
            muted(tr!(
                lang,
                "Cartes : Natural Earth (domaine public).",
                "Maps: Natural Earth (public domain)."
            )),
            muted(tr!(
                lang,
                "Drapeaux : hampusborgos/country-flags, d'après Wikimedia Commons (domaine public).",
                "Flags: hampusborgos/country-flags, from Wikimedia Commons (public domain)."
            )),
            muted(tr!(
                lang,
                "Reconnaissance vocale : modèles Vosk d'Alpha Cephei (Apache 2.0).",
                "Speech recognition: Vosk models by Alpha Cephei (Apache 2.0)."
            )),
            muted(tr!(
                lang,
                "Interface : iced. © 2026 Paul Mesnilgrente (BigLearner).",
                "Interface: iced. © 2026 Paul Mesnilgrente (BigLearner)."
            )),
        ]
        .spacing(4),
    ]
    .spacing(18)
    .into()
}

fn shortcuts<'a>(lang: Lang) -> Element<'a, Message> {
    let lines = [
        (
            "F1".to_owned(),
            tr!(lang, "Ouvrir ce menu d'aide", "Open this help menu"),
        ),
        (
            tr!(lang, "Entrée", "Enter"),
            tr!(
                lang,
                "Valider la réponse tapée (un nom reconnu est validé tout seul)",
                "Submit the typed answer (a recognised name is submitted by itself)"
            ),
        ),
        (
            tr!(lang, "Échap", "Esc"),
            tr!(
                lang,
                "Refermer : drapeau agrandi, carte de situation, bilan, exploration, galerie",
                "Close: enlarged flag, locator map, summary, explorer, gallery"
            ),
        ),
        (
            "Ctrl+M".to_owned(),
            tr!(
                lang,
                "Allumer ou couper le micro (saisie vocale)",
                "Turn the microphone on or off (voice input)"
            ),
        ),
        (
            "Ctrl+Z".to_owned(),
            tr!(
                lang,
                "Annuler la suppression d'une partie",
                "Undo deleting a game"
            ),
        ),
        (
            tr!(lang, "Molette", "Wheel"),
            tr!(
                lang,
                "Zoomer sur la carte, vers le curseur",
                "Zoom the map, towards the cursor"
            ),
        ),
        (
            tr!(lang, "Glisser", "Drag"),
            tr!(lang, "Déplacer la carte", "Move the map"),
        ),
    ];
    let rows = lines.into_iter().map(|(key, what)| {
        row![
            container(
                container(text(key.to_string()).size(13).font(style::FONT_SEMIBOLD))
                    .padding([3, 9])
                    .style(|_: &Theme| container::Style {
                        background: Some(style::SURFACE_2.into()),
                        border: iced::Border {
                            color: style::BORDER,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    })
            )
            .width(92),
            text(what).size(14).width(Fill),
        ]
        .spacing(12)
        .align_y(Center)
        .into()
    });
    Column::with_children(rows).spacing(10).into()
}

fn report<'a>(lang: Lang) -> Element<'a, Message> {
    let action = |i: Icon, label: String, url: String| {
        button(
            row![icon(i, 16.0, style::TEXT), text(label).size(14)]
                .spacing(10)
                .align_y(Center),
        )
        .padding([10, 14])
        .width(Fill)
        .style(style::secondary)
        .on_press(Message::OpenUrl(url))
    };
    column![
        text(tr!(
            lang,
            "Un bug, un nom mal reconnu, une frontière fausse, une idée ? Décrivez ce qui s'est \
             passé ; la version et le système sont ajoutés pour vous.",
            "A bug, a name not recognised, a wrong border, an idea? Describe what happened; \
             the version and system are added for you."
        ))
        .size(14),
        column![
            action(
                Icon::Message,
                tr!(
                    lang,
                    "Ouvrir un ticket sur GitHub",
                    "Open an issue on GitHub"
                ),
                links::new_issue()
            ),
            action(
                Icon::Mail,
                tr!(lang, "Écrire à {}", "Email {}", links::CONTACT),
                links::mail()
            ),
        ]
        .spacing(8),
        text(format!(
            "GeoLearner {} · {}",
            links::VERSION,
            links::system()
        ))
        .size(12)
        .color(style::MUTED),
    ]
    .spacing(16)
    .into()
}

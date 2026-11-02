//! Application : écran d'accueil (choix du mode et de la zone, progression) et écran de jeu.

mod progression;

use std::time::{Duration, Instant};

use geo_core::{Mark, Matcher, Mode, Outcome, Quiz, RegionKind, World};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::Cache;
use iced::widget::{
    Column, Row, button, canvas, column, container, opaque, operation, progress_bar, row, rule,
    scrollable, sensor, space, stack, text, text_input,
};
use iced::{Center, Color, Element, Fill, Size, Subscription, Task, Theme, Vector, keyboard, time};

use crate::ATLASES;
use crate::icons::{Icon, icon};
use crate::map::{self, Camera, Flash, MapEvent, MapView, Projection, Transform};
use crate::progress::{self, Progress, Session};
use crate::settings::Settings;
use crate::style;
use crate::voice;
use crate::words::{Words, capitalize};

const ANSWER_INPUT: &str = "answer";
const HOME_SCROLL: &str = "home";
/// Délai avant de valider un nom qui est aussi le début d'un autre (« Niger » / « Nigeria »).
const AMBIGUOUS_DELAY: Duration = Duration::from_millis(900);
const FEEDBACK_DURATION: Duration = Duration::from_millis(2600);
/// Taille minimale d'un pays à deviner, en part de la hauteur de la carte, pour qu'on le
/// reconnaisse sans zoomer (Slovénie, Belgique, Guinée-Bissau… sont en dessous sur le monde).
const TARGET_MIN_SHARE: f32 = 0.04;
/// Marges de la carte occupées par l'interface : invite (haut), boutons (droite),
/// barre de saisie (bas).
const SAFE_AREA: map::Insets = map::Insets {
    top: 90.0,
    right: 80.0,
    bottom: 150.0,
    left: 40.0,
};

/// Onglet de l'écran d'accueil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Play,
    Progress,
}

#[derive(Debug, Clone)]
pub enum Message {
    ShowTab(Tab),
    /// Choix d'une série dans l'onglet Progression : mode et zone.
    SelectSeries(Mode, usize),
    /// Demande confirmation avant de supprimer une partie (index dans l'historique).
    AskDelete(usize),
    CancelDelete,
    /// Supprime une partie (index dans l'historique), après confirmation.
    DeleteRun(usize),
    UndoDelete,
    /// Quitte la partie terminée pour voir la progression de sa série.
    ShowProgress,
    SelectMode(Mode),
    /// Choix du quiz (index dans `ATLASES`).
    SelectAtlas(usize),
    SelectRegion(usize),
    ToggleTimer,
    /// Avancement affiché par continent (`true`) ou par sous-région.
    ByContinent(bool),
    /// Allonge (`true`) ou raccourcit la durée du minuteur.
    StepTimer(bool),
    ResetTimer,
    Start,
    Restart,
    BackToMenu,
    Map(MapEvent),
    Input(String),
    Submit,
    Skip,
    GiveUp,
    ZoomIn,
    ZoomOut,
    ResetView,
    CloseSummary,
    /// Ferme la carte de situation d'un encart.
    CloseLocator,
    Tick(Instant),
    Voice(voice::Event),
    ToggleMic,
    DownloadModel,
    Key(keyboard::Event),
    #[cfg(debug_assertions)]
    DevTick,
    #[cfg(debug_assertions)]
    DevShot(iced::window::Screenshot),
}

pub struct App {
    /// Reconnaissance des noms, une par atlas.
    matchers: Vec<Matcher>,
    /// Atlas choisi (index dans `ATLASES`).
    atlas: usize,
    progress: Progress,
    settings: Settings,
    mode: Mode,
    region: usize,
    tab: Tab,
    /// Dernière partie supprimée et sa place dans l'historique, pour pouvoir l'annuler.
    deleted: Option<(usize, Session)>,
    /// Partie dont la suppression attend confirmation.
    confirm_delete: Option<usize>,
    game: Option<Game>,
    /// Aperçu de la zone choisie, sur l'écran d'accueil.
    preview: Preview,
    voice: Voice,
    #[cfg(debug_assertions)]
    dev_ticks: u32,
}

struct Voice {
    commands: Option<iced::futures::channel::mpsc::Sender<voice::Command>>,
    /// Le modèle vocal est installé (mis à jour après chaque téléchargement).
    installed: bool,
    state: voice::State,
    speaking: bool,
    /// Transcription provisoire de la phrase en cours.
    partial: String,
    /// Pays déjà entendus dans la phrase en cours.
    sentence: Vec<usize>,
    level: f32,
}

impl Voice {
    fn refresh_installed(&mut self) {
        self.installed = voice::installed();
    }

    fn active(&self) -> bool {
        matches!(self.state, voice::State::Listening | voice::State::Loading)
    }
}

struct Game {
    /// Atlas joué (index dans `ATLASES`) et ses données.
    atlas: usize,
    world: &'static World,
    region: usize,
    quiz: Quiz,
    projection: Projection,
    view: iced::Rectangle,
    camera: Camera,
    animation: Option<Animation>,
    /// L'utilisateur a déplacé la carte lui-même depuis le dernier cadrage automatique.
    manual_camera: bool,
    map_size: Size,
    hover: Option<map::Place>,
    cache: Cache,
    input: String,
    /// Réponse ambiguë en attente de validation.
    pending: Option<(usize, Instant)>,
    feedback: Option<Feedback>,
    flash: Option<(usize, Color, Instant, Duration)>,
    started: Instant,
    now: Instant,
    ended: Option<Instant>,
    /// Durée du minuteur, s'il est activé.
    limit: Option<Duration>,
    /// La partie s'est terminée faute de temps.
    timed_out: bool,
    /// Résumé affiché en fin de partie ; `new_record` indique un meilleur score.
    summary: bool,
    new_record: bool,
    /// Derniers pays joués (plus récent à la fin).
    history: Vec<usize>,
    /// Carte de situation d'un encart (outre-mer, Alaska…), ouverte par sa pastille.
    locator: Option<Locator>,
}

/// Petite carte du monde qui situe les terres d'un encart.
struct Locator {
    inset: usize,
    projection: Projection,
    view: iced::Rectangle,
    origin: [f32; 4],
    /// Partie vide : aucun lieu mis en évidence ni nommé (pas d'indice).
    quiz: Quiz,
    cache: Cache,
}

impl Locator {
    fn new(inset: usize, origin: [f32; 4]) -> Self {
        let world = &ATLASES[0];
        let [x0, y0, x1, y1] = origin;
        let (lon, lat) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        // Assez large pour voir les terres voisines (Caraïbes, Madagascar, Californie…).
        let half_w = (1.3 * (x1 - x0)).max(45.0);
        let half_h = (1.3 * (y1 - y0)).max(25.0);
        let view = [
            lon - half_w,
            (lat - half_h).max(-80.0),
            lon + half_w,
            (lat + half_h).min(85.0),
        ];
        Self {
            inset,
            projection: Projection::new(world, lon),
            view: map::view_rect(view, lon),
            origin,
            quiz: Quiz::new(Mode::Free, world.countries.len(), Vec::new(), Vec::new()),
            cache: Cache::new(),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let map_view = MapView {
            world: &ATLASES[0],
            projection: &self.projection,
            view: self.view,
            camera: Camera::fit(self.view),
            quiz: &self.quiz,
            hover: None,
            target: None,
            flash: None,
            pulse: 0.0,
            reveal_all: false,
            clickable: false,
            preview: true,
            marker: Some(self.origin),
            cache: &self.cache,
        };
        let map: Element<'_, MapEvent> = canvas(map_view).width(Fill).height(Fill).into();
        let close = button(container(icon(Icon::Close, 12.0, style::TEXT)).center(Fill))
            .width(28)
            .height(28)
            .padding(0)
            .style(style::map_control)
            .on_press(Message::CloseLocator);
        let card = column![
            row![
                icon(Icon::Globe, 16.0, style::TEXT),
                text("Sur la carte du monde")
                    .size(14)
                    .font(style::FONT_SEMIBOLD)
                    .width(Fill),
                close,
            ]
            .spacing(8)
            .align_y(Center),
            container(map.map(Message::Map))
                .width(Fill)
                .height(210)
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
        ]
        .spacing(10);
        container(card)
            .padding(14)
            .width(360)
            .style(|theme: &Theme| container::Style {
                background: Some(style::SURFACE.into()),
                ..style::floating(theme)
            })
            .into()
    }
}

/// Carte de l'écran d'accueil : la zone choisie, ses pays mis en évidence.
struct Preview {
    atlas: usize,
    world: &'static World,
    region: usize,
    projection: Projection,
    view: iced::Rectangle,
    /// Partie fictive dont les pays « à trouver » sont ceux de la zone.
    quiz: Quiz,
    cache: Cache,
}

impl Preview {
    fn new(atlas: usize, region: usize) -> Self {
        let world = &ATLASES[atlas];
        let zone = &world.regions[region];
        let members = zone.members.clone();
        Self {
            atlas,
            world,
            region,
            projection: Projection::new(world, zone.center),
            view: map::view_rect(zone.view, zone.center),
            quiz: Quiz::new(Mode::Free, world.countries.len(), members, Vec::new()),
            cache: Cache::new(),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let map_view = MapView {
            world: self.world,
            projection: &self.projection,
            view: self.view,
            camera: Camera::fit(self.view),
            quiz: &self.quiz,
            hover: None,
            target: None,
            flash: None,
            pulse: 0.0,
            reveal_all: false,
            clickable: false,
            preview: true,
            marker: None,
            cache: &self.cache,
        };
        let map: Element<'_, MapEvent> = canvas(map_view).width(Fill).height(Fill).into();
        container(map.map(Message::Map))
            .width(Fill)
            .height(200)
            .padding(2)
            .clip(true)
            .style(|_: &Theme| container::Style {
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
}

struct Animation {
    from: Camera,
    to: Camera,
    start: Instant,
}

struct Feedback {
    text: String,
    color: Color,
    at: Instant,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let app = Self {
            matchers: ATLASES.iter().map(Matcher::new).collect(),
            progress: Progress::load(),
            settings: Settings::load(),
            mode: Mode::Free,
            atlas: 0,
            region: 0,
            tab: Tab::Play,
            deleted: None,
            confirm_delete: None,
            game: None,
            preview: Preview::new(0, 0),
            voice: Voice {
                commands: None,
                installed: false,
                state: voice::State::Idle,
                speaking: false,
                partial: String::new(),
                sentence: Vec::new(),
                level: 0.0,
            },
            #[cfg(debug_assertions)]
            dev_ticks: 0,
        };
        let mut app = app;
        app.voice.refresh_installed();
        #[cfg(debug_assertions)]
        {
            // Saisie simulée (`type=…` dans GEO_DEMO), par le même chemin que le clavier.
            let demo = crate::dev::demo().unwrap_or_default();
            let app = app.with_demo();
            let mut task = demo
                .typed
                .into_iter()
                .fold(operation::focus(ANSWER_INPUT), |task, t| {
                    task.chain(Task::done(Message::Input(t)))
                });
            if demo.bottom {
                task = task.chain(operation::snap_to_end(HOME_SCROLL));
            }
            (app, task)
        }
        #[cfg(not(debug_assertions))]
        (app, Task::none())
    }

    #[cfg(debug_assertions)]
    fn with_demo(mut self) -> Self {
        let Some(demo) = crate::dev::demo() else {
            return self;
        };
        if let Some(atlas) = demo
            .atlas
            .as_deref()
            .and_then(|id| ATLASES.iter().position(|a| a.info.id == id))
        {
            self.atlas = atlas;
            self.preview = Preview::new(atlas, 0);
        }
        // Les parties de démonstration ne sont pas enregistrées.
        self.progress = Progress::default();
        if let Some(mode) = demo.mode {
            self.mode = mode;
        }
        if let Some(region) = demo.region.as_deref().and_then(|r| self.world().region(r)) {
            self.region = region;
            self.preview = Preview::new(self.atlas, region);
        }
        // En mémoire seulement, comme le minuteur ci-dessous.
        self.settings.by_continent = demo.by_continent;
        if demo.timer.is_some() {
            // En mémoire seulement : les réglages ne sont enregistrés que sur action.
            self.settings.timer = true;
        }
        if let Some(count) = demo.progress {
            self.tab = Tab::Progress;
            let world = self.world();
            let zone = &world.regions[self.region];
            let id = &world.info.id;
            for (mode, region, n) in [
                (Mode::Identify, 0, 3),
                (Mode::Free, 0, 2),
                (Mode::Locate, 1, 1),
            ] {
                let r = &world.regions[region];
                let runs = crate::dev::fake_runs(id, mode, &r.id, r.members.len() as u32, n);
                self.progress.sessions.extend(runs);
            }
            let runs =
                crate::dev::fake_runs(id, Mode::Free, &zone.id, zone.members.len() as u32, count);
            self.progress.sessions.extend(runs);
            if demo.delete {
                let last = self.progress.sessions.len() - 1;
                let _ = self.update(Message::AskDelete(last));
                let _ = self.update(Message::DeleteRun(last));
            }
            if demo.confirm {
                let last = self.progress.sessions.len() - 1;
                let _ = self.update(Message::AskDelete(last));
            }
            return self;
        }
        if demo.mode.is_none() {
            return self;
        }
        let mut game = Game::new(self.atlas, self.region, self.mode, self.time_limit());
        if let Some(first) = demo
            .target
            .as_deref()
            .and_then(|code| self.world().countries.iter().position(|c| c.code == code))
        {
            let members = self.world().regions[self.region].members.clone();
            let mut order: Vec<usize> = members.iter().copied().filter(|&c| c != first).collect();
            order.insert(0, first);
            game.quiz = Quiz::new(self.mode, self.world().countries.len(), members, order);
        }
        for code in &demo.answers {
            if let Some(c) = self.world().countries.iter().position(|c| &c.code == code) {
                let outcome = game.quiz.submit(c);
                game.apply(outcome);
            }
        }
        if let Some((lon, lat, zoom)) = demo.zoom {
            let [x, y] =
                geo_core::projection::project(lon, lat, self.world().regions[self.region].center);
            game.camera = Camera {
                center: iced::Point::new(x, y),
                zoom,
            };
        }
        if let Some(input) = demo.input {
            game.input = input;
        }
        if demo.listening {
            self.voice.state = voice::State::Listening;
            self.voice.speaking = true;
            self.voice.level = 0.4;
        }
        if let Some(seconds) = demo.timer {
            game.limit = Some(Duration::from_secs(seconds));
        }
        if let Some(seconds) = demo.elapsed {
            game.started -= Duration::from_secs(seconds);
        }
        if demo.give_up {
            game.quiz.give_up();
        }
        if let Some(inset) = demo
            .globe
            .and_then(|i| self.world().info.insets.get(i).map(|x| (i, x)))
        {
            game.locator = Some(Locator::new(inset.0, inset.1.origin));
        }
        game.flash = None;
        self.game = Some(game);
        self.check_finished();
        if let Some(game) = &mut self.game {
            game.summary = demo.summary;
        }
        self
    }

    /// Atlas choisi sur l'écran d'accueil.
    fn world(&self) -> &'static World {
        &ATLASES[self.atlas]
    }

    /// Reconstruit l'aperçu si l'atlas ou la zone ont changé.
    fn refresh_preview(&mut self) {
        if self.preview.atlas != self.atlas || self.preview.region != self.region {
            self.preview = Preview::new(self.atlas, self.region);
        }
    }

    pub fn title(&self) -> String {
        match &self.game {
            Some(game) => format!(
                "{} · {} — GeoLearner",
                game.world.regions[game.region].name,
                mode_title(game.quiz.mode(), &Words::of(&game.world.info))
            ),
            None => "GeoLearner".into(),
        }
    }

    pub fn theme(&self) -> Theme {
        style::theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            voice::subscription().map(Message::Voice),
            keyboard::listen().map(Message::Key),
        ];
        #[cfg(debug_assertions)]
        if crate::dev::shot_path().is_some() {
            subs.push(time::every(Duration::from_millis(400)).map(|_| Message::DevTick));
        }
        if let Some(game) = &self.game {
            // Les mouvements de caméra sont fluides (60 images par seconde) ; les effets plus
            // légers (pulsation, flash) se contentent d'environ 25 images par seconde.
            let effects = game.flash.is_some()
                || game.pending.is_some()
                || (game.quiz.mode() == Mode::Identify && game.quiz.target().is_some());
            if game.animation.is_some() {
                subs.push(time::every(Duration::from_millis(16)).map(Message::Tick));
            } else if effects {
                subs.push(time::every(Duration::from_millis(40)).map(Message::Tick));
            } else if game.ended.is_none() || game.feedback.is_some() {
                subs.push(time::every(Duration::from_millis(250)).map(Message::Tick));
            }
        }
        Subscription::batch(subs)
    }

    // --- Mise à jour ---

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ShowTab(tab) => {
                self.tab = tab;
                self.deleted = None;
                self.confirm_delete = None;
            }
            Message::SelectSeries(mode, region) => {
                self.mode = mode;
                self.region = region;
                self.deleted = None;
                self.confirm_delete = None;
                self.refresh_preview();
            }
            Message::AskDelete(index) => self.confirm_delete = Some(index),
            Message::CancelDelete => self.confirm_delete = None,
            Message::DeleteRun(index) => {
                if self.confirm_delete.take() == Some(index)
                    && let Some(session) = self.progress.remove(index)
                {
                    self.deleted = Some((index, session));
                }
            }
            Message::UndoDelete => {
                self.confirm_delete = None;
                if let Some((index, session)) = self.deleted.take() {
                    self.progress.restore(index, session);
                }
            }
            Message::ShowProgress => {
                if let Some(game) = self.game.take() {
                    self.atlas = game.atlas;
                    self.region = game.region;
                    self.mode = game.quiz.mode();
                }
                self.tab = Tab::Progress;
                self.deleted = None;
                self.confirm_delete = None;
                self.refresh_preview();
                self.send_voice(voice::Command::Stop);
            }
            Message::SelectMode(mode) => self.mode = mode,
            Message::SelectAtlas(atlas) => {
                if atlas != self.atlas {
                    self.atlas = atlas;
                    self.region = 0;
                    self.deleted = None;
                    self.confirm_delete = None;
                    self.refresh_preview();
                }
            }
            Message::SelectRegion(region) => {
                self.region = region;
                self.refresh_preview();
            }
            Message::ToggleTimer => self.settings.toggle_timer(),
            Message::ByContinent(by_continent) => self.settings.set_by_continent(by_continent),
            Message::StepTimer(up) => {
                let zone = &self.world().regions[self.region];
                self.settings.step_duration(
                    &self.world().info.id,
                    self.mode,
                    &zone.id,
                    zone.members.len(),
                    up,
                );
            }
            Message::ResetTimer => {
                let zone = &self.world().regions[self.region];
                self.settings
                    .reset_duration(&self.world().info.id, self.mode, &zone.id);
            }
            Message::Start | Message::Restart => {
                if let Some(game) = &self.game {
                    self.atlas = game.atlas;
                    self.region = game.region;
                    self.mode = game.quiz.mode();
                }
                self.game = Some(Game::new(
                    self.atlas,
                    self.region,
                    self.mode,
                    self.time_limit(),
                ));
                return operation::focus(ANSWER_INPUT);
            }
            Message::BackToMenu => {
                self.game = None;
                self.send_voice(voice::Command::Stop);
            }
            Message::Map(event) => return self.on_map(event),
            Message::Input(value) => self.on_input(value),
            Message::Submit => self.on_submit(),
            Message::Skip => {
                if let Some(game) = &mut self.game
                    && let Some(country) = game.quiz.skip()
                {
                    let name = &game.world.countries[country].name;
                    game.say(format!("C'était : {name}"), style::DANGER);
                    game.flash(country, style::DANGER, 1800);
                    game.history.push(country);
                    game.after_answer();
                    self.check_finished();
                }
            }
            Message::GiveUp => {
                if let Some(game) = &mut self.game {
                    game.quiz.give_up();
                    game.cache.clear();
                    self.check_finished();
                }
            }
            Message::ZoomIn | Message::ZoomOut | Message::ResetView => {
                if let Some(game) = &mut self.game {
                    let to = match message {
                        Message::ZoomIn => game.camera.zoom_at(game.view, game.map_size, 1.6, None),
                        Message::ZoomOut => {
                            game.camera
                                .zoom_at(game.view, game.map_size, 1.0 / 1.6, None)
                        }
                        _ => Camera::fit(game.view),
                    };
                    game.manual_camera = !matches!(message, Message::ResetView);
                    game.animate_to(to);
                }
                return operation::focus(ANSWER_INPUT);
            }
            Message::CloseLocator => {
                if let Some(game) = &mut self.game {
                    game.locator = None;
                }
                return operation::focus(ANSWER_INPUT);
            }
            Message::CloseSummary => {
                if let Some(game) = &mut self.game {
                    game.summary = false;
                }
            }
            Message::Tick(now) => self.on_tick(now),
            Message::Voice(event) => self.on_voice(event),
            Message::ToggleMic => self.toggle_mic(),
            Message::DownloadModel => self.send_voice(voice::Command::Download),
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                use keyboard::key::Named;
                match key.as_ref() {
                    keyboard::Key::Character("m") if modifiers.command() => self.toggle_mic(),
                    keyboard::Key::Character("z") if modifiers.command() && self.game.is_none() => {
                        return self.update(Message::UndoDelete);
                    }
                    keyboard::Key::Named(Named::Escape) => {
                        self.confirm_delete = None;
                        if let Some(game) = &mut self.game {
                            game.locator = None;
                            game.summary = false;
                        }
                    }
                    _ => {}
                }
            }
            Message::Key(_) => {}
            #[cfg(debug_assertions)]
            Message::DevTick => {
                self.dev_ticks += 1;
                if self.dev_ticks == 6 {
                    return iced::window::latest()
                        .and_then(iced::window::screenshot)
                        .map(Message::DevShot);
                }
            }
            #[cfg(debug_assertions)]
            Message::DevShot(shot) => {
                if let Some(path) = crate::dev::shot_path() {
                    crate::dev::save(&shot, &path);
                }
                return iced::exit();
            }
        }
        Task::none()
    }

    fn on_map(&mut self, event: MapEvent) -> Task<Message> {
        let Some(game) = &mut self.game else {
            return Task::none();
        };
        match event {
            MapEvent::Camera(camera) => {
                game.animation = None;
                game.manual_camera = true;
                game.camera = camera;
                game.cache.clear();
            }
            MapEvent::Hover(hover) => game.hover = hover,
            MapEvent::Resized(size) => {
                let first = game.map_size == Size::ZERO;
                game.map_size = size;
                game.cache.clear();
                if first {
                    game.follow_target();
                }
            }
            MapEvent::Click(Some(country)) if game.quiz.mode() == Mode::Locate => {
                self.on_click(country);
            }
            MapEvent::Globe(inset) => {
                // Un second clic sur la même pastille referme la carte.
                game.locator = match &game.locator {
                    Some(l) if l.inset == inset => None,
                    _ => game
                        .world
                        .info
                        .insets
                        .get(inset)
                        .map(|i| Locator::new(inset, i.origin)),
                };
            }
            MapEvent::Click(_) | MapEvent::DragEnded => {}
        }
        // Un clic sur la carte retire le focus du champ de saisie : on le lui rend.
        if matches!(
            event,
            MapEvent::Click(_) | MapEvent::DragEnded | MapEvent::Globe(_)
        ) {
            operation::focus(ANSWER_INPUT)
        } else {
            Task::none()
        }
    }

    fn on_click(&mut self, country: usize) {
        let Some(game) = &mut self.game else { return };
        let name = &game.world.countries[country].name;
        match game.quiz.mark(country) {
            None => {
                game.say(
                    format!("{name} ne fait pas partie de la zone"),
                    style::MUTED,
                );
                return;
            }
            Some(Mark::Found { .. } | Mark::Missed) if game.quiz.target() != Some(country) => {
                game.flash(country, style::SURFACE_3, 900);
                return;
            }
            _ => {}
        }
        let outcome = game.quiz.submit(country);
        game.apply(outcome);
        self.check_finished();
    }

    fn on_input(&mut self, value: String) {
        let Some(game) = &mut self.game else { return };
        game.input = value;
        game.pending = None;
        let Some(country) = self.matchers[game.atlas].lookup(&game.input) else {
            return;
        };
        match game.quiz.mode() {
            Mode::Free if game.quiz.mark(country) == Some(Mark::Pending) => {
                if is_prefix_of_other(game.world, &game.input, country, &game.quiz) {
                    game.pending = Some((country, game.now));
                } else {
                    let outcome = game.quiz.submit(country);
                    game.apply(outcome);
                }
            }
            Mode::Identify if game.quiz.target() == Some(country) => {
                let outcome = game.quiz.submit(country);
                game.apply(outcome);
            }
            _ => {}
        }
        self.check_finished();
    }

    fn on_submit(&mut self) {
        let Some(game) = &mut self.game else { return };
        if game.input.trim().is_empty() {
            return;
        }
        let Some(country) = self.matchers[game.atlas].lookup(&game.input) else {
            let info = &game.world.info;
            let words = Words::of(info);
            let typed = geo_core::normalize(&game.input);
            let message = match game
                .world
                .territories
                .iter()
                .find(|t| t.answers.contains(&typed))
            {
                // Sans dire à quel pays il appartient : ce serait un indice.
                Some(territory) => format!("{} n'est pas {}", territory.name, words.a()),
                None => format!(
                    "« {} » : {} {}",
                    game.input.trim(),
                    info.one,
                    words.agree("inconnu")
                ),
            };
            game.say(message, style::MUTED);
            return;
        };
        let name = &game.world.countries[country].name;
        match game.quiz.mode() {
            Mode::Free => match game.quiz.mark(country) {
                Some(Mark::Pending) => {
                    let outcome = game.quiz.submit(country);
                    game.apply(outcome);
                }
                Some(_) => {
                    game.say(format!("Déjà trouvé : {name}"), style::WARNING);
                    game.flash(country, style::WARNING, 900);
                    game.input.clear();
                }
                None => game.say(
                    format!("{name} ne fait pas partie de la zone"),
                    style::MUTED,
                ),
            },
            Mode::Identify => {
                let outcome = game.quiz.submit(country);
                game.apply(outcome);
            }
            Mode::Locate => {}
        }
        game.pending = None;
        self.check_finished();
    }

    fn on_tick(&mut self, now: Instant) {
        let Some(game) = &mut self.game else { return };
        // Captures de développement : chronomètre figé pour un rendu reproductible.
        #[cfg(debug_assertions)]
        if crate::dev::shot_path().is_some() {
            game.started += now - game.now;
        }
        game.now = now;
        if let Some(animation) = &game.animation {
            let t = ((now - animation.start).as_secs_f32() / 0.45).min(1.0);
            let eased = 1.0 - (1.0 - t).powi(3);
            game.camera = animation.from.lerp(animation.to, eased);
            game.cache.clear();
            if t >= 1.0 {
                game.animation = None;
            }
        }
        if game.flash.is_some_and(|(_, _, at, d)| now - at > d) {
            game.flash = None;
        }
        if game
            .feedback
            .as_ref()
            .is_some_and(|f| now - f.at > FEEDBACK_DURATION)
        {
            game.feedback = None;
        }
        // Minuteur écoulé : la partie est validée telle quelle.
        if let Some(limit) = game.limit
            && game.ended.is_none()
            && now.saturating_duration_since(game.started) >= limit
        {
            game.timed_out = true;
            game.quiz.give_up();
            game.pending = None;
            self.check_finished();
            return;
        }
        if let Some((country, at)) = game.pending
            && now - at > AMBIGUOUS_DELAY
        {
            game.pending = None;
            if game.quiz.mark(country) == Some(Mark::Pending) {
                let outcome = game.quiz.submit(country);
                game.apply(outcome);
                self.check_finished();
            }
        }
    }

    fn on_voice(&mut self, event: voice::Event) {
        match event {
            voice::Event::Ready(sender) => self.voice.commands = Some(sender),
            voice::Event::State(state) => {
                if let voice::State::Error(e) = &state {
                    eprintln!("{e}");
                }
                if state == voice::State::Idle {
                    self.voice.refresh_installed();
                }
                self.voice.state = state;
            }
            voice::Event::Speaking(speaking) => self.voice.speaking = speaking,
            voice::Event::Partial(text) => self.voice.partial = text,
            voice::Event::Level(level) => self.voice.level = level,
            voice::Event::Heard {
                countries,
                text,
                done,
            } => self.on_heard(countries, &text, done),
        }
    }

    /// Pays reconnus à la voix, au fil de la phrase (`done` : phrase terminée).
    fn on_heard(&mut self, countries: Vec<usize>, text: &str, done: bool) {
        let Some(game) = &mut self.game else { return };
        let earlier = self.voice.sentence.len();
        self.voice.sentence.extend(&countries);
        match game.quiz.mode() {
            Mode::Free => {
                for country in countries {
                    let outcome = game.quiz.submit(country);
                    game.apply(outcome);
                }
            }
            // Une mauvaise réponse n'est comptée qu'en fin de phrase : la transcription
            // provisoire peut encore se corriger.
            Mode::Identify => match game.quiz.target() {
                Some(target) if countries.contains(&target) => {
                    let outcome = game.quiz.submit(target);
                    game.apply(outcome);
                    self.voice.sentence.clear();
                }
                _ if done => {
                    if let Some(&guess) = self.voice.sentence.first() {
                        let outcome = game.quiz.submit(guess);
                        game.apply(outcome);
                    }
                }
                _ => {}
            },
            Mode::Locate if !countries.is_empty() => {
                game.say("Dans ce mode, il faut cliquer sur la carte", style::MUTED)
            }
            Mode::Locate => {}
        }
        if done {
            if earlier == 0 && self.voice.sentence.is_empty() && !text.trim().is_empty() {
                game.say(format!("Entendu : « {} »", text.trim()), style::MUTED);
            }
            self.voice.sentence.clear();
        }
        self.check_finished();
    }

    fn toggle_mic(&mut self) {
        if self.voice.active() {
            self.send_voice(voice::Command::Stop);
        } else if matches!(self.voice.state, voice::State::Downloading(..)) {
        } else if !self.voice.installed {
            self.send_voice(voice::Command::Download);
        } else if self.game.is_some() {
            self.send_voice(voice::Command::Start(
                self.game.as_ref().map_or(self.atlas, |g| g.atlas),
            ));
        }
    }

    fn send_voice(&mut self, command: voice::Command) {
        if let Some(sender) = &mut self.voice.commands {
            let _ = sender.try_send(command);
        }
    }

    /// Enregistre la partie dès qu'elle est terminée.
    fn check_finished(&mut self) {
        let Some(game) = &mut self.game else { return };
        if !game.quiz.is_finished() || game.ended.is_some() {
            return;
        }
        game.ended = Some(game.now);
        game.summary = true;
        game.feedback = None;
        game.cache.clear();
        let quiz = &game.quiz;
        let region = &game.world.regions[game.region];
        let session = Session {
            atlas: game.world.info.id.clone(),
            when: progress::now(),
            mode: quiz.mode(),
            region: region.id.clone(),
            total: quiz.total() as u32,
            found: quiz.found() as u32,
            perfect: quiz.perfect() as u32,
            score: quiz.score(),
            seconds: game.elapsed().as_secs() as u32,
            missed: quiz
                .members()
                .iter()
                .filter(|&&c| quiz.mark(c) == Some(Mark::Missed))
                .map(|&c| game.world.countries[c].code.clone())
                .collect(),
            limit: game.limit.map(|d| d.as_secs() as u32),
        };
        game.new_record = match self
            .progress
            .best(&session.atlas, session.mode, &session.region)
        {
            Some(best) => {
                session.score > best.score
                    || (session.score == best.score && session.seconds < best.seconds)
            }
            None => session.found > 0,
        };
        self.progress.record(session);
        self.send_voice(voice::Command::Stop);
    }

    // --- Vues ---

    pub fn view(&self) -> Element<'_, Message> {
        match &self.game {
            Some(game) => self.game_view(game),
            None => self.home_view(),
        }
    }

    fn home_view(&self) -> Element<'_, Message> {
        let (main, side) = match self.tab {
            Tab::Play => (
                self.play_main(),
                column![self.preview_card(), self.voice_card(), self.history_card()]
                    .spacing(16)
                    .into(),
            ),
            Tab::Progress => (self.progress_main(), self.progress_side()),
        };
        let main = column![self.home_header(), main].spacing(24).width(Fill);
        let side = container(side).width(320);
        let content = container(row![self.catalog(), main, side].spacing(28))
            .padding(32)
            .max_width(1440);
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

    /// Titre du quiz choisi et onglets Jouer / Progression.
    fn home_header(&self) -> Element<'_, Message> {
        let info = &self.world().info;
        let tab = |tab: Tab, i: Icon, label: &'static str| {
            let on = self.tab == tab;
            button(
                row![
                    icon(i, 14.0, if on { Color::WHITE } else { style::TEXT }),
                    text(label).size(14)
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding([8, 16])
            .style(style::pill(on))
            .on_press(Message::ShowTab(tab))
        };
        row![
            column![
                text(info.title.clone()).size(26).font(style::FONT_BOLD),
                text(format!("{} {}", self.world().countries.len(), info.many))
                    .size(14)
                    .color(style::MUTED),
            ]
            .spacing(2)
            .width(Fill),
            container(
                row![
                    tab(Tab::Play, Icon::Play, "Jouer"),
                    tab(Tab::Progress, Icon::Chart, "Progression"),
                ]
                .spacing(4)
            )
            .padding(4)
            .style(|_: &Theme| container::Style {
                background: Some(style::SURFACE.into()),
                border: iced::Border {
                    color: style::BORDER,
                    width: 1.0,
                    radius: 999.0.into(),
                },
                ..Default::default()
            }),
        ]
        .align_y(Center)
        .into()
    }

    /// Onglet Jouer : mode, zone et lancement.
    fn play_main(&self) -> Element<'_, Message> {
        let words = Words::of(&self.world().info);
        let modes = Row::with_children(Mode::ALL.iter().map(|&mode| {
            button(
                column![
                    text(mode_title(mode, &words))
                        .size(16)
                        .font(style::FONT_SEMIBOLD),
                    text(mode_description(mode, &words))
                        .size(13)
                        .color(style::MUTED),
                ]
                .spacing(6),
            )
            .padding(16)
            .width(Fill)
            .height(124)
            .style(style::option_card(self.mode == mode))
            .on_press(Message::SelectMode(mode))
            .into()
        }))
        .spacing(12);

        let selected = &self.world().regions[self.region];
        let open_continent = match selected.kind {
            RegionKind::Continent => Some(self.region),
            RegionKind::Sub | RegionKind::Group => selected.parent,
            RegionKind::World => None,
        };
        let pill = |index: usize, label: &str| -> Element<'_, Message> {
            button(text(label.to_owned()).size(14))
                .padding([8, 16])
                .style(style::pill(self.region == index))
                .on_press(Message::SelectRegion(index))
                .into()
        };
        let top = Row::with_children(
            self.world()
                .regions
                .iter()
                .enumerate()
                .filter(|(_, r)| matches!(r.kind, RegionKind::World | RegionKind::Continent))
                .map(|(i, r)| pill(i, &r.name)),
        )
        .spacing(8)
        .wrap();
        let mut zones = column![section_title("Zone"), top].spacing(12);
        if let Some(continent) = open_continent {
            let subs = self
                .world()
                .regions
                .iter()
                .enumerate()
                .filter(|(_, r)| r.parent == Some(continent) && !r.hidden)
                .map(|(i, r)| pill(i, &r.name));
            let all = pill(continent, "Toute la zone");
            if subs.clone().count() > 0 {
                zones = zones.push(
                    container(
                        Row::with_children(std::iter::once(all).chain(subs))
                            .spacing(8)
                            .wrap(),
                    )
                    .padding([12, 14])
                    .style(|_: &Theme| container::Style {
                        background: Some(style::with_alpha(style::SURFACE_2, 0.5).into()),
                        border: iced::Border {
                            radius: 12.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                );
            }
        }

        let best = self
            .progress
            .best(&self.world().info.id, self.mode, &selected.id)
            .map(|s| {
                format!(
                    "Record : {} en {}",
                    percent(s.score),
                    clock(Duration::from_secs(u64::from(s.seconds)))
                )
            });
        let launch = container(
            row![
                column![
                    text(selected.name.clone()).size(22).font(style::FONT_BOLD),
                    text(format!(
                        "{} · {} {}",
                        mode_title(self.mode, &words),
                        selected.members.len(),
                        words.many()
                    ))
                    .size(14)
                    .color(style::MUTED),
                    text(best.unwrap_or_else(|| "Pas encore de record".into()))
                        .size(13)
                        .color(style::MUTED),
                    self.timer_setting(),
                ]
                .spacing(4)
                .width(Fill),
                button(
                    row![
                        icon(Icon::Play, 16.0, Color::WHITE),
                        text("Commencer").size(16).font(style::FONT_SEMIBOLD)
                    ]
                    .spacing(10)
                    .align_y(Center)
                )
                .padding([14, 26])
                .style(style::primary)
                .on_press(Message::Start),
            ]
            .align_y(Center)
            .spacing(16),
        )
        .padding(22)
        .style(style::card);

        column![
            column![section_title("Mode de jeu"), modes].spacing(12),
            zones,
            launch,
        ]
        .spacing(24)
        .into()
    }

    /// Réglage du minuteur pour le mode et la zone choisis.
    fn timer_setting(&self) -> Element<'_, Message> {
        let zone = &self.world().regions[self.region];
        let on = self.settings.timer;
        let fg = if on { Color::WHITE } else { style::TEXT };
        let mut setting = row![
            button(
                row![icon(Icon::Timer, 14.0, fg), text("Minuteur").size(13)]
                    .spacing(6)
                    .align_y(Center)
            )
            .padding([6, 12])
            .style(style::pill(on))
            .on_press(Message::ToggleTimer),
        ]
        .spacing(8)
        .align_y(Center);
        if on {
            let seconds = self.settings.duration(
                &self.world().info.id,
                self.mode,
                &zone.id,
                zone.members.len(),
            );
            let step = |i: Icon, message: Message| {
                button(container(icon(i, 12.0, style::TEXT)).center(Fill))
                    .width(26)
                    .height(26)
                    .padding(0)
                    .style(style::map_control)
                    .on_press(message)
            };
            setting = setting
                .push(step(Icon::Minus, Message::StepTimer(false)))
                .push(
                    text(clock(Duration::from_secs(u64::from(seconds))))
                        .size(15)
                        .font(style::FONT_SEMIBOLD),
                )
                .push(step(Icon::Plus, Message::StepTimer(true)));
            if self
                .settings
                .is_custom(&self.world().info.id, self.mode, &zone.id)
            {
                setting = setting.push(
                    button(text("Par défaut").size(12))
                        .padding([4, 8])
                        .style(style::ghost)
                        .on_press(Message::ResetTimer),
                );
            }
        } else {
            setting = setting.push(text("désactivé").size(13).color(style::MUTED));
        }
        container(setting)
            .padding(iced::Padding::ZERO.top(8))
            .into()
    }

    /// Catalogue des quiz, groupés par catégorie (dans l'ordre de `ATLASES`).
    fn catalog(&self) -> Element<'_, Message> {
        let mut list = column![
            text("GeoLearner").size(26).font(style::FONT_BOLD),
            text("La géographie, carte en main.")
                .size(13)
                .color(style::MUTED),
        ]
        .spacing(4);
        let mut categories: Vec<&str> = Vec::new();
        for atlas in ATLASES.iter() {
            if !categories.contains(&atlas.info.category.as_str()) {
                categories.push(&atlas.info.category);
            }
        }
        for category in categories {
            let mut group = column![section_title(category)].spacing(6);
            for (i, atlas) in ATLASES
                .iter()
                .enumerate()
                .filter(|(_, a)| a.info.category == category)
            {
                group = group.push(
                    button(
                        column![
                            text(atlas.info.title.clone())
                                .size(14)
                                .font(style::FONT_SEMIBOLD),
                            text(format!("{} {}", atlas.countries.len(), atlas.info.many))
                                .size(12)
                                .color(style::MUTED),
                        ]
                        .spacing(2),
                    )
                    .padding([9, 12])
                    .width(Fill)
                    .style(style::option_card(self.atlas == i))
                    .on_press(Message::SelectAtlas(i)),
                );
            }
            list = list.push(container(group).padding(iced::Padding::ZERO.top(14)));
        }
        container(list).width(230).into()
    }

    /// Aperçu de la zone choisie : ses pays en bleu sur la carte.
    fn preview_card(&self) -> Element<'_, Message> {
        let zone = &self.world().regions[self.region];
        container(
            column![
                row![
                    text("Aperçu")
                        .size(16)
                        .font(style::FONT_SEMIBOLD)
                        .width(Fill),
                    text(format!("{} {}", zone.members.len(), self.world().info.many))
                        .size(13)
                        .color(style::MUTED),
                ]
                .align_y(Center),
                self.preview.view(),
            ]
            .spacing(12),
        )
        .padding(18)
        .width(Fill)
        .style(style::card)
        .into()
    }

    fn voice_card(&self) -> Element<'_, Message> {
        let (status, color) = match &self.voice.state {
            voice::State::Downloading(p) => {
                (format!("Téléchargement… {}", percent(*p)), style::MUTED)
            }
            voice::State::Error(e) => (e.clone(), style::DANGER),
            voice::State::Listening => ("Écoute en cours.".into(), style::SUCCESS),
            _ if !self.voice.installed => (
                format!(
                    "Le modèle de reconnaissance française ({} Mo) doit être téléchargé une fois. Tout se passe ensuite hors ligne.",
                    voice::MODEL_SIZE_MB
                ),
                style::MUTED,
            ),
            _ => (
                "Prête. En partie, cliquez sur le micro (ou Ctrl+M) et citez les noms à la suite."
                    .into(),
                style::MUTED,
            ),
        };
        let mut body = column![
            row![
                icon(Icon::Mic, 18.0, style::TEXT),
                text("Saisie vocale").size(16).font(style::FONT_SEMIBOLD)
            ]
            .spacing(8)
            .align_y(Center),
            text(status).size(13).color(color),
        ]
        .spacing(10);
        match &self.voice.state {
            voice::State::Downloading(p) => {
                body = body.push(
                    progress_bar(0.0..=1.0, *p)
                        .girth(6)
                        .style(style::progress(style::ACCENT)),
                );
            }
            _ if !self.voice.installed => {
                body = body.push(
                    button(
                        row![
                            icon(Icon::Download, 15.0, style::TEXT),
                            text(format!(
                                "Télécharger le modèle ({} Mo)",
                                voice::MODEL_SIZE_MB
                            ))
                            .size(14)
                        ]
                        .spacing(8)
                        .align_y(Center),
                    )
                    .padding([9, 14])
                    .style(style::secondary)
                    .on_press(Message::DownloadModel),
                );
            }
            _ => {}
        }
        container(body)
            .padding(18)
            .width(Fill)
            .style(style::card)
            .into()
    }

    fn history_card(&self) -> Element<'_, Message> {
        let mut list = Column::new().spacing(10);
        let mut empty = true;
        for session in self.progress.recent(6) {
            empty = false;
            let atlas = ATLASES
                .iter()
                .find(|a| a.info.id == session.atlas)
                .unwrap_or(&ATLASES[0]);
            let region = atlas
                .region(&session.region)
                .map(|i| atlas.regions[i].name.clone())
                .unwrap_or_else(|| session.region.clone());
            list = list.push(
                row![
                    container(space())
                        .width(8)
                        .height(8)
                        .style(style::dot(score_color(session.score))),
                    column![
                        text(region).size(14),
                        text(if atlas.info.id == "monde" {
                            mode_title(session.mode, &Words::of(&atlas.info))
                        } else {
                            format!(
                                "{} · {}",
                                atlas.info.title,
                                mode_title(session.mode, &Words::of(&atlas.info))
                            )
                        })
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
                .align_y(Center),
            );
        }
        if empty {
            list = list.push(
                text("Aucune partie pour l'instant.")
                    .size(13)
                    .color(style::MUTED),
            );
        }

        let mut body = column![
            text("Dernières parties")
                .size(16)
                .font(style::FONT_SEMIBOLD),
            list
        ]
        .spacing(14);
        let id = &self.world().info.id;
        let missed = self.progress.most_missed(|s| &s.atlas == id, 10);
        if !missed.is_empty() {
            let names = missed
                .iter()
                .filter_map(|(code, _)| self.world().countries.iter().find(|c| &c.code == code))
                .map(|c| {
                    container(text(c.name.clone()).size(12))
                        .padding([4, 10])
                        .style(style::chip(style::ORANGE))
                        .into()
                });
            body = body.push(rule::horizontal(1).style(|_: &Theme| rule::Style {
                color: style::BORDER,
                radius: 0.0.into(),
                fill_mode: rule::FillMode::Full,
                snap: true,
            }));
            body = body.push(text("À revoir").size(14).font(style::FONT_SEMIBOLD));
            body = body.push(
                Row::with_children(names)
                    .spacing(6)
                    .wrap()
                    .vertical_spacing(6),
            );
        }
        container(body)
            .padding(18)
            .width(Fill)
            .style(style::card)
            .into()
    }

    fn game_view<'a>(&'a self, game: &'a Game) -> Element<'a, Message> {
        let column = column![
            self.top_bar(game),
            row![self.map_area(game), self.sidebar(game)]
        ];
        container(column)
            .style(style::page)
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Durée du minuteur pour le mode et la zone choisis, s'il est activé.
    fn time_limit(&self) -> Option<Duration> {
        let zone = &self.world().regions[self.region];
        self.settings.timer.then(|| {
            Duration::from_secs(u64::from(self.settings.duration(
                &self.world().info.id,
                self.mode,
                &zone.id,
                zone.members.len(),
            )))
        })
    }

    /// Chronomètre, ou compte à rebours qui vire à l'orange puis au rouge.
    fn clock_chip<'a>(&self, game: &Game) -> Element<'a, Message> {
        let (value, color) = match game.remaining() {
            Some(left) if !game.quiz.is_finished() => {
                // Arrondi à la seconde supérieure : « 00:00 » coïncide avec la fin.
                let shown =
                    Duration::from_secs(left.as_secs() + u64::from(left.subsec_nanos() > 0));
                let color = match left.as_secs() {
                    0..10 => style::DANGER,
                    10..60 => style::ORANGE,
                    _ => style::ACCENT,
                };
                (shown, color)
            }
            _ => (game.elapsed(), style::ACCENT),
        };
        let mut content = row![].spacing(6).align_y(Center);
        if game.limit.is_some() {
            content = content.push(icon(Icon::Timer, 14.0, style::lighten(color, 0.25)));
        }
        content = content.push(text(clock(value)).size(16).font(style::FONT_SEMIBOLD));
        container(content)
            .padding([6, 12])
            .style(style::chip(color))
            .into()
    }

    fn top_bar<'a>(&'a self, game: &'a Game) -> Element<'a, Message> {
        let quiz = &game.quiz;
        let region = &game.world.regions[game.region];
        let counted = if quiz.mode() == Mode::Free {
            quiz.found()
        } else {
            quiz.done()
        };

        let mut actions = Row::new().spacing(8).align_y(Center);
        if quiz.is_finished() {
            actions = actions.push(
                button(
                    row![
                        icon(Icon::Refresh, 15.0, Color::WHITE),
                        text("Rejouer").size(14)
                    ]
                    .spacing(8)
                    .align_y(Center),
                )
                .padding([9, 16])
                .style(style::primary)
                .on_press(Message::Restart),
            );
        } else {
            if quiz.mode() != Mode::Free {
                actions = actions.push(
                    button(
                        row![icon(Icon::Skip, 14.0, style::TEXT), text("Passer").size(14)]
                            .spacing(8)
                            .align_y(Center),
                    )
                    .padding([9, 14])
                    .style(style::secondary)
                    .on_press(Message::Skip),
                );
            }
            actions = actions.push(
                button(
                    row![
                        icon(Icon::Flag, 14.0, style::lighten(style::DANGER, 0.2)),
                        text("Abandonner").size(14)
                    ]
                    .spacing(8)
                    .align_y(Center),
                )
                .padding([9, 14])
                .style(style::danger_soft)
                .on_press(Message::GiveUp),
            );
        }

        let bar = row![
            button(
                row![icon(Icon::Back, 16.0, style::TEXT), text("Menu").size(14)]
                    .spacing(8)
                    .align_y(Center)
            )
            .padding([8, 12])
            .style(style::ghost)
            .on_press(Message::BackToMenu),
            column![
                text(region.name.clone())
                    .size(16)
                    .font(style::FONT_SEMIBOLD),
                text(mode_title(quiz.mode(), &Words::of(&game.world.info)))
                    .size(12)
                    .color(style::MUTED),
            ]
            .spacing(2),
            space::horizontal(),
            column![
                text(format!("{counted} / {}", quiz.total()))
                    .size(15)
                    .font(style::FONT_SEMIBOLD),
                progress_bar(0.0..=quiz.total() as f32, counted as f32)
                    .length(150)
                    .girth(5)
                    .style(style::progress(style::SUCCESS)),
            ]
            .spacing(6)
            .align_x(Horizontal::Right),
            self.clock_chip(game),
            actions,
        ]
        .spacing(18)
        .align_y(Center)
        .padding([10, 16]);
        container(bar).width(Fill).style(style::top_bar).into()
    }

    fn map_area<'a>(&'a self, game: &'a Game) -> Element<'a, Message> {
        let quiz = &game.quiz;
        let pulse = ((game.now - game.started).as_secs_f32() / 1.3).fract();
        let flash = game.flash.map(|(country, color, at, duration)| {
            let t = (game.now - at).as_secs_f32() / duration.as_secs_f32();
            Flash {
                country,
                color,
                strength: (1.0 - t.powi(3)).clamp(0.0, 1.0),
            }
        });
        let map_view = MapView {
            world: game.world,
            projection: &game.projection,
            view: game.view,
            camera: game.camera,
            quiz,
            hover: game.hover,
            target: if quiz.mode() == Mode::Identify {
                quiz.target()
            } else {
                None
            },
            flash,
            pulse,
            reveal_all: quiz.is_finished(),
            clickable: quiz.mode() == Mode::Locate && !quiz.is_finished(),
            preview: false,
            marker: None,
            cache: &game.cache,
        };
        let map: Element<'a, MapEvent> = canvas(map_view).width(Fill).height(Fill).into();
        let map = sensor(
            container(map.map(Message::Map))
                .width(Fill)
                .height(Fill)
                .style(style::map_frame),
        )
        .on_show(|size| Message::Map(MapEvent::Resized(size)))
        .on_resize(|size| Message::Map(MapEvent::Resized(size)));

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
                control(Icon::Plus, Message::ZoomIn),
                control(Icon::Minus, Message::ZoomOut),
                control(Icon::Target, Message::ResetView),
            ]
            .spacing(8),
        )
        .width(Fill)
        .height(Fill)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Bottom)
        .padding(16);

        let mut layers = stack![map, controls];

        if let Some(prompt) = self.prompt(game) {
            layers = layers.push(
                container(prompt)
                    .width(Fill)
                    .align_x(Horizontal::Center)
                    .padding(18),
            );
        }
        if !quiz.is_finished() && quiz.mode() != Mode::Locate {
            layers = layers.push(
                container(self.answer_bar(game))
                    .width(Fill)
                    .height(Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Bottom)
                    .padding(22),
            );
        } else if let Some(feedback) = &game.feedback {
            layers = layers.push(
                container(feedback_chip(feedback))
                    .width(Fill)
                    .height(Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Bottom)
                    .padding(28),
            );
        }
        if let Some(locator) = &game.locator {
            // À droite, sous l'invite : les encarts sont à gauche (France) ou en bas à gauche
            // (États-Unis) et les boutons de zoom en bas.
            layers = layers.push(
                container(opaque(locator.view()))
                    .width(Fill)
                    .align_x(Horizontal::Right)
                    .padding(iced::Padding::from(16).top(SAFE_AREA.top)),
            );
        }
        if quiz.is_finished() && game.summary {
            layers = layers.push(self.summary(game));
        }
        layers.width(Fill).height(Fill).into()
    }

    fn prompt<'a>(&'a self, game: &'a Game) -> Option<Element<'a, Message>> {
        let quiz = &game.quiz;
        let target = quiz.target()?;
        let tries = quiz.wrong_tries();
        let mut content = match quiz.mode() {
            Mode::Locate => column![
                text("Où se trouve…").size(13).color(style::MUTED),
                text(game.world.countries[target].name.clone())
                    .size(26)
                    .font(style::FONT_BOLD),
            ],
            Mode::Identify => column![
                text({
                    let words = Words::of(&game.world.info);
                    let feminine = game.world.info.feminine;
                    format!(
                        "Quel{} est {} ?",
                        if feminine { "le" } else { "" },
                        words.this()
                    )
                })
                .size(18)
                .font(style::FONT_SEMIBOLD)
            ],
            Mode::Free => return None,
        };
        if tries > 0 {
            content = content.push(
                text(format!(
                    "Essai {} sur {}",
                    tries + 1,
                    geo_core::quiz::MAX_TRIES
                ))
                .size(12)
                .color(style::ORANGE),
            );
        }
        Some(
            container(content.align_x(Horizontal::Center).spacing(2))
                .padding([12, 28])
                .style(style::floating)
                .into(),
        )
    }

    fn answer_bar<'a>(&'a self, game: &'a Game) -> Element<'a, Message> {
        let listening = self.voice.active();
        let mic_color = if listening { Color::WHITE } else { style::TEXT };
        let mic = button(container(icon(Icon::Mic, 20.0, mic_color)).center(Fill))
            .width(46)
            .height(46)
            .padding(0)
            .style(style::mic(listening))
            .on_press(Message::ToggleMic);
        let words = Words::of(&game.world.info);
        let placeholder = match game.quiz.mode() {
            Mode::Identify => format!("Nom {} en surbrillance…", words.of_the()),
            _ => format!("Nom {}…", words.of_a()),
        };
        let flash_border = game
            .feedback
            .as_ref()
            .filter(|f| game.now - f.at < Duration::from_millis(500))
            .map(|f| f.color);
        let input = text_input(&placeholder, &game.input)
            .id(ANSWER_INPUT)
            .on_input(Message::Input)
            .on_submit(Message::Submit)
            .size(18)
            .padding([12, 16])
            .style(style::answer_input(flash_border));

        let mut body = column![row![mic, input].spacing(12).align_y(Center)].spacing(10);

        let status: Option<Element<'a, Message>> = match &self.voice.state {
            voice::State::Loading => Some(
                text("Chargement du modèle vocal…")
                    .size(13)
                    .color(style::MUTED)
                    .into(),
            ),
            voice::State::Listening => {
                let heard = if !self.voice.partial.is_empty() {
                    format!("« {} »", self.voice.partial)
                } else if self.voice.speaking {
                    "Parole détectée…".to_owned()
                } else {
                    "Je vous écoute — citez les noms à la suite".to_owned()
                };
                Some(
                    row![
                        progress_bar(0.0..=1.0, self.voice.level)
                            .length(60)
                            .girth(4)
                            .style(style::progress(style::DANGER)),
                        text(heard).size(13).color(style::MUTED),
                    ]
                    .spacing(10)
                    .align_y(Center)
                    .into(),
                )
            }
            voice::State::Downloading(p) => Some(
                row![
                    progress_bar(0.0..=1.0, *p)
                        .length(120)
                        .girth(4)
                        .style(style::progress(style::ACCENT)),
                    text(format!("Téléchargement du modèle vocal… {}", percent(*p)))
                        .size(13)
                        .color(style::MUTED),
                ]
                .spacing(10)
                .align_y(Center)
                .into(),
            ),
            voice::State::Error(e) => Some(text(e.clone()).size(13).color(style::DANGER).into()),
            _ => None,
        };
        if let Some(status) = status {
            body = body.push(status);
        }

        let card = container(body)
            .padding(12)
            .max_width(640)
            .style(style::floating);
        // La structure ne doit pas changer quand un message apparaît : iced perdrait
        // l'état du champ (et donc le focus) s'il changeait de place dans l'arbre.
        let feedback: Element<'a, Message> = match &game.feedback {
            Some(feedback) => feedback_chip(feedback),
            None => space().into(),
        };
        column![
            container(feedback).height(34).align_y(Vertical::Bottom),
            card
        ]
        .spacing(8)
        .align_x(Center)
        .into()
    }

    fn sidebar<'a>(&'a self, game: &'a Game) -> Element<'a, Message> {
        let quiz = &game.quiz;
        let mut body = Column::new().spacing(18);

        let headline = if quiz.mode() == Mode::Free {
            column![
                text(percent(quiz.found() as f32 / quiz.total().max(1) as f32))
                    .size(34)
                    .font(style::FONT_BOLD),
                text({
                    let words = Words::of(&game.world.info);
                    format!(
                        "{} {} {} sur {}",
                        quiz.found(),
                        words.many(),
                        words.agree("trouvés"),
                        quiz.total()
                    )
                })
                .size(13)
                .color(style::MUTED),
            ]
        } else {
            column![
                text(percent(quiz.score())).size(34).font(style::FONT_BOLD),
                text(format!(
                    "{} du premier coup · {} erreur{} · {} {}{}",
                    quiz.perfect(),
                    quiz.mistakes(),
                    plural(quiz.mistakes() as usize),
                    quiz.total() - quiz.done(),
                    Words::of(&game.world.info).agree("restant"),
                    plural(quiz.total() - quiz.done()),
                ))
                .size(13)
                .color(style::MUTED),
            ]
        };
        body = body.push(headline.spacing(2));

        // Avancement par continent ou par sous-région (mode libre, zones assez grandes).
        if quiz.mode() == Mode::Free {
            let in_quiz =
                |region: &&geo_core::Region| region.members.iter().any(|&c| quiz.mark(c).is_some());
            let count_of = |kind: RegionKind| {
                game.world
                    .regions
                    .iter()
                    .filter(|r| r.kind == kind && !r.hidden)
                    .filter(in_quiz)
                    .count()
            };
            let (continents, subs) = (count_of(RegionKind::Continent), count_of(RegionKind::Sub));
            // Le choix n'a de sens que si la partie couvre plusieurs continents et que les deux
            // niveaux diffèrent (pour les régions françaises, il n'y a pas de sous-niveau).
            let choice_useful = continents > 1 && subs > 1;
            let by_continent = continents > 1 && (self.settings.by_continent || subs <= 1);
            let kind = if by_continent {
                RegionKind::Continent
            } else {
                RegionKind::Sub
            };
            let mut rows = Column::new().spacing(12);
            let mut count = 0;
            for region in game.world.regions.iter().filter(|r| r.kind == kind) {
                let members: Vec<usize> = region
                    .members
                    .iter()
                    .copied()
                    .filter(|&c| quiz.mark(c).is_some())
                    .collect();
                if members.is_empty() {
                    continue;
                }
                count += 1;
                let found = members
                    .iter()
                    .filter(|&&c| matches!(quiz.mark(c), Some(Mark::Found { .. })))
                    .count();
                let color = if found == members.len() {
                    style::SUCCESS
                } else {
                    style::ACCENT
                };
                rows = rows.push(
                    column![
                        row![
                            text(region.name.clone()).size(13).width(Fill),
                            text(format!("{found}/{}", members.len()))
                                .size(12)
                                .color(style::MUTED),
                        ],
                        progress_bar(0.0..=members.len() as f32, found as f32)
                            .girth(4)
                            .style(style::progress(color)),
                    ]
                    .spacing(5),
                );
            }
            if count > 1 {
                let mut header = row![section_title("Avancement"), space::horizontal()]
                    .align_y(Center)
                    .spacing(4);
                if choice_useful {
                    let levels = &game.world.info.levels;
                    let choice = |label: &str, value: bool| {
                        button(text(label.to_owned()).size(11))
                            .padding([3, 8])
                            .style(style::pill(by_continent == value))
                            .on_press(Message::ByContinent(value))
                    };
                    header = header
                        .push(choice(&levels[0], true))
                        .push(choice(&levels[1], false));
                }
                body = body.push(column![header, rows].spacing(12));
            }
        }

        let mut recent = Column::new().spacing(8);
        for &country in game.history.iter().rev().take(14) {
            let mark = quiz.mark(country);
            let note = match mark {
                Some(Mark::Found { tries: 1 }) => "",
                Some(Mark::Found { tries: 2 }) => "2e essai",
                Some(Mark::Found { .. }) => "3e essai",
                Some(Mark::Missed) => "raté",
                _ => "",
            };
            recent = recent.push(
                row![
                    container(space())
                        .width(8)
                        .height(8)
                        .style(style::dot(map::mark_color(mark))),
                    text(game.world.countries[country].name.clone())
                        .size(13)
                        .width(Fill),
                    text(note).size(12).color(style::MUTED),
                ]
                .spacing(10)
                .align_y(Center),
            );
        }
        if !game.history.is_empty() {
            body = body.push(column![section_title("Dernières réponses"), recent].spacing(12));
        }

        container(scrollable(body.padding(20)).height(Fill))
            .width(290)
            .height(Fill)
            .style(style::sidebar)
            .into()
    }

    fn summary<'a>(&'a self, game: &'a Game) -> Element<'a, Message> {
        let quiz = &game.quiz;
        let region = &game.world.regions[game.region];
        let title = if quiz.score() >= 1.0 {
            "Parfait !"
        } else if game.timed_out {
            "Temps écoulé !"
        } else {
            "Partie terminée"
        };
        let stat = |label: String, value: String| {
            container(
                column![
                    text(value).size(22).font(style::FONT_BOLD),
                    text(label).size(12).color(style::MUTED)
                ]
                .align_x(Horizontal::Center)
                .spacing(2),
            )
            .padding([12, 18])
            .width(Fill)
            .style(|_: &Theme| container::Style {
                background: Some(style::SURFACE_2.into()),
                border: iced::Border {
                    radius: 12.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
        };
        let mut content = column![
            text(title).size(28).font(style::FONT_BOLD),
            text(format!(
                "{} · {}",
                region.name,
                mode_title(quiz.mode(), &Words::of(&game.world.info))
            ))
            .size(14)
            .color(style::MUTED),
            row![
                stat("score".into(), percent(quiz.score())),
                stat(
                    Words::of(&game.world.info).agree("trouvés"),
                    format!("{}/{}", quiz.found(), quiz.total())
                ),
                stat("temps".into(), clock(game.elapsed())),
            ]
            .spacing(10),
        ]
        .spacing(14)
        .align_x(Center);
        if game.new_record {
            content = content.push(
                container(text("Nouveau record !").size(13).font(style::FONT_SEMIBOLD))
                    .padding([5, 12])
                    .style(style::chip(style::SUCCESS)),
            );
        }
        if quiz.missed() > 0 {
            content = content.push(
                text({
                    let words = Words::of(&game.world.info);
                    format!(
                        "Les {} {} sont en rouge sur la carte.",
                        words.many(),
                        words.agree("ratés")
                    )
                })
                .size(13)
                .color(style::MUTED),
            );
        }
        content = content.push(
            row![
                button(text("Voir la carte").size(14))
                    .padding([10, 16])
                    .style(style::secondary)
                    .on_press(Message::CloseSummary),
                button(text("Menu").size(14))
                    .padding([10, 16])
                    .style(style::secondary)
                    .on_press(Message::BackToMenu),
                button(
                    row![
                        icon(Icon::Chart, 14.0, style::TEXT),
                        text("Progression").size(14)
                    ]
                    .spacing(8)
                    .align_y(Center)
                )
                .padding([10, 16])
                .style(style::secondary)
                .on_press(Message::ShowProgress),
                button(text("Rejouer").size(14).font(style::FONT_SEMIBOLD))
                    .padding([10, 20])
                    .style(style::primary)
                    .on_press(Message::Restart),
            ]
            .spacing(10),
        );
        let card = container(content)
            .padding(28)
            .max_width(480)
            .style(style::floating);
        opaque(
            container(card)
                .center(Fill)
                .style(|_: &Theme| container::Style {
                    background: Some(style::with_alpha(Color::BLACK, 0.45).into()),
                    ..Default::default()
                }),
        )
    }
}

impl Game {
    fn new(atlas: usize, region: usize, mode: Mode, limit: Option<Duration>) -> Self {
        let world = &ATLASES[atlas];
        let zone = &world.regions[region];
        let members = zone.members.clone();
        let mut order = members.clone();
        fastrand::shuffle(&mut order);
        let view = map::view_rect(zone.view, zone.center);
        let now = Instant::now();
        Self {
            atlas,
            world,
            region,
            quiz: Quiz::new(mode, world.countries.len(), members, order),
            projection: Projection::new(world, zone.center),
            view,
            camera: Camera::fit(view),
            animation: None,
            manual_camera: false,
            map_size: Size::ZERO,
            hover: None,
            cache: Cache::new(),
            input: String::new(),
            pending: None,
            feedback: None,
            flash: None,
            started: now,
            now,
            ended: None,
            limit,
            timed_out: false,
            summary: false,
            new_record: false,
            history: Vec::new(),
            locator: None,
        }
    }

    fn elapsed(&self) -> Duration {
        let elapsed = self
            .ended
            .unwrap_or(self.now)
            .saturating_duration_since(self.started);
        self.limit.map_or(elapsed, |limit| elapsed.min(limit))
    }

    /// Temps restant au minuteur, s'il est activé.
    fn remaining(&self) -> Option<Duration> {
        self.limit.map(|limit| limit.saturating_sub(self.elapsed()))
    }

    fn say(&mut self, text: impl Into<String>, color: Color) {
        self.feedback = Some(Feedback {
            text: text.into(),
            color,
            at: self.now,
        });
    }

    fn flash(&mut self, country: usize, color: Color, millis: u64) {
        self.flash = Some((country, color, self.now, Duration::from_millis(millis)));
    }

    fn animate_to(&mut self, to: Camera) {
        if self.map_size == Size::ZERO {
            self.camera = to;
            self.cache.clear();
            return;
        }
        self.animation = Some(Animation {
            from: self.camera,
            to,
            start: Instant::now(),
        });
    }

    /// Mode Nommer : s'assure que le pays à deviner est visible.
    fn follow_target(&mut self) {
        let (Mode::Identify, Some(target)) = (self.quiz.mode(), self.quiz.target()) else {
            return;
        };
        if self.map_size == Size::ZERO {
            return;
        }
        let core = self.projection.core(target);
        let transform = |camera: Camera| Transform::new(camera, self.view, self.map_size);
        // L'utilisateur a pris la main : on ne bouge que si le pays est caché ou trop petit.
        let current = transform(self.camera);
        if self.manual_camera
            && current.nudge(core, SAFE_AREA) == Vector::ZERO
            && core.width.max(core.height) * current.scale >= self.target_min_px()
        {
            self.manual_camera = false;
            return;
        }
        self.manual_camera = false;
        let mut to = self.context_camera(target);
        // Décale juste assez pour que le pays ne soit pas sous l'invite, la barre de saisie
        // ou les boutons.
        to.center += transform(to).nudge(core, SAFE_AREA);
        if to != self.camera {
            self.animate_to(to);
        }
    }

    fn target_min_px(&self) -> f32 {
        TARGET_MIN_SHARE * self.map_size.width.min(self.map_size.height)
    }

    /// Cadrage qui situe le pays dans son contexte : la vue d'ensemble s'il y est assez grand
    /// (`TARGET_MIN_SHARE`), sinon son continent, où un pays minuscule (île, micro-État) est
    /// repéré par le cercle pulsant.
    fn context_camera(&self, target: usize) -> Camera {
        let overview = Camera::fit(self.view);
        let core = self.projection.core(target);
        let scale = Transform::new(overview, self.view, self.map_size).scale;
        if core.width.max(core.height) * scale >= self.target_min_px() {
            return overview;
        }
        // Zone de contexte : le continent, ou la sous-région (départements : leur région).
        let continent = if self.world.info.zoom_sub {
            Some(self.world.countries[target].sub)
        } else {
            self.world.continent_of(target)
        };
        let Some(continent) = continent else {
            return overview;
        };
        let area = map::view_rect(
            self.world.regions[continent].view,
            self.world.regions[self.region].center,
        );
        // Inutile si la partie se joue déjà sur ce continent (ou une zone plus petite).
        if area.width >= self.view.width * 0.9 && area.height >= self.view.height * 0.9 {
            return overview;
        }
        Camera::frame(area, self.view, self.map_size)
    }

    fn after_answer(&mut self) {
        self.cache.clear();
        self.follow_target();
    }

    /// Réagit au résultat d'une réponse.
    fn apply(&mut self, outcome: Outcome) {
        let name = |c: usize| self.world.countries[c].name.clone();
        match outcome {
            Outcome::Found(c) => {
                self.input.clear();
                self.say(format!("✓ {}", name(c)), style::SUCCESS);
                self.flash(c, style::SUCCESS, 900);
                self.history.push(c);
            }
            Outcome::AlreadyFound(c) => {
                self.say(format!("Déjà trouvé : {}", name(c)), style::WARNING);
                self.flash(c, style::WARNING, 900);
            }
            Outcome::OutOfScope(c) => self.say(
                format!("{} ne fait pas partie de la zone", name(c)),
                style::MUTED,
            ),
            Outcome::Correct { country, tries } => {
                self.input.clear();
                let color = map::mark_color(Some(Mark::Found { tries }));
                self.say(format!("✓ {}", name(country)), color);
                self.flash(country, color, 900);
                self.history.push(country);
            }
            Outcome::Wrong { guess, .. } => {
                self.input.clear();
                self.say(
                    format!("✕ Non, ce n'est pas : {}", name(guess)),
                    style::DANGER,
                );
                if self.quiz.mode() == Mode::Locate {
                    self.flash(guess, style::DANGER, 1200);
                }
            }
            Outcome::Revealed { target, .. } => {
                self.input.clear();
                self.say(format!("C'était : {}", name(target)), style::DANGER);
                self.flash(target, style::DANGER, 2000);
                self.history.push(target);
            }
            Outcome::Ignored => {}
        }
        self.after_answer();
    }
}

/// Le texte saisi est-il aussi le début du nom d'un autre pays encore à trouver ?
fn is_prefix_of_other(world: &World, input: &str, country: usize, quiz: &Quiz) -> bool {
    let typed = geo_core::normalize(input);
    quiz.members().iter().any(|&other| {
        other != country
            && quiz.mark(other) == Some(Mark::Pending)
            && world.countries[other]
                .answers
                .iter()
                .any(|a| a.len() > typed.len() && a.starts_with(&typed))
    })
}

fn feedback_chip<'a>(feedback: &Feedback) -> Element<'a, Message> {
    container(
        text(feedback.text.clone())
            .size(14)
            .font(style::FONT_SEMIBOLD),
    )
    .padding([7, 14])
    .style(style::chip(feedback.color))
    .into()
}

fn section_title<'a>(label: &'a str) -> Element<'a, Message> {
    text(label.to_uppercase())
        .size(12)
        .font(style::FONT_SEMIBOLD)
        .color(style::MUTED)
        .into()
}

/// « Tous les pays », « Nommer la région »…
pub fn mode_title(mode: Mode, w: &Words) -> String {
    match mode {
        Mode::Free => capitalize(&w.all()),
        Mode::Identify => format!("Nommer {}", w.the()),
        Mode::Locate => "Trouver sur la carte".into(),
    }
}

fn mode_description(mode: Mode, w: &Words) -> String {
    match mode {
        Mode::Free => format!(
            "Citer {} de la zone, dans n'importe quel ordre. Au clavier ou à la voix.",
            w.all()
        ),
        Mode::Identify => format!(
            "{} s'allume sur la carte : donner son nom.",
            capitalize(&w.a())
        ),
        Mode::Locate => "Un nom s'affiche : cliquer dessus sur la carte.".into(),
    }
}

fn score_color(score: f32) -> Color {
    match score {
        s if s >= 0.9 => style::SUCCESS,
        s if s >= 0.6 => style::WARNING,
        s if s >= 0.3 => style::ORANGE,
        _ => style::DANGER,
    }
}

fn percent(value: f32) -> String {
    format!("{} %", (value * 100.0).round() as i32)
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

fn clock(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Durée de construction de l'aperçu (refait à chaque changement de zone).
    #[test]
    #[ignore]
    fn preview_build_time() {
        for region in ["monde", "europe", "oceanie"] {
            let start = Instant::now();
            let _ = Preview::new(0, ATLASES[0].region(region).unwrap());
            println!("{region} : {:?}", start.elapsed());
        }
    }
}

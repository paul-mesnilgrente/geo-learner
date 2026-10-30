//! Application : écran d'accueil (choix du mode et de la zone) et écran de jeu.

use std::time::{Duration, Instant};

use geo_core::{Mark, Matcher, Mode, Outcome, Quiz, RegionKind};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::Cache;
use iced::widget::{
    Column, Row, button, canvas, column, container, opaque, operation, progress_bar, row, rule,
    scrollable, sensor, space, stack, text, text_input,
};
use iced::{Center, Color, Element, Fill, Size, Subscription, Task, Theme, keyboard, time};

use crate::WORLD;
use crate::icons::{Icon, icon};
use crate::map::{self, Camera, Flash, MapEvent, MapView, Projection, Transform};
use crate::progress::{self, Progress, Session};
use crate::style;
use crate::voice;

const ANSWER_INPUT: &str = "answer";
/// Délai avant de valider un nom qui est aussi le début d'un autre (« Niger » / « Nigeria »).
const AMBIGUOUS_DELAY: Duration = Duration::from_millis(900);
const FEEDBACK_DURATION: Duration = Duration::from_millis(2600);

#[derive(Debug, Clone)]
pub enum Message {
    SelectMode(Mode),
    SelectRegion(usize),
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
    Tick(Instant),
    Voice(voice::Event),
    ToggleMic,
    DownloadModel(voice::Engine),
    SelectEngine(voice::Engine),
    Key(keyboard::Event),
    #[cfg(debug_assertions)]
    DevTick,
    #[cfg(debug_assertions)]
    DevShot(iced::window::Screenshot),
}

pub struct App {
    matcher: Matcher,
    progress: Progress,
    mode: Mode,
    region: usize,
    game: Option<Game>,
    voice: Voice,
    #[cfg(debug_assertions)]
    dev_ticks: u32,
}

struct Voice {
    commands: Option<iced::futures::channel::mpsc::Sender<voice::Command>>,
    engine: voice::Engine,
    /// Moteurs dont le modèle est installé (mis à jour après chaque téléchargement).
    installed: Vec<voice::Engine>,
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
        self.installed = voice::Engine::ALL
            .into_iter()
            .filter(|e| e.installed())
            .collect();
    }

    fn ready(&self) -> bool {
        self.installed.contains(&self.engine)
    }

    fn active(&self) -> bool {
        matches!(self.state, voice::State::Listening | voice::State::Loading)
    }
}

struct Game {
    region: usize,
    quiz: Quiz,
    projection: Projection,
    view: iced::Rectangle,
    camera: Camera,
    animation: Option<Animation>,
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
    /// Résumé affiché en fin de partie ; `new_record` indique un meilleur score.
    summary: bool,
    new_record: bool,
    /// Derniers pays joués (plus récent à la fin).
    history: Vec<usize>,
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
            matcher: Matcher::new(&WORLD),
            progress: Progress::load(),
            mode: Mode::Free,
            region: 0,
            game: None,
            voice: Voice {
                commands: None,
                engine: voice::saved_engine(),
                installed: Vec::new(),
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
        let app = app.with_demo();
        (app, Task::none())
    }

    #[cfg(debug_assertions)]
    fn with_demo(mut self) -> Self {
        let Some(demo) = crate::dev::demo() else {
            return self;
        };
        // Les parties de démonstration ne sont pas enregistrées.
        self.progress = Progress::default();
        if let Some(mode) = demo.mode {
            self.mode = mode;
        }
        if let Some(region) = demo.region.as_deref().and_then(|r| WORLD.region(r)) {
            self.region = region;
        }
        if demo.mode.is_none() {
            return self;
        }
        let mut game = Game::new(self.region, self.mode);
        for code in &demo.answers {
            if let Some(c) = WORLD.countries.iter().position(|c| &c.code == code) {
                let outcome = game.quiz.submit(c);
                game.apply(outcome);
            }
        }
        if let Some((lon, lat, zoom)) = demo.zoom {
            let [x, y] = geo_core::projection::project(lon, lat, WORLD.regions[self.region].center);
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
        if demo.give_up {
            game.quiz.give_up();
        }
        game.flash = None;
        self.game = Some(game);
        self.check_finished();
        if let Some(game) = &mut self.game {
            game.summary = demo.summary;
        }
        self
    }

    pub fn title(&self) -> String {
        match &self.game {
            Some(game) => format!(
                "{} · {} — GeoLearner",
                WORLD.regions[game.region].name,
                mode_title(game.quiz.mode())
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
            Message::SelectMode(mode) => self.mode = mode,
            Message::SelectRegion(region) => self.region = region,
            Message::Start | Message::Restart => {
                if let Some(game) = &self.game {
                    self.region = game.region;
                    self.mode = game.quiz.mode();
                }
                self.game = Some(Game::new(self.region, self.mode));
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
                    let name = &WORLD.countries[country].name;
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
                    game.animate_to(to);
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
            Message::DownloadModel(engine) => self.send_voice(voice::Command::Download(engine)),
            Message::SelectEngine(engine) => {
                if engine != self.voice.engine {
                    if self.voice.active() {
                        self.send_voice(voice::Command::Stop);
                    }
                    self.voice.engine = engine;
                    voice::save_engine(engine);
                }
            }
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                use keyboard::key::Named;
                match key.as_ref() {
                    keyboard::Key::Character("m") if modifiers.command() => self.toggle_mic(),
                    keyboard::Key::Named(Named::Escape) => {
                        if let Some(game) = &mut self.game {
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
            MapEvent::Click(_) => {}
        }
        if matches!(event, MapEvent::Click(_)) {
            operation::focus(ANSWER_INPUT)
        } else {
            Task::none()
        }
    }

    fn on_click(&mut self, country: usize) {
        let Some(game) = &mut self.game else { return };
        let name = &WORLD.countries[country].name;
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
        let Some(country) = self.matcher.lookup(&game.input) else {
            return;
        };
        match game.quiz.mode() {
            Mode::Free if game.quiz.mark(country) == Some(Mark::Pending) => {
                if is_prefix_of_other(&game.input, country, &game.quiz) {
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
        let Some(country) = self.matcher.lookup(&game.input) else {
            let typed = geo_core::normalize(&game.input);
            let message = match WORLD
                .territories
                .iter()
                .find(|t| t.answers.contains(&typed))
            {
                Some(territory) => match territory.owner {
                    Some(owner) => format!(
                        "{} n'est pas un pays du quiz (rattachement : {})",
                        territory.name, WORLD.countries[owner].name
                    ),
                    None => format!("{} n'est pas un pays du quiz", territory.name),
                },
                None => format!("« {} » : pays inconnu", game.input.trim()),
            };
            game.say(message, style::MUTED);
            return;
        };
        let name = &WORLD.countries[country].name;
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
        } else if !self.voice.ready() {
            self.send_voice(voice::Command::Download(self.voice.engine));
        } else if self.game.is_some() {
            self.send_voice(voice::Command::Start(self.voice.engine));
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
        let region = &WORLD.regions[game.region];
        let session = Session {
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
                .map(|&c| WORLD.countries[c].code.clone())
                .collect(),
        };
        game.new_record = match self.progress.best(session.mode, &session.region) {
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
        let header = column![
            text("GeoLearner").size(34).font(style::FONT_BOLD),
            text("Apprendre les pays du monde, carte en main.")
                .size(15)
                .color(style::MUTED),
        ]
        .spacing(4);

        let modes = Row::with_children(Mode::ALL.iter().map(|&mode| {
            button(
                column![
                    text(mode_title(mode)).size(16).font(style::FONT_SEMIBOLD),
                    text(mode_description(mode)).size(13).color(style::MUTED),
                ]
                .spacing(6),
            )
            .padding(18)
            .width(Fill)
            .height(108)
            .style(style::option_card(self.mode == mode))
            .on_press(Message::SelectMode(mode))
            .into()
        }))
        .spacing(12);

        let selected = &WORLD.regions[self.region];
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
            WORLD
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
            let subs = WORLD
                .regions
                .iter()
                .enumerate()
                .filter(|(_, r)| r.parent == Some(continent) && !r.hidden)
                .map(|(i, r)| pill(i, &r.name));
            let all = pill(continent, "Tout le continent");
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

        let best = self.progress.best(self.mode, &selected.id).map(|s| {
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
                        "{} · {} pays",
                        mode_title(self.mode),
                        selected.members.len()
                    ))
                    .size(14)
                    .color(style::MUTED),
                    text(best.unwrap_or_else(|| "Pas encore de record".into()))
                        .size(13)
                        .color(style::MUTED),
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

        let main = column![
            column![section_title("Mode de jeu"), modes].spacing(12),
            zones,
            launch,
        ]
        .spacing(28)
        .width(Fill);

        let side = column![self.voice_card(), self.history_card()]
            .spacing(16)
            .width(340);

        let content = column![header, row![main, side].spacing(28)]
            .spacing(28)
            .padding(36)
            .max_width(1240);
        container(scrollable(container(content).center_x(Fill)).height(Fill))
            .style(style::page)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn voice_card(&self) -> Element<'_, Message> {
        let mut body = column![
            row![
                icon(Icon::Mic, 18.0, style::TEXT),
                text("Saisie vocale").size(16).font(style::FONT_SEMIBOLD)
            ]
            .spacing(8)
            .align_y(Center),
            text("Le modèle choisi est téléchargé une fois ; la reconnaissance se fait ensuite hors ligne. En partie : micro ou Ctrl+M.")
                .size(13)
                .color(style::MUTED),
        ]
        .spacing(10);

        for engine in voice::Engine::ALL {
            let status: Element<'_, Message> = match &self.voice.state {
                voice::State::Downloading(e, p) if *e == engine => {
                    text(percent(*p)).size(13).color(style::ACCENT).into()
                }
                _ if self.voice.installed.contains(&engine) => container(text("Installé").size(12))
                    .padding([3, 10])
                    .style(style::chip(style::SUCCESS))
                    .into(),
                _ => button(
                    row![
                        icon(Icon::Download, 13.0, style::TEXT),
                        text(format!("{} Mo", engine.size_mb())).size(12)
                    ]
                    .spacing(6)
                    .align_y(Center),
                )
                .padding([5, 10])
                .style(style::secondary)
                .on_press(Message::DownloadModel(engine))
                .into(),
            };
            body = body.push(
                button(
                    row![
                        column![
                            text(engine.label()).size(14).font(style::FONT_SEMIBOLD),
                            text(engine.description()).size(12).color(style::MUTED),
                        ]
                        .spacing(2)
                        .width(Fill),
                        status,
                    ]
                    .spacing(10)
                    .align_y(Center),
                )
                .padding([10, 12])
                .width(Fill)
                .style(style::option_card(self.voice.engine == engine))
                .on_press(Message::SelectEngine(engine)),
            );
        }
        if let voice::State::Downloading(_, p) = &self.voice.state {
            body = body.push(
                progress_bar(0.0..=1.0, *p)
                    .girth(5)
                    .style(style::progress(style::ACCENT)),
            );
        }
        if let voice::State::Error(e) = &self.voice.state {
            body = body.push(text(e.clone()).size(13).color(style::DANGER));
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
            let region = WORLD
                .region(&session.region)
                .map(|i| WORLD.regions[i].name.clone())
                .unwrap_or_else(|| session.region.clone());
            list = list.push(
                row![
                    container(space())
                        .width(8)
                        .height(8)
                        .style(style::dot(score_color(session.score))),
                    column![
                        text(region).size(14),
                        text(mode_title(session.mode)).size(12).color(style::MUTED),
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
        let missed = self.progress.most_missed(10);
        if !missed.is_empty() {
            let names = missed
                .iter()
                .filter_map(|(code, _)| WORLD.countries.iter().find(|c| &c.code == code))
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

    fn top_bar<'a>(&'a self, game: &'a Game) -> Element<'a, Message> {
        let quiz = &game.quiz;
        let region = &WORLD.regions[game.region];
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
                text(mode_title(quiz.mode())).size(12).color(style::MUTED),
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
            container(
                text(clock(game.elapsed()))
                    .size(16)
                    .font(style::FONT_SEMIBOLD)
            )
            .padding([6, 12])
            .style(style::chip(style::ACCENT)),
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
            world: &WORLD,
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
        .align_y(Vertical::Center)
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
                text(WORLD.countries[target].name.clone())
                    .size(26)
                    .font(style::FONT_BOLD),
            ],
            Mode::Identify => column![
                text("Quel est ce pays ?")
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
        let placeholder = match game.quiz.mode() {
            Mode::Identify => "Nom du pays en surbrillance…",
            _ => "Nom d'un pays…",
        };
        let flash_border = game
            .feedback
            .as_ref()
            .filter(|f| game.now - f.at < Duration::from_millis(500))
            .map(|f| f.color);
        let input = text_input(placeholder, &game.input)
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
                    "Je vous écoute — citez les pays à la suite".to_owned()
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
            voice::State::Downloading(_, p) => Some(
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
        match &game.feedback {
            Some(feedback) => column![feedback_chip(feedback), card]
                .spacing(10)
                .align_x(Center)
                .into(),
            None => card.into(),
        }
    }

    fn sidebar<'a>(&'a self, game: &'a Game) -> Element<'a, Message> {
        let quiz = &game.quiz;
        let mut body = Column::new().spacing(18);

        let headline = if quiz.mode() == Mode::Free {
            column![
                text(percent(quiz.found() as f32 / quiz.total().max(1) as f32))
                    .size(34)
                    .font(style::FONT_BOLD),
                text(format!(
                    "{} pays trouvés sur {}",
                    quiz.found(),
                    quiz.total()
                ))
                .size(13)
                .color(style::MUTED),
            ]
        } else {
            column![
                text(percent(quiz.score())).size(34).font(style::FONT_BOLD),
                text(format!(
                    "{} du premier coup · {} erreur{} · {} restant{}",
                    quiz.perfect(),
                    quiz.mistakes(),
                    plural(quiz.mistakes() as usize),
                    quiz.total() - quiz.done(),
                    plural(quiz.total() - quiz.done()),
                ))
                .size(13)
                .color(style::MUTED),
            ]
        };
        body = body.push(headline.spacing(2));

        // Avancement par sous-région (mode libre, zones assez grandes).
        if quiz.mode() == Mode::Free {
            let mut rows = Column::new().spacing(12);
            let mut count = 0;
            for region in WORLD.regions.iter().filter(|r| r.kind == RegionKind::Sub) {
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
                body = body.push(column![section_title("Par région"), rows].spacing(12));
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
                    text(WORLD.countries[country].name.clone())
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
        let region = &WORLD.regions[game.region];
        let title = if quiz.score() >= 1.0 {
            "Parfait !"
        } else {
            "Partie terminée"
        };
        let stat = |label: &'static str, value: String| {
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
            text(format!("{} · {}", region.name, mode_title(quiz.mode())))
                .size(14)
                .color(style::MUTED),
            row![
                stat("score", percent(quiz.score())),
                stat("trouvés", format!("{}/{}", quiz.found(), quiz.total())),
                stat("temps", clock(game.elapsed())),
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
                text("Les pays ratés sont en rouge sur la carte.")
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
    fn new(region: usize, mode: Mode) -> Self {
        let zone = &WORLD.regions[region];
        let members = zone.members.clone();
        let mut order = members.clone();
        fastrand::shuffle(&mut order);
        let view = map::view_rect(zone.view, zone.center);
        let now = Instant::now();
        Self {
            region,
            quiz: Quiz::new(mode, WORLD.countries.len(), members, order),
            projection: Projection::new(&WORLD, zone.center),
            view,
            camera: Camera::fit(view),
            animation: None,
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
            summary: false,
            new_record: false,
            history: Vec::new(),
        }
    }

    fn elapsed(&self) -> Duration {
        self.ended
            .unwrap_or(self.now)
            .saturating_duration_since(self.started)
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
        let t = Transform::new(self.camera, self.view, self.map_size);
        let extent = core.width.max(core.height) * t.scale;
        if !t.shows(core, 60.0) || extent < 10.0 {
            let to = self
                .camera
                .focus(core, self.view, self.map_size, 30.0, 10.0);
            self.animate_to(to);
        }
    }

    fn after_answer(&mut self) {
        self.cache.clear();
        self.follow_target();
    }

    /// Réagit au résultat d'une réponse.
    fn apply(&mut self, outcome: Outcome) {
        let name = |c: usize| WORLD.countries[c].name.clone();
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
fn is_prefix_of_other(input: &str, country: usize, quiz: &Quiz) -> bool {
    let typed = geo_core::normalize(input);
    quiz.members().iter().any(|&other| {
        other != country
            && quiz.mark(other) == Some(Mark::Pending)
            && WORLD.countries[other]
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

pub fn mode_title(mode: Mode) -> &'static str {
    match mode {
        Mode::Free => "Tous les pays",
        Mode::Identify => "Nommer le pays",
        Mode::Locate => "Trouver sur la carte",
    }
}

fn mode_description(mode: Mode) -> &'static str {
    match mode {
        Mode::Free => {
            "Citer tous les pays de la zone, dans n'importe quel ordre. Au clavier ou à la voix."
        }
        Mode::Identify => "Un pays s'allume sur la carte : donner son nom.",
        Mode::Locate => "Un nom de pays s'affiche : cliquer dessus sur la carte.",
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

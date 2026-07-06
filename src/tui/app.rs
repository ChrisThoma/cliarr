use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Tabs};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;

use crate::api::models::arr::{Paged, QualityProfile, QueueItem, RootFolder, SystemStatus};
use crate::api::models::nzbget::NzbGroup;
use crate::api::models::plex::SessionList;
use crate::api::models::qbit::Torrent;
use crate::api::models::radarr::Movie;
use crate::api::models::sonarr::{Episode, Series};
use crate::api::Clients;
use crate::commands::calendar::CalendarEntry;
use crate::tui::event::{DataMsg, Event, Loadable};
use crate::tui::posters::PosterManager;
use crate::tui::{fetch, theme};

const TOAST_TICKS: u8 = 16; // ~4s at 250ms/tick
const DOWNLOADS_REFRESH_TICKS: u8 = 20; // 5s

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Search,
    Library,
    Calendar,
    Downloads,
    Missing,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Search,
        Tab::Dashboard,
        Tab::Library,
        Tab::Calendar,
        Tab::Downloads,
        Tab::Missing,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Search => "Search",
            Tab::Library => "Library",
            Tab::Calendar => "Calendar",
            Tab::Downloads => "Downloads",
            Tab::Missing => "Missing",
        }
    }

    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Movie/series toggle used by the Search and Library screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Movies,
    Series,
}

#[derive(Debug, Default)]
pub struct DashboardState {
    pub radarr: Loadable<SystemStatus>,
    pub sonarr: Loadable<SystemStatus>,
    pub plex: Loadable<String>,
    pub qbit: Loadable<String>,
    pub nzbget: Loadable<String>,
    pub sessions: Loadable<SessionList>,
}

#[derive(Debug)]
pub struct SearchState {
    pub input: String,
    /// The omnibox starts live: type immediately on launch.
    pub editing: bool,
    pub movies: Loadable<Vec<Movie>>,
    pub series: Loadable<Vec<Series>>,
    pub selected: usize,
    /// Tick at which the debounced live search should fire.
    pub fire_at: Option<u64>,
    /// Bumped per search; lookup responses carrying an older seq are stale.
    pub seq: u64,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            input: String::new(),
            editing: true,
            movies: Loadable::NotAsked,
            series: Loadable::NotAsked,
            selected: 0,
            fire_at: None,
            seq: 0,
        }
    }
}

#[derive(Debug)]
pub struct LibraryState {
    pub kind: MediaKind,
    pub filter: String,
    pub editing_filter: bool,
    pub selected: usize,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            kind: MediaKind::Movies,
            filter: String::new(),
            editing_filter: false,
            selected: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct DownloadsState {
    pub radarr_queue: Loadable<Vec<QueueItem>>,
    pub sonarr_queue: Loadable<Vec<QueueItem>>,
    pub torrents: Loadable<Vec<Torrent>>,
    pub nzb: Loadable<Vec<NzbGroup>>,
    pub selected: usize,
    pub ticks_since_refresh: u8,
}

#[derive(Debug, Default)]
pub struct MissingState {
    pub radarr: Loadable<Paged<Movie>>,
    pub sonarr: Loadable<Paged<Episode>>,
    pub selected: usize,
}

#[derive(Debug, Default)]
pub struct CalendarState {
    pub entries: Loadable<Vec<CalendarEntry>>,
}

// At most one modal exists at a time; boxing the big variants buys nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Modal {
    Help,
    Confirm {
        msg: String,
        /// Extra toggle rendered as "[f] …: on/off" (delete files etc.).
        toggle_label: Option<&'static str>,
        toggle: bool,
        action: ConfirmAction,
    },
    Add(AddModal),
    Edit(EditModal),
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    DeleteMovie { id: i64, title: String },
    DeleteSeries { id: i64, title: String },
    RemoveQueueItem { radarr: bool, id: i64, blocklist: bool },
    DeleteTorrent { hash: String, name: String },
    DeleteNzb { id: i64, name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddField {
    Profile,
    Root,
    Monitored,
    SearchOnAdd,
    Confirm,
}

#[derive(Debug)]
pub struct AddModal {
    pub movie: Option<Movie>,
    pub series: Option<Series>,
    pub options: Loadable<(Vec<QualityProfile>, Vec<RootFolder>)>,
    pub profile_idx: usize,
    pub root_idx: usize,
    pub field: AddField,
    pub monitored: bool,
    pub search_on_add: bool,
}

impl AddModal {
    pub fn title(&self) -> String {
        match (&self.movie, &self.series) {
            (Some(m), _) => format!(
                "{} ({})",
                m.title,
                m.year.map(|y| y.to_string()).unwrap_or_default()
            ),
            (_, Some(s)) => format!(
                "{} ({})",
                s.title,
                s.year.map(|y| y.to_string()).unwrap_or_default()
            ),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Profile,
    Monitored,
    Confirm,
}

/// Edit an existing library item (quality profile, monitored).
#[derive(Debug)]
pub struct EditModal {
    /// Radarr movie id when editing a movie, Sonarr series id otherwise.
    pub id: i64,
    pub is_movie: bool,
    pub title: String,
    pub options: Loadable<(Vec<QualityProfile>, Vec<RootFolder>)>,
    /// Profile id the item currently has; selects the initial profile_idx
    /// once the profile list arrives.
    pub current_profile_id: Option<i64>,
    pub profile_idx: usize,
    pub monitored: bool,
    pub field: EditField,
}

impl EditModal {
    /// Point profile_idx at the item's current profile (once options load).
    pub fn sync_profile_idx(&mut self) {
        if let (Loadable::Ready((profiles, _)), Some(id)) = (&self.options, self.current_profile_id) {
            self.profile_idx = profiles.iter().position(|p| p.id == id).unwrap_or(0);
        }
    }
}

pub struct App {
    pub clients: Arc<Clients>,
    pub tx: UnboundedSender<Event>,
    pub tab: Tab,
    pub should_quit: bool,
    pub tick: u64,
    pub toast: Option<(String, Color, u8)>,
    pub modal: Option<Modal>,
    pub posters: PosterManager,

    // Shared library data (dashboard counts + library screen).
    pub movies: Loadable<Vec<Movie>>,
    pub series: Loadable<Vec<Series>>,

    pub dash: DashboardState,
    pub search: SearchState,
    pub library: LibraryState,
    pub calendar: CalendarState,
    pub downloads: DownloadsState,
    pub missing: MissingState,
}

impl App {
    pub fn new(clients: Arc<Clients>, tx: UnboundedSender<Event>, posters: PosterManager) -> Self {
        Self {
            clients,
            tx,
            tab: Tab::Search,
            should_quit: false,
            tick: 0,
            toast: None,
            modal: None,
            posters,
            movies: Loadable::NotAsked,
            series: Loadable::NotAsked,
            dash: DashboardState::default(),
            search: SearchState::default(),
            library: LibraryState::default(),
            calendar: CalendarState::default(),
            downloads: DownloadsState::default(),
            missing: MissingState::default(),
        }
    }

    pub fn spinner(&self) -> &'static str {
        theme::SPINNER_FRAMES[(self.tick as usize) % theme::SPINNER_FRAMES.len()]
    }

    pub fn toast_ok(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), theme::SUCCESS, TOAST_TICKS));
    }

    pub fn toast_err(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), theme::ERROR, TOAST_TICKS));
    }

    // ---- data loading ----------------------------------------------------

    /// Kick off the fetches the active tab needs (only if not yet asked).
    pub fn load_tab(&mut self) {
        match self.tab {
            Tab::Dashboard => {
                if matches!(self.dash.radarr, Loadable::NotAsked)
                    && matches!(self.dash.qbit, Loadable::NotAsked)
                {
                    self.refresh_tab();
                }
                self.ensure_library_loaded();
            }
            Tab::Library => self.ensure_library_loaded(),
            Tab::Calendar => {
                if matches!(self.calendar.entries, Loadable::NotAsked) {
                    self.refresh_tab();
                }
            }
            Tab::Downloads => {
                if matches!(self.downloads.torrents, Loadable::NotAsked)
                    && matches!(self.downloads.radarr_queue, Loadable::NotAsked)
                {
                    self.refresh_tab();
                }
            }
            Tab::Missing => {
                if matches!(self.missing.radarr, Loadable::NotAsked)
                    && matches!(self.missing.sonarr, Loadable::NotAsked)
                {
                    self.refresh_tab();
                }
            }
            Tab::Search => {}
        }
    }

    fn ensure_library_loaded(&mut self) {
        if matches!(self.movies, Loadable::NotAsked) && self.clients.radarr.is_some() {
            self.movies = Loadable::Loading;
            fetch::movies(self.tx.clone(), self.clients.clone());
        }
        if matches!(self.series, Loadable::NotAsked) && self.clients.sonarr.is_some() {
            self.series = Loadable::Loading;
            fetch::series(self.tx.clone(), self.clients.clone());
        }
    }

    /// Force-refresh the active tab's data.
    pub fn refresh_tab(&mut self) {
        let tx = self.tx.clone();
        let clients = self.clients.clone();
        match self.tab {
            Tab::Dashboard => {
                if clients.radarr.is_some() {
                    self.dash.radarr = Loadable::Loading;
                }
                if clients.sonarr.is_some() {
                    self.dash.sonarr = Loadable::Loading;
                }
                if clients.plex.is_some() {
                    self.dash.plex = Loadable::Loading;
                    self.dash.sessions = Loadable::Loading;
                }
                if clients.qbit.is_some() {
                    self.dash.qbit = Loadable::Loading;
                }
                if clients.nzbget.is_some() {
                    self.dash.nzbget = Loadable::Loading;
                }
                fetch::dashboard(tx, clients);
            }
            Tab::Library => {
                if self.clients.radarr.is_some() {
                    self.movies = Loadable::Loading;
                    fetch::movies(tx.clone(), clients.clone());
                }
                if self.clients.sonarr.is_some() {
                    self.series = Loadable::Loading;
                    fetch::series(tx, clients);
                }
            }
            Tab::Calendar => {
                self.calendar.entries = Loadable::Loading;
                fetch::calendar(tx, clients, 14);
            }
            Tab::Downloads => {
                if clients.radarr.is_some() {
                    self.downloads.radarr_queue = Loadable::Loading;
                }
                if clients.sonarr.is_some() {
                    self.downloads.sonarr_queue = Loadable::Loading;
                }
                if clients.qbit.is_some() {
                    self.downloads.torrents = Loadable::Loading;
                }
                if clients.nzbget.is_some() {
                    self.downloads.nzb = Loadable::Loading;
                }
                self.downloads.ticks_since_refresh = 0;
                fetch::downloads(tx, clients);
            }
            Tab::Missing => {
                if clients.radarr.is_some() {
                    self.missing.radarr = Loadable::Loading;
                }
                if clients.sonarr.is_some() {
                    self.missing.sonarr = Loadable::Loading;
                }
                fetch::missing(tx, clients);
            }
            Tab::Search => {}
        }
    }

    // ---- event handling ----------------------------------------------------

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Resize => {}
            Event::Tick => self.handle_tick(),
            Event::Data(msg) => self.handle_data(msg),
            Event::Poster { url, result } => self.posters.on_loaded(&url, result),
        }
    }

    fn handle_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if let Some(at) = self.search.fire_at
            && self.tick >= at {
                self.search.fire_at = None;
                self.run_search();
            }
        if let Some((_, _, ttl)) = &mut self.toast {
            *ttl = ttl.saturating_sub(1);
            if *ttl == 0 {
                self.toast = None;
            }
        }
        if self.tab == Tab::Downloads && self.modal.is_none() {
            self.downloads.ticks_since_refresh += 1;
            if self.downloads.ticks_since_refresh >= DOWNLOADS_REFRESH_TICKS {
                self.downloads.ticks_since_refresh = 0;
                // Silent refresh: keep old data on screen, no Loading flicker.
                fetch::downloads(self.tx.clone(), self.clients.clone());
            }
        }
    }

    fn handle_data(&mut self, msg: DataMsg) {
        match msg {
            DataMsg::RadarrHealth(r) => self.dash.radarr.set(r),
            DataMsg::SonarrHealth(r) => self.dash.sonarr.set(r),
            DataMsg::PlexHealth(r) => self.dash.plex.set(r),
            DataMsg::QbitHealth(r) => self.dash.qbit.set(r),
            DataMsg::NzbHealth(r) => self.dash.nzbget.set(r),
            DataMsg::PlexSessions(r) => self.dash.sessions.set(r),
            DataMsg::RadarrMovies(r) => self.movies.set(r),
            DataMsg::SonarrSeries(r) => self.series.set(r),
            DataMsg::RadarrLookup { seq, result } => {
                if seq == self.search.seq {
                    self.search.movies.set(result);
                    self.search.selected = 0;
                }
            }
            DataMsg::SonarrLookup { seq, result } => {
                if seq == self.search.seq {
                    self.search.series.set(result);
                    self.search.selected = 0;
                }
            }
            DataMsg::AddOptions(r) => match &mut self.modal {
                Some(Modal::Add(add)) => add.options.set(r),
                Some(Modal::Edit(edit)) => {
                    edit.options.set(r);
                    edit.sync_profile_idx();
                }
                _ => {}
            },
            DataMsg::RadarrQueue(r) => self.downloads.radarr_queue.set(r),
            DataMsg::SonarrQueue(r) => self.downloads.sonarr_queue.set(r),
            DataMsg::Torrents(r) => self.downloads.torrents.set(r),
            DataMsg::NzbGroups(r) => self.downloads.nzb.set(r),
            DataMsg::Calendar(r) => self.calendar.entries.set(r),
            DataMsg::RadarrMissing(r) => self.missing.radarr.set(r),
            DataMsg::SonarrMissing(r) => self.missing.sonarr.set(r),
            DataMsg::ActionDone { desc, result } => {
                match result {
                    Ok(()) => {
                        self.toast_ok(format!("✓ {desc}"));
                        self.after_action_refresh();
                    }
                    Err(e) => self.toast_err(format!("✗ {desc}: {e}")),
                }
            }
        }
    }

    /// After a mutating action, re-fetch whatever the current tab shows.
    fn after_action_refresh(&mut self) {
        match self.tab {
            Tab::Downloads => {
                fetch::downloads(self.tx.clone(), self.clients.clone());
            }
            Tab::Library => {
                if self.clients.radarr.is_some() {
                    fetch::movies(self.tx.clone(), self.clients.clone());
                }
                if self.clients.sonarr.is_some() {
                    fetch::series(self.tx.clone(), self.clients.clone());
                }
            }
            Tab::Missing => fetch::missing(self.tx.clone(), self.clients.clone()),
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl-C always quits.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        if self.modal.is_some() {
            self.handle_modal_key(key);
            return;
        }
        // Text-input modes capture printable keys.
        if self.tab == Tab::Search && self.search.editing {
            self.handle_search_input(key);
            return;
        }
        if self.tab == Tab::Library && self.library.editing_filter {
            self.handle_filter_input(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char('r') => {
                self.refresh_tab();
                self.toast_ok("refreshing");
            }
            KeyCode::Char(c @ '1'..='6') => {
                self.tab = Tab::ALL[(c as usize) - ('1' as usize)];
                self.load_tab();
            }
            KeyCode::Tab => {
                self.tab = Tab::ALL[(self.tab.index() + 1) % Tab::ALL.len()];
                self.load_tab();
            }
            KeyCode::BackTab => {
                self.tab = Tab::ALL[(self.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len()];
                self.load_tab();
            }
            _ => self.handle_screen_key(key),
        }
    }

    fn handle_screen_key(&mut self, key: KeyEvent) {
        match self.tab {
            Tab::Dashboard => {}
            Tab::Search => self.search_key(key),
            Tab::Library => self.library_key(key),
            Tab::Calendar => {}
            Tab::Downloads => self.downloads_key(key),
            Tab::Missing => self.missing_key(key),
        }
    }

    // ---- drawing ----------------------------------------------------------

    pub fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(Block::new().style(theme::base()), area);

        let [tabs_area, content, status] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(0), Constraint::Length(1)])
                .areas(area);

        self.draw_tabs(f, tabs_area);
        match self.tab {
            Tab::Dashboard => self.draw_dashboard(f, content),
            Tab::Search => self.draw_search(f, content),
            Tab::Library => self.draw_library(f, content),
            Tab::Calendar => self.draw_calendar(f, content),
            Tab::Downloads => self.draw_downloads(f, content),
            Tab::Missing => self.draw_missing(f, content),
        }
        self.draw_status_bar(f, status);
        self.draw_modal(f, area);
    }

    fn draw_tabs(&self, f: &mut Frame, area: Rect) {
        let titles: Vec<Line> = Tab::ALL
            .iter()
            .enumerate()
            .map(|(i, t)| {
                Line::from(vec![
                    Span::styled(format!("{} ", i + 1), theme::dim()),
                    Span::raw(t.title()),
                ])
            })
            .collect();
        let tabs = Tabs::new(titles)
            .select(self.tab.index())
            .style(theme::dim())
            .highlight_style(theme::accent_bold(theme::ACCENT))
            .divider(Span::styled("·", theme::dim()));
        let [bar, _pad] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        let [logo_area, tabs_area] =
            Layout::horizontal([Constraint::Length(10), Constraint::Min(0)]).areas(bar);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ● ", theme::accent_bold(theme::ACCENT)),
                Span::styled("cliarr", theme::accent_bold(theme::FG)),
            ])),
            logo_area,
        );
        f.render_widget(tabs, tabs_area);
    }

    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let mut spans: Vec<Span> = Vec::new();
        if let Some((msg, color, _)) = &self.toast {
            spans.push(Span::styled(format!(" {msg} "), theme::accent_bold(*color)));
        } else {
            let keys = self.context_keys();
            for (key, label) in keys {
                spans.push(Span::styled(format!(" {key} "), theme::accent_bold(theme::ACCENT)));
                spans.push(Span::styled(format!("{label}  "), theme::dim()));
            }
        }
        if self.anything_loading() {
            spans.push(Span::styled(
                format!("{} ", self.spinner()),
                theme::accent_bold(theme::ACCENT),
            ));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)).style(ratatui::style::Style::new().bg(theme::SURFACE)),
            area,
        );
    }

    fn context_keys(&self) -> Vec<(&'static str, &'static str)> {
        if self.modal.is_some() {
            return vec![("Esc", "close"), ("Enter", "confirm")];
        }
        let mut keys: Vec<(&'static str, &'static str)> = vec![("1-6", "tabs")];
        match self.tab {
            Tab::Dashboard => {}
            Tab::Search => {
                if self.search.editing {
                    keys.push(("↑↓", "select"));
                    keys.push(("Enter", "add"));
                    keys.push(("Esc", "done typing"));
                } else {
                    keys.push(("/", "type"));
                    keys.push(("a", "add"));
                }
            }
            Tab::Library => {
                keys.push(("←/→", "movies/series"));
                keys.push(("/", "filter"));
                keys.push(("s", "search release"));
                keys.push(("e", "edit"));
                keys.push(("d", "delete"));
            }
            Tab::Calendar => {}
            Tab::Downloads => {
                keys.push(("p/P", "pause/resume"));
                keys.push(("d", "delete"));
                keys.push(("b", "blocklist"));
            }
            Tab::Missing => {
                keys.push(("s", "search selected"));
                keys.push(("a", "search all"));
            }
        }
        keys.push(("r", "refresh"));
        keys.push(("?", "help"));
        keys.push(("q", "quit"));
        keys
    }

    fn anything_loading(&self) -> bool {
        self.movies.is_loading()
            || self.series.is_loading()
            || self.search.movies.is_loading()
            || self.search.series.is_loading()
            || self.calendar.entries.is_loading()
            || self.downloads.torrents.is_loading()
            || self.downloads.radarr_queue.is_loading()
            || self.downloads.sonarr_queue.is_loading()
            || self.downloads.nzb.is_loading()
            || self.missing.radarr.is_loading()
            || self.missing.sonarr.is_loading()
            || self.dash.radarr.is_loading()
    }
}

// list navigation helper shared by screens
pub fn move_selection(selected: &mut usize, delta: i64, len: usize) {
    if len == 0 {
        *selected = 0;
        return;
    }
    let cur = *selected as i64;
    *selected = (cur + delta).clamp(0, len as i64 - 1) as usize;
}

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::api::models::radarr::Movie;
use crate::api::models::sonarr::Series;
use crate::tui::app::{AddField, AddModal, AddTarget, App, Modal, move_selection};
use crate::tui::event::Loadable;
use crate::tui::{fetch, theme};

/// Live search fires this many ticks (250ms each) after the last keystroke.
const DEBOUNCE_TICKS: u64 = 2;
const MIN_QUERY_LEN: usize = 2;

/// One row in the merged movie+series results list.
pub(crate) enum SearchHit<'a> {
    Movie(&'a Movie),
    Series(&'a Series),
}

impl SearchHit<'_> {
    fn title(&self) -> &str {
        match self {
            SearchHit::Movie(m) => &m.title,
            SearchHit::Series(s) => &s.title,
        }
    }

    fn year(&self) -> Option<i32> {
        match self {
            SearchHit::Movie(m) => m.year,
            SearchHit::Series(s) => s.year,
        }
    }

    fn in_library(&self) -> bool {
        match self {
            SearchHit::Movie(m) => m.id > 0,
            SearchHit::Series(s) => s.id > 0,
        }
    }

    fn tag(&self) -> (&'static str, ratatui::style::Color) {
        match self {
            SearchHit::Movie(_) => ("MOVIE ", theme::RADARR),
            SearchHit::Series(_) => ("SERIES", theme::SONARR),
        }
    }
}

impl App {
    /// Merged results: interleave movies and series so both stay visible
    /// (each list arrives relevance-ordered from its service).
    pub(crate) fn search_hits(&self) -> Vec<SearchHit<'_>> {
        let movies = self.search.movies.ready().map(|v| v.as_slice()).unwrap_or(&[]);
        let series = self.search.series.ready().map(|v| v.as_slice()).unwrap_or(&[]);
        let mut hits = Vec::with_capacity(movies.len() + series.len());
        let mut m = movies.iter();
        let mut s = series.iter();
        loop {
            match (m.next(), s.next()) {
                (None, None) => break,
                (movie, episode) => {
                    if let Some(movie) = movie {
                        hits.push(SearchHit::Movie(movie));
                    }
                    if let Some(series) = episode {
                        hits.push(SearchHit::Series(series));
                    }
                }
            }
        }
        hits
    }

    /// Identity of the currently selected hit, stable across list rebuilds.
    pub(crate) fn selected_search_key(&self) -> Option<(bool, String, Option<i32>)> {
        let hits = self.search_hits();
        hits.get(self.search.selected).map(|hit| match hit {
            SearchHit::Movie(m) => (true, m.title.clone(), m.year),
            SearchHit::Series(s) => (false, s.title.clone(), s.year),
        })
    }

    /// Re-point the selection at the same hit after results changed
    /// (the second service's reply re-interleaves the merged list).
    pub(crate) fn restore_search_selection(&mut self, prev: Option<(bool, String, Option<i32>)>) {
        let Some((is_movie, title, year)) = prev else {
            self.search.selected = 0;
            return;
        };
        self.search.selected = self
            .search_hits()
            .iter()
            .position(|hit| match hit {
                SearchHit::Movie(m) => is_movie && m.title == title && m.year == year,
                SearchHit::Series(s) => !is_movie && s.title == title && s.year == year,
            })
            .unwrap_or(0);
    }

    pub(crate) fn handle_search_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.search.editing = false,
            // Tab still switches screens so the omnibox never traps you.
            KeyCode::Tab | KeyCode::BackTab => {
                self.search.editing = false;
                self.handle(crate::tui::event::Event::Key(key));
            }
            KeyCode::Down => {
                let len = self.search_results_len();
                move_selection(&mut self.search.selected, 1, len);
            }
            KeyCode::Up => {
                let len = self.search_results_len();
                move_selection(&mut self.search.selected, -1, len);
            }
            KeyCode::Enter => {
                if self.search.fire_at.is_some() {
                    self.search.fire_at = None;
                    self.run_search();
                } else {
                    self.open_add_modal();
                }
            }
            KeyCode::Backspace => {
                self.search.input.pop();
                self.schedule_search();
            }
            KeyCode::Char(c) => {
                self.search.input.push(c);
                self.schedule_search();
            }
            _ => {}
        }
    }

    fn schedule_search(&mut self) {
        if self.search.input.trim().len() < MIN_QUERY_LEN {
            self.search.fire_at = None;
            // Invalidate any in-flight lookups so they can't repopulate.
            self.search.seq += 1;
            self.search.movies = Loadable::NotAsked;
            self.search.series = Loadable::NotAsked;
            self.search.selected = 0;
            return;
        }
        self.search.fire_at = Some(self.tick + DEBOUNCE_TICKS);
    }

    /// Query Radarr and Sonarr simultaneously.
    pub(crate) fn run_search(&mut self) {
        let term = self.search.input.trim().to_string();
        if term.len() < MIN_QUERY_LEN {
            return;
        }
        if self.clients.radarr.is_none() && self.clients.sonarr.is_none() {
            self.toast_err("neither radarr nor sonarr is configured");
            return;
        }
        self.search.seq += 1;
        if self.clients.radarr.is_some() {
            self.search.movies = Loadable::Loading;
            fetch::lookup_movies(self.tx.clone(), self.clients.clone(), term.clone(), self.search.seq);
        }
        if self.clients.sonarr.is_some() {
            self.search.series = Loadable::Loading;
            fetch::lookup_series(self.tx.clone(), self.clients.clone(), term, self.search.seq);
        }
    }

    /// Prefill the query (from `cliarr <words>`) and search immediately.
    pub(crate) fn start_with_query(&mut self, query: String) {
        self.search.input = query;
        self.search.editing = true;
        self.run_search();
    }

    pub(crate) fn search_key(&mut self, key: KeyEvent) {
        let len = self.search_results_len();
        match key.code {
            KeyCode::Char('/') | KeyCode::Char('i') => self.search.editing = true,
            KeyCode::Char('j') | KeyCode::Down => move_selection(&mut self.search.selected, 1, len),
            KeyCode::Char('k') | KeyCode::Up => move_selection(&mut self.search.selected, -1, len),
            KeyCode::Char('a') | KeyCode::Enter => self.open_add_modal(),
            _ => {}
        }
    }

    fn search_results_len(&self) -> usize {
        self.search.movies.ready().map(|v| v.len()).unwrap_or(0)
            + self.search.series.ready().map(|v| v.len()).unwrap_or(0)
    }

    fn open_add_modal(&mut self) {
        let target = {
            let hits = self.search_hits();
            match hits.get(self.search.selected) {
                Some(SearchHit::Movie(m)) => AddTarget::Movie((*m).clone()),
                Some(SearchHit::Series(s)) => AddTarget::Series((*s).clone()),
                None => return,
            }
        };
        // A nonzero id means the lookup hit is already in the library.
        let (for_movies, title, in_library) = match &target {
            AddTarget::Movie(m) => (true, m.title.clone(), m.id > 0),
            AddTarget::Series(s) => (false, s.title.clone(), s.id > 0),
        };
        if in_library {
            self.toast_err(format!("\"{title}\" is already in the library"));
            return;
        }
        fetch::add_options(self.tx.clone(), self.clients.clone(), for_movies);
        self.modal = Some(Modal::Add(AddModal {
            target,
            options: Loadable::Loading,
            profile_idx: 0,
            root_idx: 0,
            field: AddField::Profile,
            monitored: true,
            search_on_add: true,
        }));
    }

    pub(crate) fn draw_search(&mut self, f: &mut Frame, area: Rect) {
        let [input_area, body] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);

        // Omnibox.
        let cursor = if self.search.editing { "▏" } else { "" };
        let placeholder = self.search.input.is_empty() && self.search.editing;
        let input_line = if placeholder {
            Line::from(vec![
                Span::styled(cursor, theme::accent_bold(theme::ACCENT)),
                Span::styled("start typing to search movies & series…", theme::dim()),
            ])
        } else {
            Line::from(vec![
                Span::raw(self.search.input.clone()),
                Span::styled(cursor, theme::accent_bold(theme::ACCENT)),
            ])
        };
        let block = theme::panel(
            if self.search.editing { "Search" } else { "Search (/ to type)" },
            theme::ACCENT,
            self.search.editing,
        );
        let inner = block.inner(input_area);
        f.render_widget(block, input_area);
        f.render_widget(Paragraph::new(input_line), inner);

        let [list_area, detail_area] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(body);

        self.draw_search_results(f, list_area);
        self.draw_search_detail(f, detail_area);
    }

    fn draw_search_results(&mut self, f: &mut Frame, area: Rect) {
        let searching = self.search.movies.is_loading()
            || self.search.series.is_loading()
            || self.search.fire_at.is_some();
        let spinner = self.spinner().to_string();

        let items: Vec<ListItem> = self
            .search_hits()
            .iter()
            .map(|hit| {
                let (tag, color) = hit.tag();
                let mut spans = vec![
                    Span::styled(format!("{tag} "), theme::accent_bold(color)),
                    Span::raw(hit.title().to_string()),
                    Span::styled(
                        format!("  {}", hit.year().map(|y| y.to_string()).unwrap_or_default()),
                        theme::dim(),
                    ),
                ];
                if hit.in_library() {
                    spans.push(Span::styled("  ✓ in library", theme::accent_bold(theme::SUCCESS)));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();

        let title = if searching {
            format!("Results {spinner}")
        } else {
            format!("Results ({})", items.len())
        };
        let block = theme::panel(&title, theme::ACCENT, !self.search.editing);
        let inner = block.inner(area);
        f.render_widget(block, area);

        if items.is_empty() {
            let msg: (String, ratatui::style::Style) = if searching {
                (format!("{spinner} searching…"), theme::dim())
            } else if let Loadable::Failed(e) = &self.search.movies {
                (e.clone(), theme::accent_bold(theme::ERROR))
            } else if let Loadable::Failed(e) = &self.search.series {
                (e.clone(), theme::accent_bold(theme::ERROR))
            } else if self.search.input.trim().len() >= MIN_QUERY_LEN {
                ("no results".into(), theme::dim())
            } else {
                ("results appear as you type".into(), theme::dim())
            };
            f.render_widget(Paragraph::new(msg.0).style(msg.1).wrap(Wrap { trim: true }), inner);
            return;
        }

        let list = List::new(items)
            .highlight_style(theme::selected_row())
            .highlight_symbol(theme::SELECT_MARKER);
        let mut state = ListState::default().with_selected(Some(self.search.selected));
        f.render_stateful_widget(list, inner, &mut state);
    }

    fn draw_search_detail(&mut self, f: &mut Frame, area: Rect) {
        let block = theme::panel("Details", theme::ACCENT, false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let detail: Option<(String, String, String, Option<String>)> = {
            let hits = self.search_hits();
            hits.get(self.search.selected).map(|hit| match hit {
                SearchHit::Movie(m) => (
                    format!("{} ({})", m.title, m.year.map(|y| y.to_string()).unwrap_or_default()),
                    format!(
                        "movie · {} · {} min · {}",
                        m.status.clone().unwrap_or_default(),
                        m.runtime.unwrap_or(0),
                        m.genres.join(", ")
                    ),
                    m.overview.clone().unwrap_or_default(),
                    m.poster_remote_url().map(String::from),
                ),
                SearchHit::Series(s) => (
                    format!("{} ({})", s.title, s.year.map(|y| y.to_string()).unwrap_or_default()),
                    format!(
                        "series · {} · {} · {}",
                        s.status.clone().unwrap_or_default(),
                        s.network.clone().unwrap_or_default(),
                        s.genres.join(", ")
                    ),
                    s.overview.clone().unwrap_or_default(),
                    s.poster_remote_url().map(String::from),
                ),
            })
        };
        let Some((title, meta, overview, poster)) = detail else { return };
        self.draw_media_detail(f, inner, &title, &meta, &overview, poster.as_deref());
    }

    /// Shared detail pane: poster on top (when renderable), text below.
    pub(crate) fn draw_media_detail(
        &mut self,
        f: &mut Frame,
        area: Rect,
        title: &str,
        meta: &str,
        overview: &str,
        poster_url: Option<&str>,
    ) {
        let mut text_area = area;
        if let Some(url) = poster_url
            && self.posters.enabled() && area.height > 14 {
                let [poster_area, rest] = Layout::vertical([
                    Constraint::Length(area.height.saturating_sub(8).min(18)),
                    Constraint::Min(0),
                ])
                .areas(area);
                let poster_width = (poster_area.height * 2 / 3 * 2).min(poster_area.width);
                let poster_rect = Rect { width: poster_width, ..poster_area };
                if !self.posters.render(f, poster_rect, url) {
                    f.render_widget(
                        Paragraph::new(format!("{} loading art…", self.spinner())).style(theme::dim()),
                        poster_rect,
                    );
                }
                text_area = rest;
            }
        let lines = vec![
            Line::styled(title.to_string(), theme::accent_bold(theme::FG)),
            Line::styled(meta.to_string(), theme::dim()),
            Line::raw(""),
            Line::raw(overview.to_string()),
        ];
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text_area);
    }
}

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::app::{AddField, AddModal, App, MediaKind, Modal, move_selection};
use crate::tui::event::Loadable;
use crate::tui::{fetch, theme};

impl App {
    pub(crate) fn handle_search_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.search.editing = false,
            KeyCode::Enter => {
                self.search.editing = false;
                self.run_search();
            }
            KeyCode::Backspace => {
                self.search.input.pop();
            }
            KeyCode::Char(c) => self.search.input.push(c),
            _ => {}
        }
    }

    fn run_search(&mut self) {
        let term = self.search.input.trim().to_string();
        if term.is_empty() {
            return;
        }
        match self.search.kind {
            MediaKind::Movies => {
                if self.clients.radarr.is_none() {
                    self.toast_err("radarr is not configured");
                    return;
                }
                self.search.movies = Loadable::Loading;
                fetch::lookup_movies(self.tx.clone(), self.clients.clone(), term);
            }
            MediaKind::Series => {
                if self.clients.sonarr.is_none() {
                    self.toast_err("sonarr is not configured");
                    return;
                }
                self.search.series = Loadable::Loading;
                fetch::lookup_series(self.tx.clone(), self.clients.clone(), term);
            }
        }
    }

    pub(crate) fn search_key(&mut self, key: KeyEvent) {
        let len = self.search_results_len();
        match key.code {
            KeyCode::Char('/') | KeyCode::Char('i') => self.search.editing = true,
            KeyCode::Char('m') => self.search.kind = MediaKind::Movies,
            KeyCode::Char('s') => self.search.kind = MediaKind::Series,
            KeyCode::Char('j') | KeyCode::Down => move_selection(&mut self.search.selected, 1, len),
            KeyCode::Char('k') | KeyCode::Up => move_selection(&mut self.search.selected, -1, len),
            KeyCode::Char('a') | KeyCode::Enter => self.open_add_modal(),
            _ => {}
        }
    }

    fn search_results_len(&self) -> usize {
        match self.search.kind {
            MediaKind::Movies => self.search.movies.ready().map(|v| v.len()).unwrap_or(0),
            MediaKind::Series => self.search.series.ready().map(|v| v.len()).unwrap_or(0),
        }
    }

    fn open_add_modal(&mut self) {
        let modal = match self.search.kind {
            MediaKind::Movies => {
                let Some(movie) = self
                    .search
                    .movies
                    .ready()
                    .and_then(|v| v.get(self.search.selected))
                    .cloned()
                else {
                    return;
                };
                if movie.id > 0 {
                    self.toast_err(format!("\"{}\" is already in the library", movie.title));
                    return;
                }
                AddModal {
                    movie: Some(movie),
                    series: None,
                    options: Loadable::Loading,
                    profile_idx: 0,
                    root_idx: 0,
                    field: AddField::Profile,
                    monitored: true,
                    search_on_add: true,
                }
            }
            MediaKind::Series => {
                let Some(series) = self
                    .search
                    .series
                    .ready()
                    .and_then(|v| v.get(self.search.selected))
                    .cloned()
                else {
                    return;
                };
                if series.id > 0 {
                    self.toast_err(format!("\"{}\" is already in the library", series.title));
                    return;
                }
                AddModal {
                    movie: None,
                    series: Some(series),
                    options: Loadable::Loading,
                    profile_idx: 0,
                    root_idx: 0,
                    field: AddField::Profile,
                    monitored: true,
                    search_on_add: true,
                }
            }
        };
        fetch::add_options(
            self.tx.clone(),
            self.clients.clone(),
            self.search.kind == MediaKind::Movies,
        );
        self.modal = Some(Modal::Add(modal));
    }

    pub(crate) fn draw_search(&mut self, f: &mut Frame, area: Rect) {
        let [input_area, body] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);

        // Query input.
        let kind_label = match self.search.kind {
            MediaKind::Movies => ("movies", theme::RADARR),
            MediaKind::Series => ("series", theme::SONARR),
        };
        let cursor = if self.search.editing { "▏" } else { "" };
        let input_line = Line::from(vec![
            Span::styled(format!("[{}] ", kind_label.0), theme::accent_bold(kind_label.1)),
            Span::raw(self.search.input.clone()),
            Span::styled(cursor, theme::accent_bold(theme::ACCENT)),
        ]);
        let block = theme::panel(
            if self.search.editing { "Search (Enter to run)" } else { "Search (/ to edit)" },
            theme::ACCENT,
            self.search.editing,
        );
        let inner = block.inner(input_area);
        f.render_widget(block, input_area);
        f.render_widget(Paragraph::new(input_line), inner);

        let [list_area, detail_area] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(body);

        // Results list.
        let spinner = self.spinner().to_string();
        let (items, loading, failed): (Vec<ListItem>, bool, Option<String>) = match self.search.kind {
            MediaKind::Movies => match &self.search.movies {
                Loadable::Ready(movies) => (
                    movies
                        .iter()
                        .map(|m| {
                            let mut spans = vec![
                                Span::raw(m.title.clone()),
                                Span::styled(
                                    format!("  {}", m.year.map(|y| y.to_string()).unwrap_or_default()),
                                    theme::dim(),
                                ),
                            ];
                            if m.id > 0 {
                                spans.push(Span::styled("  ✓ in library", theme::accent_bold(theme::SUCCESS)));
                            }
                            ListItem::new(Line::from(spans))
                        })
                        .collect(),
                    false,
                    None,
                ),
                Loadable::Loading => (vec![], true, None),
                Loadable::Failed(e) => (vec![], false, Some(e.clone())),
                Loadable::NotAsked => (vec![], false, None),
            },
            MediaKind::Series => match &self.search.series {
                Loadable::Ready(series) => (
                    series
                        .iter()
                        .map(|s| {
                            let mut spans = vec![
                                Span::raw(s.title.clone()),
                                Span::styled(
                                    format!("  {}", s.year.map(|y| y.to_string()).unwrap_or_default()),
                                    theme::dim(),
                                ),
                            ];
                            if s.id > 0 {
                                spans.push(Span::styled("  ✓ in library", theme::accent_bold(theme::SUCCESS)));
                            }
                            ListItem::new(Line::from(spans))
                        })
                        .collect(),
                    false,
                    None,
                ),
                Loadable::Loading => (vec![], true, None),
                Loadable::Failed(e) => (vec![], false, Some(e.clone())),
                Loadable::NotAsked => (vec![], false, None),
            },
        };

        let block = theme::panel("Results", kind_label.1, !self.search.editing);
        let inner = block.inner(list_area);
        f.render_widget(block, list_area);
        if loading {
            f.render_widget(Paragraph::new(format!("{spinner} searching…")).style(theme::dim()), inner);
        } else if let Some(e) = failed {
            f.render_widget(
                Paragraph::new(e).style(theme::accent_bold(theme::ERROR)).wrap(Wrap { trim: true }),
                inner,
            );
        } else if items.is_empty() {
            f.render_widget(
                Paragraph::new("type a query and press Enter").style(theme::dim()),
                inner,
            );
        } else {
            let list = List::new(items)
                .highlight_style(theme::selected_row())
                .highlight_symbol(theme::SELECT_MARKER);
            let mut state = ListState::default().with_selected(Some(self.search.selected));
            f.render_stateful_widget(list, inner, &mut state);
        }

        self.draw_search_detail(f, detail_area);
    }

    fn draw_search_detail(&mut self, f: &mut Frame, area: Rect) {
        let accent = match self.search.kind {
            MediaKind::Movies => theme::RADARR,
            MediaKind::Series => theme::SONARR,
        };
        let block = theme::panel("Details", accent, false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let (title, meta, overview, poster): (String, String, String, Option<String>) =
            match self.search.kind {
                MediaKind::Movies => {
                    let Some(m) = self
                        .search
                        .movies
                        .ready()
                        .and_then(|v| v.get(self.search.selected))
                    else {
                        return;
                    };
                    (
                        format!("{} ({})", m.title, m.year.map(|y| y.to_string()).unwrap_or_default()),
                        format!(
                            "{} · {} min · {}",
                            m.status.clone().unwrap_or_default(),
                            m.runtime.unwrap_or(0),
                            m.genres.join(", ")
                        ),
                        m.overview.clone().unwrap_or_default(),
                        m.poster_remote_url().map(String::from),
                    )
                }
                MediaKind::Series => {
                    let Some(s) = self
                        .search
                        .series
                        .ready()
                        .and_then(|v| v.get(self.search.selected))
                    else {
                        return;
                    };
                    (
                        format!("{} ({})", s.title, s.year.map(|y| y.to_string()).unwrap_or_default()),
                        format!(
                            "{} · {} · {}",
                            s.status.clone().unwrap_or_default(),
                            s.network.clone().unwrap_or_default(),
                            s.genres.join(", ")
                        ),
                        s.overview.clone().unwrap_or_default(),
                        s.poster_remote_url().map(String::from),
                    )
                }
            };

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
                let [poster_area, rest] =
                    Layout::vertical([Constraint::Length(area.height.saturating_sub(8).min(18)), Constraint::Min(0)])
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

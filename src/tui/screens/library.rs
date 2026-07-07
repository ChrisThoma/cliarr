use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::api::http::join_url;
use crate::api::models::radarr::Movie;
use crate::api::models::sonarr::Series;
use crate::commands::output::fmt_bytes;
use crate::tui::app::{App, ConfirmAction, EditField, EditModal, MediaKind, Modal, move_selection};
use crate::tui::event::Loadable;
use crate::tui::{fetch, theme};

impl App {
    pub(crate) fn handle_filter_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.library.filter.clear();
                self.library.editing_filter = false;
            }
            KeyCode::Enter => self.library.editing_filter = false,
            // Navigation keys still work while filtering (never trap them):
            // leave the typed filter in place and re-dispatch the key.
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => {
                self.library.editing_filter = false;
                self.handle(crate::tui::event::Event::Key(key));
            }
            KeyCode::Backspace => {
                self.library.filter.pop();
            }
            KeyCode::Char(c) => {
                self.library.filter.push(c);
                self.library.selected = 0;
            }
            _ => {}
        }
    }

    pub(crate) fn library_key(&mut self, key: KeyEvent) {
        let len = match self.library.kind {
            MediaKind::Movies => self.filtered_movie_indices().len(),
            MediaKind::Series => self.filtered_series_indices().len(),
        };
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                self.library.kind = MediaKind::Movies;
                self.library.selected = 0;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.library.kind = MediaKind::Series;
                self.library.selected = 0;
            }
            KeyCode::Char('/') => self.library.editing_filter = true,
            KeyCode::Char('j') | KeyCode::Down => move_selection(&mut self.library.selected, 1, len),
            KeyCode::Char('k') | KeyCode::Up => move_selection(&mut self.library.selected, -1, len),
            KeyCode::Char('g') => self.library.selected = 0,
            KeyCode::Char('G') => self.library.selected = len.saturating_sub(1),
            KeyCode::Char('s') | KeyCode::Char('S') => self.library_search_release(),
            KeyCode::Char('e') => self.library_edit(),
            KeyCode::Char('d') => self.library_delete(),
            _ => {}
        }
    }

    pub(crate) fn filtered_movie_indices(&self) -> Vec<usize> {
        filtered_indices(&self.movies, &self.library.filter, |m| m.title.as_str())
    }

    pub(crate) fn filtered_series_indices(&self) -> Vec<usize> {
        filtered_indices(&self.series, &self.library.filter, |s| s.title.as_str())
    }

    /// The movie under the library cursor (filter + sort applied), if any.
    fn selected_movie(&self) -> Option<&Movie> {
        let i = *self.filtered_movie_indices().get(self.library.selected)?;
        self.movies.ready()?.get(i)
    }

    /// The series under the library cursor (filter + sort applied), if any.
    fn selected_series(&self) -> Option<&Series> {
        let i = *self.filtered_series_indices().get(self.library.selected)?;
        self.series.ready()?.get(i)
    }

    fn library_search_release(&mut self) {
        match self.library.kind {
            MediaKind::Movies => {
                let Some(m) = self.selected_movie() else { return };
                let (id, title) = (m.id, m.title.clone());
                let Some(radarr) = self.clients.radarr.clone() else { return };
                fetch::action(self.tx.clone(), self.tab, format!("searching for {title}"), async move {
                    radarr.command("MoviesSearch", serde_json::json!({ "movieIds": [id] })).await
                });
            }
            MediaKind::Series => {
                let Some(s) = self.selected_series() else { return };
                let (id, title) = (s.id, s.title.clone());
                let Some(sonarr) = self.clients.sonarr.clone() else { return };
                fetch::action(self.tx.clone(), self.tab, format!("searching for {title}"), async move {
                    sonarr.command("SeriesSearch", serde_json::json!({ "seriesId": id })).await
                });
            }
        }
    }

    /// Open the edit modal for the selected item, seeded with its current
    /// profile/monitored values; profiles load async like the add modal.
    fn library_edit(&mut self) {
        let (id, is_movie, title, current_profile_id, monitored) = match self.library.kind {
            MediaKind::Movies => {
                let Some(m) = self.selected_movie() else { return };
                (m.id, true, m.title.clone(), m.quality_profile_id, m.monitored.unwrap_or(false))
            }
            MediaKind::Series => {
                let Some(s) = self.selected_series() else { return };
                (s.id, false, s.title.clone(), s.quality_profile_id, s.monitored.unwrap_or(false))
            }
        };
        self.modal = Some(Modal::Edit(EditModal {
            id,
            is_movie,
            title,
            options: Loadable::Loading,
            current_profile_id,
            profile_idx: 0,
            monitored,
            field: EditField::Profile,
        }));
        self.modal_seq += 1;
        fetch::add_options(self.tx.clone(), self.clients.clone(), is_movie, self.modal_seq);
    }

    fn library_delete(&mut self) {
        let (msg, action) = match self.library.kind {
            MediaKind::Movies => {
                let Some(m) = self.selected_movie() else { return };
                (
                    format!("Remove \"{}\" from Radarr?", m.title),
                    ConfirmAction::DeleteMovie { id: m.id, title: m.title.clone() },
                )
            }
            MediaKind::Series => {
                let Some(s) = self.selected_series() else { return };
                (
                    format!("Remove \"{}\" from Sonarr?", s.title),
                    ConfirmAction::DeleteSeries { id: s.id, title: s.title.clone() },
                )
            }
        };
        self.modal = Some(Modal::Confirm {
            msg,
            toggle_label: Some("also delete files"),
            toggle: false,
            action,
        });
    }

    pub(crate) fn draw_library(&mut self, f: &mut Frame, area: Rect) {
        let show_filter = self.library.editing_filter || !self.library.filter.is_empty();
        let (filter_area, body) = if show_filter {
            let [fa, body] = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);
            (Some(fa), body)
        } else {
            (None, area)
        };

        if let Some(fa) = filter_area {
            let cursor = if self.library.editing_filter { "▏" } else { "" };
            let block = theme::panel("Filter", theme::ACCENT, self.library.editing_filter);
            let inner = block.inner(fa);
            f.render_widget(block, fa);
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(self.library.filter.clone()),
                    Span::styled(cursor, theme::accent_bold(theme::ACCENT)),
                ])),
                inner,
            );
        }

        let [list_area, detail_area] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(body);

        match self.library.kind {
            MediaKind::Movies => self.draw_movie_list(f, list_area),
            MediaKind::Series => self.draw_series_list(f, list_area),
        }
        self.draw_library_detail(f, detail_area);
    }

    fn draw_movie_list(&mut self, f: &mut Frame, area: Rect) {
        let indices = self.filtered_movie_indices();
        draw_loadable_list(
            f,
            area,
            &format!("Movies ({})", indices.len()),
            theme::RADARR,
            &self.movies,
            &indices,
            self.library.selected,
            self.spinner(),
            "radarr not configured",
            |m| {
                let status = if m.has_file.unwrap_or(false) {
                    Span::styled("● ", theme::accent_bold(theme::SUCCESS))
                } else if m.monitored.unwrap_or(false) {
                    Span::styled("◌ ", theme::accent_bold(theme::WARN))
                } else {
                    Span::styled("· ", theme::dim())
                };
                ListItem::new(Line::from(vec![
                    status,
                    Span::raw(m.title.clone()),
                    Span::styled(
                        format!("  {}", m.year.map(|y| y.to_string()).unwrap_or_default()),
                        theme::dim(),
                    ),
                ]))
            },
        );
    }

    fn draw_series_list(&mut self, f: &mut Frame, area: Rect) {
        let indices = self.filtered_series_indices();
        draw_loadable_list(
            f,
            area,
            &format!("Series ({})", indices.len()),
            theme::SONARR,
            &self.series,
            &indices,
            self.library.selected,
            self.spinner(),
            "sonarr not configured",
            |s| {
                let pct = s
                    .statistics
                    .as_ref()
                    .and_then(|st| st.percent_of_episodes)
                    .unwrap_or(0.0);
                let status = if pct >= 100.0 {
                    Span::styled("● ", theme::accent_bold(theme::SUCCESS))
                } else if s.monitored.unwrap_or(false) {
                    Span::styled("◌ ", theme::accent_bold(theme::WARN))
                } else {
                    Span::styled("· ", theme::dim())
                };
                let eps = s
                    .statistics
                    .as_ref()
                    .map(|st| {
                        format!(
                            "  {}/{}",
                            st.episode_file_count.unwrap_or(0),
                            st.episode_count.unwrap_or(0)
                        )
                    })
                    .unwrap_or_default();
                ListItem::new(Line::from(vec![
                    status,
                    Span::raw(s.title.clone()),
                    Span::styled(eps, theme::dim()),
                ]))
            },
        );
    }

    fn draw_library_detail(&mut self, f: &mut Frame, area: Rect) {
        let accent = match self.library.kind {
            MediaKind::Movies => theme::RADARR,
            MediaKind::Series => theme::SONARR,
        };
        let block = theme::panel("Details", accent, false);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let (title, meta, overview, poster): (String, String, String, Option<String>) =
            match self.library.kind {
                MediaKind::Movies => {
                    let Some(m) = self.selected_movie() else { return };
                    (
                        format!("{} ({})", m.title, m.year.map(|y| y.to_string()).unwrap_or_default()),
                        format!(
                            "{} · {} · {}",
                            if m.has_file.unwrap_or(false) { "downloaded" } else { "missing" },
                            fmt_bytes(m.size_on_disk.unwrap_or(0) as f64),
                            m.path.clone().unwrap_or_default(),
                        ),
                        m.overview.clone().unwrap_or_default(),
                        self.arr_poster_url(true, m.poster_remote_url(), &m.images),
                    )
                }
                MediaKind::Series => {
                    let Some(s) = self.selected_series() else { return };
                    let stats = s
                        .statistics
                        .as_ref()
                        .map(|st| {
                            format!(
                                "{}/{} episodes · {}",
                                st.episode_file_count.unwrap_or(0),
                                st.episode_count.unwrap_or(0),
                                fmt_bytes(st.size_on_disk.unwrap_or(0) as f64)
                            )
                        })
                        .unwrap_or_default();
                    (
                        format!("{} ({})", s.title, s.year.map(|y| y.to_string()).unwrap_or_default()),
                        format!("{} · {stats}", s.status.clone().unwrap_or_default()),
                        s.overview.clone().unwrap_or_default(),
                        self.arr_poster_url(false, s.poster_remote_url(), &s.images),
                    )
                }
            };

        self.draw_media_detail(f, inner, &title, &meta, &overview, poster.as_deref());
    }

    /// Library items carry an arr-proxied relative `url`; prefer remoteUrl,
    /// fall back to the proxied path joined onto the service base + api key.
    fn arr_poster_url(
        &self,
        radarr: bool,
        remote: Option<&str>,
        images: &[crate::api::models::arr::Image],
    ) -> Option<String> {
        if let Some(r) = remote
            && r.starts_with("http") {
                return Some(r.to_string());
            }
        let core = if radarr {
            self.clients.radarr.as_ref().map(|c| c.core().clone())
        } else {
            self.clients.sonarr.as_ref().map(|c| c.core().clone())
        }?;
        let rel = images
            .iter()
            .find(|i| i.cover_type == "poster")
            .and_then(|i| i.url.as_deref())?;
        let mut url = join_url(core.base_url(), rel).ok()?;
        url.query_pairs_mut().append_pair("apikey", core.api_key());
        Some(url.to_string())
    }
}

/// Indices into `items` matching the title filter, sorted by title.
fn filtered_indices<T>(
    items: &Loadable<Vec<T>>,
    filter: &str,
    title: impl Fn(&T) -> &str,
) -> Vec<usize> {
    let filter = filter.to_lowercase();
    items
        .ready()
        .map(|items| {
            let mut idx: Vec<usize> = (0..items.len())
                .filter(|&i| filter.is_empty() || title(&items[i]).to_lowercase().contains(&filter))
                .collect();
            idx.sort_by(|&a, &b| title(&items[a]).to_lowercase().cmp(&title(&items[b]).to_lowercase()));
            idx
        })
        .unwrap_or_default()
}

/// Panel + list over a `Loadable`, with the shared loading/failed/not-asked
/// arms; `row` renders one Ready item, in `indices` order.
#[allow(clippy::too_many_arguments)]
fn draw_loadable_list<T>(
    f: &mut Frame,
    area: Rect,
    title: &str,
    accent: Color,
    data: &Loadable<Vec<T>>,
    indices: &[usize],
    selected: usize,
    spinner: &'static str,
    not_configured: &str,
    row: impl Fn(&T) -> ListItem<'static>,
) {
    let block = theme::panel(title, accent, true);
    let inner = block.inner(area);
    f.render_widget(block, area);

    match data {
        Loadable::Ready(items) => {
            let items: Vec<ListItem> = indices.iter().map(|&i| row(&items[i])).collect();
            let list = List::new(items)
                .highlight_style(theme::selected_row())
                .highlight_symbol(theme::SELECT_MARKER);
            let mut state = ListState::default().with_selected(Some(selected));
            f.render_stateful_widget(list, inner, &mut state);
        }
        Loadable::Loading => {
            f.render_widget(
                Paragraph::new(format!("{spinner} loading…")).style(theme::dim()),
                inner,
            );
        }
        Loadable::Failed(e) => {
            f.render_widget(
                Paragraph::new(e.clone()).style(theme::accent_bold(theme::ERROR)).wrap(Wrap { trim: true }),
                inner,
            );
        }
        Loadable::NotAsked => {
            f.render_widget(Paragraph::new(not_configured).style(theme::dim()), inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::api::Clients;
    use crate::config::{Config, PosterProtocol};
    use crate::tui::posters::PosterManager;

    fn movie(title: &str) -> Movie {
        serde_json::from_value(serde_json::json!({ "title": title, "tmdbId": 1 })).unwrap()
    }

    fn app() -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let posters =
            PosterManager::detect(PosterProtocol::Off, tx.clone(), crate::api::http::build_client());
        App::new(Arc::new(Clients::from_config(&Config::default())), tx, posters)
    }

    #[test]
    fn filtered_indices_sort_by_title_and_match_case_insensitively() {
        let mut app = app();
        app.movies = Loadable::Ready(vec![movie("The Matrix"), movie("alien"), movie("Aliens")]);

        assert_eq!(app.filtered_movie_indices(), vec![1, 2, 0], "no filter: all, title-sorted");

        app.library.filter = "ALIEN".into();
        assert_eq!(app.filtered_movie_indices(), vec![1, 2]);

        app.library.filter = "zzz".into();
        assert!(app.filtered_movie_indices().is_empty());
    }

    #[test]
    fn selected_movie_follows_filtered_sorted_order() {
        let mut app = app();
        app.movies = Loadable::Ready(vec![movie("The Matrix"), movie("Alien")]);

        app.library.selected = 0;
        assert_eq!(app.selected_movie().unwrap().title, "Alien");

        app.library.selected = 1;
        assert_eq!(app.selected_movie().unwrap().title, "The Matrix");

        app.library.selected = 5;
        assert!(app.selected_movie().is_none(), "out-of-range cursor selects nothing");
    }

    #[test]
    fn nothing_selectable_until_data_is_ready() {
        let app = app();
        assert!(app.filtered_movie_indices().is_empty());
        assert!(app.filtered_series_indices().is_empty());
        assert!(app.selected_movie().is_none());
        assert!(app.selected_series().is_none());
    }
}

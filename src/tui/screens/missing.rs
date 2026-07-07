use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::tui::app::{App, move_selection};
use crate::tui::event::Loadable;
use crate::tui::fetch;
use crate::tui::theme::{self, Service};

pub(crate) struct MissingRow {
    pub service: Service,
    pub id: i64,
    pub title: String,
    pub detail: String,
}

impl App {
    pub(crate) fn missing_rows(&self) -> Vec<MissingRow> {
        let mut rows = Vec::new();
        if let Loadable::Ready(paged) = &self.missing.radarr {
            for m in &paged.records {
                rows.push(MissingRow {
                    service: Service::Radarr,
                    id: m.id,
                    title: m.title.clone(),
                    detail: m.year.map(|y| format!("({y})")).unwrap_or_default(),
                });
            }
        }
        if let Loadable::Ready(paged) = &self.missing.sonarr {
            for e in &paged.records {
                rows.push(MissingRow {
                    service: Service::Sonarr,
                    id: e.id,
                    title: e.series_title().to_string(),
                    detail: format!("{} — {}", e.code(), e.title.clone().unwrap_or_default()),
                });
            }
        }
        rows
    }

    pub(crate) fn missing_key(&mut self, key: KeyEvent) {
        let rows = self.missing_rows();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                move_selection(&mut self.missing.selected, 1, rows.len())
            }
            KeyCode::Char('k') | KeyCode::Up => {
                move_selection(&mut self.missing.selected, -1, rows.len())
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                let Some(row) = rows.into_iter().nth(self.missing.selected) else { return };
                match row.service {
                    Service::Radarr => {
                        let Some(radarr) = self.clients.radarr.clone() else { return };
                        let id = row.id;
                        fetch::action(
                            self.tx.clone(),
                            self.tab,
                            format!("searching for {}", row.title),
                            async move {
                                radarr
                                    .command("MoviesSearch", serde_json::json!({ "movieIds": [id] }))
                                    .await
                            },
                        );
                    }
                    Service::Sonarr => {
                        let Some(sonarr) = self.clients.sonarr.clone() else { return };
                        let id = row.id;
                        fetch::action(
                            self.tx.clone(),
                            self.tab,
                            format!("searching for {} {}", row.title, row.detail),
                            async move {
                                sonarr
                                    .command("EpisodeSearch", serde_json::json!({ "episodeIds": [id] }))
                                    .await
                            },
                        );
                    }
                    _ => {}
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if let Some(radarr) = self.clients.radarr.clone() {
                    fetch::action(
                        self.tx.clone(),
                        self.tab,
                        "searching all missing movies".into(),
                        async move {
                            radarr
                                .command("MissingMoviesSearch", serde_json::json!({}))
                                .await
                        },
                    );
                }
                if let Some(sonarr) = self.clients.sonarr.clone() {
                    fetch::action(
                        self.tx.clone(),
                        self.tab,
                        "searching all missing episodes".into(),
                        async move {
                            sonarr
                                .command("MissingEpisodeSearch", serde_json::json!({}))
                                .await
                        },
                    );
                }
            }
            _ => {}
        }
    }

    pub(crate) fn draw_missing(&mut self, f: &mut Frame, area: Rect) {
        let rows = self.missing_rows();
        let totals = {
            let movies = self
                .missing
                .radarr
                .ready()
                .map(|p| p.total_records)
                .unwrap_or(0);
            let episodes = self
                .missing
                .sonarr
                .ready()
                .map(|p| p.total_records)
                .unwrap_or(0);
            format!("Missing — {movies} movies · {episodes} episodes")
        };
        let block = theme::panel(&totals, theme::WARN, true);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut errors = Vec::new();
        if let Some(e) = self.missing.radarr.failed() {
            errors.push(format!("radarr: {e}"));
        }
        if let Some(e) = self.missing.sonarr.failed() {
            errors.push(format!("sonarr: {e}"));
        }
        let inner = super::draw_fetch_errors(f, inner, &errors);

        let loading = self.missing.radarr.is_loading() || self.missing.sonarr.is_loading();
        if rows.is_empty() {
            if loading {
                let msg = format!("{} loading…", self.spinner());
                f.render_widget(Paragraph::new(msg).style(theme::dim()), inner);
            } else if errors.is_empty() {
                let msg = "nothing missing, library is complete";
                f.render_widget(Paragraph::new(msg).style(theme::dim()), inner);
            }
            return;
        }

        let items: Vec<ListItem> = rows
            .iter()
            .map(|row| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<7}", row.service.label()), theme::accent_bold(row.service.accent())),
                    Span::raw(row.title.clone()),
                    Span::styled(format!("  {}", row.detail), theme::dim()),
                ]))
            })
            .collect();
        let list = List::new(items)
            .highlight_style(theme::selected_row())
            .highlight_symbol(theme::SELECT_MARKER);
        let mut state = ListState::default().with_selected(Some(self.missing.selected));
        f.render_stateful_widget(list, inner, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::api::models::arr::Paged;
    use crate::api::models::radarr::Movie;
    use crate::api::Clients;
    use crate::config::{Config, PosterProtocol};
    use crate::tui::posters::PosterManager;

    fn app() -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let posters =
            PosterManager::detect(PosterProtocol::Off, tx.clone(), crate::api::http::build_client());
        App::new(Arc::new(Clients::from_config(&Config::default())), tx, posters)
    }

    fn render_missing(app: &mut App) -> String {
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| app.draw_missing(f, f.area())).unwrap();
        term.backend().to_string()
    }

    fn paged(titles: &[&str]) -> Paged<Movie> {
        Paged {
            page: 1,
            page_size: 50,
            total_records: titles.len() as i64,
            records: titles
                .iter()
                .map(|t| {
                    serde_json::from_value(serde_json::json!({ "title": t, "tmdbId": 1 })).unwrap()
                })
                .collect(),
        }
    }

    #[test]
    fn failed_fetch_is_not_reported_as_complete_library() {
        let mut app = app();
        app.missing.radarr = Loadable::Failed("connection refused".into());
        app.missing.sonarr = Loadable::Ready(Paged {
            page: 1,
            page_size: 50,
            total_records: 0,
            records: vec![],
        });

        let out = render_missing(&mut app);
        assert!(out.contains("radarr: connection refused"), "error must be shown: {out}");
        assert!(!out.contains("library is complete"), "failure must not read as complete: {out}");
    }

    #[test]
    fn partial_failure_shows_error_and_surviving_rows() {
        let mut app = app();
        app.missing.radarr = Loadable::Ready(paged(&["Dune"]));
        app.missing.sonarr = Loadable::Failed("timeout".into());

        let out = render_missing(&mut app);
        assert!(out.contains("sonarr: timeout"), "error line expected: {out}");
        assert!(out.contains("Dune"), "working service's rows still expected: {out}");
    }

    #[test]
    fn empty_and_ready_still_reads_as_complete() {
        let mut app = app();
        app.missing.radarr = Loadable::Ready(paged(&[]));
        app.missing.sonarr = Loadable::Ready(Paged {
            page: 1,
            page_size: 50,
            total_records: 0,
            records: vec![],
        });

        let out = render_missing(&mut app);
        assert!(out.contains("library is complete"), "genuinely empty stays friendly: {out}");
    }
}

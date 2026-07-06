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
            KeyCode::Char('S') => {
                let Some(row) = rows.into_iter().nth(self.missing.selected) else { return };
                match row.service {
                    Service::Radarr => {
                        let Some(radarr) = self.clients.radarr.clone() else { return };
                        let id = row.id;
                        fetch::action(
                            self.tx.clone(),
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
            KeyCode::Char('A') => {
                if let Some(radarr) = self.clients.radarr.clone() {
                    fetch::action(
                        self.tx.clone(),
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

        let loading = self.missing.radarr.is_loading() || self.missing.sonarr.is_loading();
        if rows.is_empty() {
            let msg = if loading {
                format!("{} loading…", self.spinner())
            } else {
                "nothing missing — library is complete".to_string()
            };
            f.render_widget(Paragraph::new(msg).style(theme::dim()), inner);
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

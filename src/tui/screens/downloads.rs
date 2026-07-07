use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::commands::output::{fmt_bytes, fmt_eta_secs, fmt_speed};
use crate::tui::app::{App, ConfirmAction, Modal, move_selection};
use crate::tui::event::Loadable;
use crate::tui::theme::{self, Service};
use crate::tui::fetch;

/// Stable identity of a download row across queue refreshes.
pub(crate) enum DownloadKey {
    Arr(Service, i64),
    Torrent(String),
    Nzb(i64),
}

/// One row in the unified downloads list.
pub(crate) enum DownloadRow {
    Arr {
        service: Service,
        id: i64,
        title: String,
        status: String,
        progress: Option<f64>,
        timeleft: String,
    },
    Torrent {
        hash: String,
        name: String,
        state: String,
        paused: bool,
        progress: f64,
        speed: i64,
        eta: i64,
    },
    Nzb {
        id: i64,
        name: String,
        status: String,
        progress: f64,
        size_mb: i64,
    },
}

impl App {
    pub(crate) fn download_rows(&self) -> Vec<DownloadRow> {
        let mut rows = Vec::new();
        if let Loadable::Ready(items) = &self.downloads.radarr_queue {
            for q in items {
                rows.push(DownloadRow::Arr {
                    service: Service::Radarr,
                    id: q.id,
                    title: q.title.clone().unwrap_or_default(),
                    status: q.status.clone().unwrap_or_default(),
                    progress: q.progress(),
                    timeleft: q.timeleft.clone().unwrap_or_default(),
                });
            }
        }
        if let Loadable::Ready(items) = &self.downloads.sonarr_queue {
            for q in items {
                rows.push(DownloadRow::Arr {
                    service: Service::Sonarr,
                    id: q.id,
                    title: q.title.clone().unwrap_or_default(),
                    status: q.status.clone().unwrap_or_default(),
                    progress: q.progress(),
                    timeleft: q.timeleft.clone().unwrap_or_default(),
                });
            }
        }
        if let Loadable::Ready(torrents) = &self.downloads.torrents {
            for t in torrents {
                rows.push(DownloadRow::Torrent {
                    hash: t.hash.clone(),
                    name: t.name.clone(),
                    state: t.state.clone(),
                    paused: t.is_paused(),
                    progress: t.progress * 100.0,
                    speed: t.dlspeed,
                    eta: t.eta,
                });
            }
        }
        if let Loadable::Ready(groups) = &self.downloads.nzb {
            for g in groups {
                rows.push(DownloadRow::Nzb {
                    id: g.id,
                    name: g.name.clone(),
                    status: g.status.clone(),
                    progress: g.progress(),
                    size_mb: g.file_size_mb,
                });
            }
        }
        rows
    }

    pub(crate) fn downloads_key(&mut self, key: KeyEvent) {
        let rows = self.download_rows();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                move_selection(&mut self.downloads.selected, 1, rows.len())
            }
            KeyCode::Char('k') | KeyCode::Up => {
                move_selection(&mut self.downloads.selected, -1, rows.len())
            }
            KeyCode::Char('p') => self.downloads_pause_resume(true),
            KeyCode::Char('P') => self.downloads_pause_resume(false),
            KeyCode::Char('d') => self.downloads_delete(),
            KeyCode::Char('b') => self.downloads_blocklist(),
            _ => {}
        }
    }

    fn selected_row(&self) -> Option<DownloadRow> {
        self.download_rows().into_iter().nth(self.downloads.selected)
    }

    /// Identity of the currently selected row, stable across refreshes.
    pub(crate) fn selected_download_key(&self) -> Option<DownloadKey> {
        self.selected_row().map(|row| match row {
            DownloadRow::Arr { service, id, .. } => DownloadKey::Arr(service, id),
            DownloadRow::Torrent { hash, .. } => DownloadKey::Torrent(hash),
            DownloadRow::Nzb { id, .. } => DownloadKey::Nzb(id),
        })
    }

    /// Re-point the selection at the same download after a (possibly silent)
    /// refresh shifted the rows; falls back to clamping into range.
    pub(crate) fn restore_download_selection(&mut self, prev: Option<DownloadKey>) {
        let rows = self.download_rows();
        let found = prev.and_then(|key| {
            rows.iter().position(|row| match (row, &key) {
                (DownloadRow::Arr { service, id, .. }, DownloadKey::Arr(s, k)) => {
                    service == s && id == k
                }
                (DownloadRow::Torrent { hash, .. }, DownloadKey::Torrent(h)) => hash == h,
                (DownloadRow::Nzb { id, .. }, DownloadKey::Nzb(k)) => id == k,
                _ => false,
            })
        });
        self.downloads.selected = match found {
            Some(pos) => pos,
            None => self.downloads.selected.min(rows.len().saturating_sub(1)),
        };
    }

    fn downloads_pause_resume(&mut self, pause: bool) {
        let Some(row) = self.selected_row() else { return };
        let verb = if pause { "pausing" } else { "resuming" };
        match row {
            DownloadRow::Torrent { hash, name, .. } => {
                let Some(qbit) = self.clients.qbit.clone() else { return };
                fetch::action(self.tx.clone(), self.tab, format!("{verb} {name}"), async move {
                    if pause {
                        qbit.pause(&[hash]).await
                    } else {
                        qbit.resume(&[hash]).await
                    }
                });
            }
            DownloadRow::Nzb { id, name, .. } => {
                let Some(nzbget) = self.clients.nzbget.clone() else { return };
                fetch::action(self.tx.clone(), self.tab, format!("{verb} {name}"), async move {
                    if pause {
                        nzbget.pause(&[id]).await.map(|_| ())
                    } else {
                        nzbget.resume(&[id]).await.map(|_| ())
                    }
                });
            }
            DownloadRow::Arr { .. } => {
                self.toast_err("pause/resume applies to torrent/nzb rows; arr queue items follow their download client");
            }
        }
    }

    fn downloads_delete(&mut self) {
        let Some(row) = self.selected_row() else { return };
        let modal = match row {
            // remove-from-client defaults on, matching the Radarr/Sonarr web
            // UI; leaving the download in the client usually just re-imports.
            DownloadRow::Arr { service, id, title, .. } => Modal::Confirm {
                msg: format!("Remove \"{title}\" from the {} queue?", service.label().to_lowercase()),
                toggle_label: Some("also remove from download client"),
                toggle: true,
                action: ConfirmAction::RemoveQueueItem {
                    radarr: service == Service::Radarr,
                    id,
                    blocklist: false,
                },
            },
            DownloadRow::Torrent { hash, name, .. } => Modal::Confirm {
                msg: format!("Delete torrent \"{name}\"?"),
                toggle_label: Some("also delete data"),
                toggle: false,
                action: ConfirmAction::DeleteTorrent { hash, name },
            },
            DownloadRow::Nzb { id, name, .. } => Modal::Confirm {
                msg: format!("Delete \"{name}\" from the NZBGet queue?"),
                toggle_label: None,
                toggle: false,
                action: ConfirmAction::DeleteNzb { id, name },
            },
        };
        self.modal = Some(modal);
    }

    fn downloads_blocklist(&mut self) {
        let Some(DownloadRow::Arr { service, id, title, .. }) = self.selected_row() else {
            self.toast_err("blocklist applies to radarr/sonarr queue rows");
            return;
        };
        self.modal = Some(Modal::Confirm {
            msg: format!("Blocklist \"{title}\" and remove it from the queue?"),
            toggle_label: Some("also remove from download client"),
            toggle: true,
            action: ConfirmAction::RemoveQueueItem {
                radarr: service == Service::Radarr,
                id,
                blocklist: true,
            },
        });
    }

    pub(crate) fn draw_downloads(&mut self, f: &mut Frame, area: Rect) {
        let rows = self.download_rows();
        let title = format!("Downloads ({})", rows.len());
        let block = theme::panel(&title, theme::QBIT, true);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut errors = Vec::new();
        if let Some(e) = self.downloads.radarr_queue.failed() {
            errors.push(format!("radarr: {e}"));
        }
        if let Some(e) = self.downloads.sonarr_queue.failed() {
            errors.push(format!("sonarr: {e}"));
        }
        if let Some(e) = self.downloads.torrents.failed() {
            errors.push(format!("qbit: {e}"));
        }
        if let Some(e) = self.downloads.nzb.failed() {
            errors.push(format!("nzbget: {e}"));
        }
        let inner = super::draw_fetch_errors(f, inner, &errors);

        let loading = self.downloads.radarr_queue.is_loading()
            || self.downloads.sonarr_queue.is_loading()
            || self.downloads.torrents.is_loading()
            || self.downloads.nzb.is_loading();
        if rows.is_empty() {
            if loading {
                let msg = format!("{} loading…", self.spinner());
                f.render_widget(Paragraph::new(msg).style(theme::dim()), inner);
            } else if errors.is_empty() {
                let msg = "no active downloads";
                f.render_widget(Paragraph::new(msg).style(theme::dim()), inner);
            }
            return;
        }

        let width = inner.width as usize;
        let items: Vec<ListItem> = rows
            .iter()
            .map(|row| {
                let (accent, tag, name, status, right) = match row {
                    DownloadRow::Arr { service, title, status, progress, timeleft, .. } => (
                        service.accent(),
                        service.label(),
                        title.clone(),
                        status.clone(),
                        format!(
                            "{} {}",
                            progress.map(|p| format!("{p:.0}%")).unwrap_or_default(),
                            timeleft
                        ),
                    ),
                    DownloadRow::Torrent { name, state, paused, progress, speed, eta, .. } => (
                        theme::QBIT,
                        "QBIT",
                        name.clone(),
                        if *paused { format!("⏸ {state}") } else { state.clone() },
                        format!("{progress:.0}% {} {}", fmt_speed(*speed as f64), fmt_eta_secs(*eta)),
                    ),
                    DownloadRow::Nzb { name, status, progress, size_mb, .. } => (
                        theme::NZBGET,
                        "NZB",
                        name.clone(),
                        status.clone(),
                        format!("{progress:.0}% {}", fmt_bytes(*size_mb as f64 * 1024.0 * 1024.0)),
                    ),
                };
                let bar = progress_bar(row, 20);
                let left = format!("{tag:<7}");
                let name_width = width.saturating_sub(left.len() + bar.len() + right.len() + status.len() + 6);
                ListItem::new(Line::from(vec![
                    Span::styled(left, theme::accent_bold(accent)),
                    Span::raw(format!("{:<w$.w$} ", name, w = name_width)),
                    Span::styled(format!("{status} "), theme::dim()),
                    Span::styled(bar, theme::accent_bold(accent)),
                    Span::styled(format!(" {right}"), theme::dim()),
                ]))
            })
            .collect();
        let list = List::new(items)
            .highlight_style(theme::selected_row())
            .highlight_symbol(theme::SELECT_MARKER);
        let mut state = ListState::default().with_selected(Some(self.downloads.selected));
        f.render_stateful_widget(list, inner, &mut state);
    }
}

fn progress_bar(row: &DownloadRow, width: usize) -> String {
    let pct = match row {
        DownloadRow::Arr { progress, .. } => progress.unwrap_or(0.0),
        DownloadRow::Torrent { progress, .. } => *progress,
        DownloadRow::Nzb { progress, .. } => *progress,
    };
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    format!(
        "{}{}",
        "█".repeat(filled.min(width)),
        "░".repeat(width.saturating_sub(filled))
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::api::models::arr::QueueItem;
    use crate::api::Clients;
    use crate::config::{ApiKeyService, Config, PosterProtocol};
    use crate::tui::event::{DataMsg, Event};
    use crate::tui::posters::PosterManager;

    fn app() -> App {
        app_with_config(&Config::default()).0
    }

    fn app_with_config(config: &Config) -> (App, tokio::sync::mpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let posters =
            PosterManager::detect(PosterProtocol::Off, tx.clone(), crate::api::http::build_client());
        (App::new(Arc::new(Clients::from_config(config)), tx, posters), rx)
    }

    fn queue_item(id: i64, title: &str) -> QueueItem {
        serde_json::from_value(serde_json::json!({ "id": id, "title": title })).unwrap()
    }

    fn render_downloads(app: &mut App) -> String {
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| app.draw_downloads(f, f.area())).unwrap();
        term.backend().to_string()
    }

    #[test]
    fn sonarr_only_loading_shows_spinner_not_empty_message() {
        let mut app = app();
        app.downloads.sonarr_queue = Loadable::Loading;

        let out = render_downloads(&mut app);
        assert!(out.contains("loading"), "sonarr load in flight must read as loading: {out}");
        assert!(!out.contains("no active downloads"), "must not claim empty while loading: {out}");
    }

    #[test]
    fn failed_queues_are_reported_not_hidden() {
        let mut app = app();
        app.downloads.sonarr_queue = Loadable::Failed("connection refused".into());
        app.downloads.torrents = Loadable::Ready(vec![]);

        let out = render_downloads(&mut app);
        assert!(out.contains("sonarr: connection refused"), "error must be shown: {out}");
        assert!(!out.contains("no active downloads"), "failure must not read as empty: {out}");
    }

    #[test]
    fn all_ready_and_empty_reads_as_no_active_downloads() {
        let mut app = app();
        app.downloads.radarr_queue = Loadable::Ready(vec![]);
        app.downloads.sonarr_queue = Loadable::Ready(vec![]);
        app.downloads.torrents = Loadable::Ready(vec![]);
        app.downloads.nzb = Loadable::Ready(vec![]);

        let out = render_downloads(&mut app);
        assert!(out.contains("no active downloads"), "genuinely empty stays friendly: {out}");
    }

    #[test]
    fn stale_downloads_refresh_is_dropped() {
        let mut app = app();
        // Two overlapping refreshes: seq 2 is current, seq 1's slow response
        // arrives after seq 2's and must not overwrite it.
        app.downloads.seq = 2;
        app.handle(Event::Data(DataMsg::RadarrQueue {
            seq: 2,
            result: Ok(vec![queue_item(1, "new")]),
        }));
        app.handle(Event::Data(DataMsg::RadarrQueue {
            seq: 1,
            result: Ok(vec![queue_item(9, "old")]),
        }));

        let rows = app.download_rows();
        assert_eq!(rows.len(), 1, "stale refresh must not replace current data");
        match &rows[0] {
            DownloadRow::Arr { id, title, .. } => {
                assert_eq!((*id, title.as_str()), (1, "new"));
            }
            _ => panic!("expected the arr queue row"),
        }
    }

    /// Drive the confirm modal end-to-end against a mock Radarr and assert
    /// the exact query the destructive action sends.
    async fn remove_queue_item_via_modal(toggle_off_client_removal: bool) {
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let expect_remove = (!toggle_off_client_removal).to_string();
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v3/queue/42"))
            .and(query_param("blocklist", "false"))
            .and(query_param("removeFromClient", expect_remove.as_str()))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let config = Config {
            radarr: Some(ApiKeyService { url: server.uri(), api_key: "k".into() }),
            ..Config::default()
        };
        let (mut app, mut rx) = app_with_config(&config);
        app.downloads.radarr_queue = Loadable::Ready(vec![queue_item(42, "stuck download")]);

        app.downloads_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(
            matches!(app.modal, Some(Modal::Confirm { toggle: true, toggle_label: Some(_), .. })),
            "removal confirm must expose the remove-from-client toggle, default on"
        );
        if toggle_off_client_removal {
            app.handle_modal_key(KeyEvent::from(KeyCode::Char('f')));
        }
        app.handle_modal_key(KeyEvent::from(KeyCode::Enter));

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Event::Data(DataMsg::ActionDone { result, .. }) =
                    rx.recv().await.expect("event channel stays open")
                {
                    break result;
                }
            }
        })
        .await
        .expect("queue removal must complete");
        result.expect("queue delete must succeed");
        // MockServer verifies the expected request (incl. query) on drop.
    }

    #[tokio::test]
    async fn queue_removal_defaults_to_removing_from_download_client() {
        remove_queue_item_via_modal(false).await;
    }

    #[tokio::test]
    async fn queue_removal_toggle_keeps_download_in_client() {
        remove_queue_item_via_modal(true).await;
    }
}

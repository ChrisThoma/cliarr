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

    fn downloads_pause_resume(&mut self, pause: bool) {
        let Some(row) = self.selected_row() else { return };
        let verb = if pause { "pausing" } else { "resuming" };
        match row {
            DownloadRow::Torrent { hash, name, .. } => {
                let Some(qbit) = self.clients.qbit.clone() else { return };
                fetch::action(self.tx.clone(), format!("{verb} {name}"), async move {
                    if pause {
                        qbit.pause(&[hash]).await
                    } else {
                        qbit.resume(&[hash]).await
                    }
                });
            }
            DownloadRow::Nzb { id, name, .. } => {
                let Some(nzbget) = self.clients.nzbget.clone() else { return };
                fetch::action(self.tx.clone(), format!("{verb} {name}"), async move {
                    if pause {
                        nzbget.pause(&[id]).await.map(|_| ())
                    } else {
                        nzbget.resume(&[id]).await.map(|_| ())
                    }
                });
            }
            DownloadRow::Arr { .. } => {
                self.toast_err("pause/resume applies to torrent/nzb rows — arr queue items follow their download client");
            }
        }
    }

    fn downloads_delete(&mut self) {
        let Some(row) = self.selected_row() else { return };
        let modal = match row {
            DownloadRow::Arr { service, id, title, .. } => Modal::Confirm {
                msg: format!("Remove \"{title}\" from the {} queue?", service.label().to_lowercase()),
                toggle_label: None,
                toggle: false,
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
            toggle_label: None,
            toggle: false,
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

        let loading = self.downloads.torrents.is_loading()
            || self.downloads.radarr_queue.is_loading()
            || self.downloads.nzb.is_loading();
        if rows.is_empty() {
            let msg = if loading {
                format!("{} loading…", self.spinner())
            } else {
                "no active downloads".to_string()
            };
            f.render_widget(Paragraph::new(msg).style(theme::dim()), inner);
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

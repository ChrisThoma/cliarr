use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::app::{AddField, App, ConfirmAction, Modal};
use crate::tui::event::Loadable;
use crate::tui::{fetch, theme};

impl App {
    pub(crate) fn handle_modal_key(&mut self, key: KeyEvent) {
        match self.modal.take() {
            None => {}
            Some(Modal::Help) => {
                // Any key closes help.
            }
            Some(Modal::Confirm { msg, toggle_label, mut toggle, action }) => match key.code {
                KeyCode::Enter | KeyCode::Char('y') => self.execute_confirm(action, toggle),
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {}
                KeyCode::Char('f') if toggle_label.is_some() => {
                    toggle = !toggle;
                    self.modal = Some(Modal::Confirm { msg, toggle_label, toggle, action });
                }
                _ => self.modal = Some(Modal::Confirm { msg, toggle_label, toggle, action }),
            },
            Some(Modal::Add(mut add)) => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => self.execute_add(add),
                KeyCode::Tab | KeyCode::Char('j') | KeyCode::Down => {
                    add.field = add_field_next(add.field);
                    self.modal = Some(Modal::Add(add));
                }
                KeyCode::BackTab | KeyCode::Char('k') | KeyCode::Up => {
                    add.field = add_field_prev(add.field);
                    self.modal = Some(Modal::Add(add));
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    add_cycle(&mut add, -1);
                    self.modal = Some(Modal::Add(add));
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Char(' ') => {
                    add_cycle(&mut add, 1);
                    self.modal = Some(Modal::Add(add));
                }
                _ => self.modal = Some(Modal::Add(add)),
            },
        }
    }

    fn execute_confirm(&mut self, action: ConfirmAction, toggle: bool) {
        let tx = self.tx.clone();
        match action {
            ConfirmAction::DeleteMovie { id, title } => {
                let Some(radarr) = self.clients.radarr.clone() else { return };
                fetch::action(tx, format!("removed {title}"), async move {
                    radarr.delete_movie(id, toggle, false).await
                });
            }
            ConfirmAction::DeleteSeries { id, title } => {
                let Some(sonarr) = self.clients.sonarr.clone() else { return };
                fetch::action(tx, format!("removed {title}"), async move {
                    sonarr.delete_series(id, toggle, false).await
                });
            }
            ConfirmAction::RemoveQueueItem { radarr, id, blocklist } => {
                let desc = if blocklist {
                    format!("blocklisted queue item {id}")
                } else {
                    format!("removed queue item {id}")
                };
                if radarr {
                    let Some(client) = self.clients.radarr.clone() else { return };
                    fetch::action(tx, desc, async move {
                        client.queue_delete(id, blocklist, true).await
                    });
                } else {
                    let Some(client) = self.clients.sonarr.clone() else { return };
                    fetch::action(tx, desc, async move {
                        client.queue_delete(id, blocklist, true).await
                    });
                }
            }
            ConfirmAction::DeleteTorrent { hash, name } => {
                let Some(qbit) = self.clients.qbit.clone() else { return };
                fetch::action(tx, format!("deleted {name}"), async move {
                    qbit.delete(&[hash], toggle).await
                });
            }
            ConfirmAction::DeleteNzb { id, name } => {
                let Some(nzbget) = self.clients.nzbget.clone() else { return };
                fetch::action(tx, format!("deleted {name}"), async move {
                    nzbget.delete(&[id]).await.map(|_| ())
                });
            }
        }
    }

    fn execute_add(&mut self, add: crate::tui::app::AddModal) {
        let Loadable::Ready((profiles, roots)) = &add.options else {
            self.modal = Some(Modal::Add(add));
            return;
        };
        let Some(profile) = profiles.get(add.profile_idx) else {
            self.toast_err("no quality profile selected");
            return;
        };
        let Some(root) = roots.get(add.root_idx) else {
            self.toast_err("no root folder selected");
            return;
        };
        let (profile_id, root_path) = (profile.id, root.path.clone());
        let tx = self.tx.clone();

        if let Some(movie) = add.movie.clone() {
            let Some(radarr) = self.clients.radarr.clone() else { return };
            let title = movie.title.clone();
            let (monitored, search) = (add.monitored, add.search_on_add);
            fetch::action(tx, format!("added {title}"), async move {
                radarr
                    .add_movie(&movie, profile_id, &root_path, monitored, search)
                    .await
                    .map(|_| ())
            });
        } else if let Some(series) = add.series.clone() {
            let Some(sonarr) = self.clients.sonarr.clone() else { return };
            let title = series.title.clone();
            let (monitored, search) = (add.monitored, add.search_on_add);
            fetch::action(tx, format!("added {title}"), async move {
                sonarr
                    .add_series(&series, profile_id, &root_path, monitored, true, search)
                    .await
                    .map(|_| ())
            });
        }
    }

    pub(crate) fn draw_modal(&mut self, f: &mut Frame, area: Rect) {
        let Some(modal) = &self.modal else { return };
        match modal {
            Modal::Help => draw_help(f, area),
            Modal::Confirm { msg, toggle_label, toggle, .. } => {
                let rect = centered_rect(area, 60, 9);
                f.render_widget(Clear, rect);
                f.render_widget(Block::new().style(Style::new().bg(theme::SURFACE)), rect);
                let block = theme::panel("Confirm", theme::WARN, true);
                let inner = block.inner(rect);
                f.render_widget(block, rect);
                let mut lines = vec![Line::raw(msg.clone()), Line::raw("")];
                if let Some(label) = toggle_label {
                    lines.push(Line::from(vec![
                        Span::styled("f ", theme::accent_bold(theme::ACCENT)),
                        Span::raw(format!("{label}: ")),
                        Span::styled(
                            if *toggle { "YES" } else { "no" },
                            if *toggle {
                                theme::accent_bold(theme::ERROR)
                            } else {
                                theme::dim()
                            },
                        ),
                    ]));
                    lines.push(Line::raw(""));
                }
                lines.push(Line::from(vec![
                    Span::styled("Enter ", theme::accent_bold(theme::SUCCESS)),
                    Span::styled("confirm   ", theme::dim()),
                    Span::styled("Esc ", theme::accent_bold(theme::ERROR)),
                    Span::styled("cancel", theme::dim()),
                ]));
                f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
            }
            Modal::Add(add) => {
                let rect = centered_rect(area, 62, 13);
                f.render_widget(Clear, rect);
                f.render_widget(Block::new().style(Style::new().bg(theme::SURFACE)), rect);
                let title = format!("Add · {}", add.title());
                let block = theme::panel(&title, theme::SUCCESS, true);
                let inner = block.inner(rect);
                f.render_widget(block, rect);

                let mut lines: Vec<Line> = Vec::new();
                match &add.options {
                    Loadable::Ready((profiles, roots)) => {
                        let profile = profiles
                            .get(add.profile_idx)
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                        let root = roots.get(add.root_idx).map(|r| r.path.clone()).unwrap_or_default();
                        lines.push(field_line(add.field == AddField::Profile, "Quality profile", &format!("◂ {profile} ▸")));
                        lines.push(field_line(add.field == AddField::Root, "Root folder", &format!("◂ {root} ▸")));
                        lines.push(field_line(
                            add.field == AddField::Monitored,
                            "Monitored",
                            if add.monitored { "yes" } else { "no" },
                        ));
                        lines.push(field_line(
                            add.field == AddField::SearchOnAdd,
                            "Search on add",
                            if add.search_on_add { "yes" } else { "no" },
                        ));
                        lines.push(Line::raw(""));
                        let confirm_style = if add.field == AddField::Confirm {
                            theme::accent_bold(theme::SUCCESS)
                        } else {
                            theme::dim()
                        };
                        lines.push(Line::styled("[ Add ]  (Enter)", confirm_style));
                    }
                    Loadable::Loading => lines.push(Line::raw("loading profiles and folders…")),
                    Loadable::Failed(e) => {
                        lines.push(Line::styled(e.clone(), theme::accent_bold(theme::ERROR)))
                    }
                    Loadable::NotAsked => {}
                }
                lines.push(Line::raw(""));
                lines.push(Line::styled(
                    "j/k fields · h/l change · Enter add · Esc cancel",
                    theme::dim(),
                ));
                f.render_widget(Paragraph::new(lines), inner);
            }
        }
    }
}

fn field_line<'a>(focused: bool, label: &'a str, value: &str) -> Line<'a> {
    let marker = if focused { theme::SELECT_MARKER } else { " " };
    let style = if focused {
        theme::accent_bold(theme::FG)
    } else {
        theme::dim()
    };
    Line::from(vec![
        Span::styled(marker, theme::accent_bold(theme::ACCENT)),
        Span::styled(format!(" {label:<16}"), style),
        Span::styled(value.to_string(), style),
    ])
}

fn add_field_next(f: AddField) -> AddField {
    match f {
        AddField::Profile => AddField::Root,
        AddField::Root => AddField::Monitored,
        AddField::Monitored => AddField::SearchOnAdd,
        AddField::SearchOnAdd => AddField::Confirm,
        AddField::Confirm => AddField::Profile,
    }
}

fn add_field_prev(f: AddField) -> AddField {
    match f {
        AddField::Profile => AddField::Confirm,
        AddField::Root => AddField::Profile,
        AddField::Monitored => AddField::Root,
        AddField::SearchOnAdd => AddField::Monitored,
        AddField::Confirm => AddField::SearchOnAdd,
    }
}

fn add_cycle(add: &mut crate::tui::app::AddModal, delta: i64) {
    let Loadable::Ready((profiles, roots)) = &add.options else { return };
    match add.field {
        AddField::Profile => {
            if !profiles.is_empty() {
                let len = profiles.len() as i64;
                add.profile_idx = ((add.profile_idx as i64 + delta).rem_euclid(len)) as usize;
            }
        }
        AddField::Root => {
            if !roots.is_empty() {
                let len = roots.len() as i64;
                add.root_idx = ((add.root_idx as i64 + delta).rem_euclid(len)) as usize;
            }
        }
        AddField::Monitored => add.monitored = !add.monitored,
        AddField::SearchOnAdd => add.search_on_add = !add.search_on_add,
        AddField::Confirm => {}
    }
}

fn draw_help(f: &mut Frame, area: Rect) {
    let rect = centered_rect(area, 64, 20);
    f.render_widget(Clear, rect);
    f.render_widget(Block::new().style(Style::new().bg(theme::SURFACE)), rect);
    let block = theme::panel("Help", theme::ACCENT, true);
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let rows: [(&str, &str); 16] = [
        ("1-6 / Tab", "switch tab"),
        ("j/k or ↑/↓", "move selection"),
        ("r", "refresh current tab"),
        ("q / Ctrl-C", "quit"),
        ("", ""),
        ("Search:  type", "results appear live (movies + series)"),
        ("↑/↓ · Enter", "select a result · add it to the library"),
        ("Esc", "stop typing (j/k, tabs work again)"),
        ("", ""),
        ("Library:  /", "filter · S search release · d delete"),
        ("", ""),
        ("Downloads:  p / P", "pause / resume (qbit + nzbget)"),
        ("d", "delete download"),
        ("b", "blocklist arr queue item"),
        ("", ""),
        ("Missing:  S / A", "search selected / search everything"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(format!("{key:<14}"), theme::accent_bold(theme::ACCENT)),
                Span::styled(desc.to_string(), Style::new().fg(theme::FG)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let [_, mid, _] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(height),
        Constraint::Min(0),
    ])
    .areas(area);
    let [_, rect, _] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .areas(mid);
    rect
}

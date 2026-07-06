use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::event::Loadable;
use crate::tui::theme::{self, Service};

impl App {
    pub(crate) fn draw_dashboard(&mut self, f: &mut Frame, area: Rect) {
        let [cards_area, body] =
            Layout::vertical([Constraint::Length(4), Constraint::Min(0)]).areas(area);

        // Service health cards.
        let card_areas = Layout::horizontal([Constraint::Ratio(1, 5); 5]).split(cards_area);
        let spinner = self.spinner();
        let cards: [(Service, bool, Vec<Span>); 5] = [
            health_card(Service::Radarr, self.clients.radarr.is_some(), &self.dash.radarr_line(spinner)),
            health_card(Service::Sonarr, self.clients.sonarr.is_some(), &self.dash.sonarr_line(spinner)),
            health_card(Service::Plex, self.clients.plex.is_some(), &self.dash.plex_line(spinner)),
            health_card(Service::Qbit, self.clients.qbit.is_some(), &self.dash.qbit_line(spinner)),
            health_card(Service::Nzbget, self.clients.nzbget.is_some(), &self.dash.nzbget_line(spinner)),
        ];
        for (i, (service, configured, line)) in cards.into_iter().enumerate() {
            let block = theme::panel(service.label(), service.accent(), configured);
            let inner = block.inner(card_areas[i]);
            f.render_widget(block, card_areas[i]);
            f.render_widget(Paragraph::new(Line::from(line)), inner);
        }

        let [library_area, playing_area] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(body);

        // Library counts.
        let mut lines: Vec<Line> = Vec::new();
        match &self.movies {
            Loadable::Ready(movies) => {
                let with_file = movies.iter().filter(|m| m.has_file.unwrap_or(false)).count();
                lines.push(Line::from(vec![
                    Span::styled("● ", theme::accent_bold(theme::RADARR)),
                    Span::raw(format!("{} movies ({} downloaded)", movies.len(), with_file)),
                ]));
            }
            Loadable::Loading => lines.push(Line::from(format!("{spinner} loading movies…"))),
            Loadable::Failed(e) => lines.push(Line::styled(format!("movies: {e}"), theme::accent_bold(theme::ERROR))),
            Loadable::NotAsked => {}
        }
        match &self.series {
            Loadable::Ready(series) => {
                let episodes: i64 = series
                    .iter()
                    .filter_map(|s| s.statistics.as_ref().and_then(|st| st.episode_file_count))
                    .sum();
                lines.push(Line::from(vec![
                    Span::styled("● ", theme::accent_bold(theme::SONARR)),
                    Span::raw(format!("{} series ({} episodes on disk)", series.len(), episodes)),
                ]));
            }
            Loadable::Loading => lines.push(Line::from(format!("{spinner} loading series…"))),
            Loadable::Failed(e) => lines.push(Line::styled(format!("series: {e}"), theme::accent_bold(theme::ERROR))),
            Loadable::NotAsked => {}
        }
        let block = theme::panel("Library", theme::ACCENT, true);
        let inner = block.inner(library_area);
        f.render_widget(block, library_area);
        f.render_widget(Paragraph::new(lines), inner);

        // Plex now playing.
        let mut lines: Vec<Line> = Vec::new();
        match &self.dash.sessions {
            Loadable::Ready(sessions) if sessions.sessions.is_empty() => {
                lines.push(Line::styled("nothing playing", theme::dim()));
            }
            Loadable::Ready(sessions) => {
                for s in &sessions.sessions {
                    let user = s.user.as_ref().and_then(|u| u.title.clone()).unwrap_or_default();
                    let state = s
                        .player
                        .as_ref()
                        .and_then(|p| p.state.clone())
                        .unwrap_or_default();
                    let progress = match (s.view_offset, s.duration) {
                        (Some(o), Some(d)) if d > 0 => format!(" · {:.0}%", o as f64 / d as f64 * 100.0),
                        _ => String::new(),
                    };
                    lines.push(Line::from(vec![
                        Span::styled("▶ ", theme::accent_bold(theme::PLEX)),
                        Span::raw(s.display_title()),
                        Span::styled(format!("  {user} · {state}{progress}"), theme::dim()),
                    ]));
                }
            }
            Loadable::Loading => lines.push(Line::from(format!("{spinner} loading…"))),
            Loadable::Failed(e) => lines.push(Line::styled(e.clone(), theme::accent_bold(theme::ERROR))),
            Loadable::NotAsked => lines.push(Line::styled("plex not configured", theme::dim())),
        }
        let block = theme::panel("Now Playing", theme::PLEX, true);
        let inner = block.inner(playing_area);
        f.render_widget(block, playing_area);
        f.render_widget(Paragraph::new(lines), inner);
    }
}

fn health_card<'a>(service: Service, configured: bool, line: &[Span<'a>]) -> (Service, bool, Vec<Span<'a>>) {
    (service, configured, line.to_vec())
}

impl crate::tui::app::DashboardState {
    fn status_line(status: &Loadable<String>, configured_hint: &str, spinner: &str) -> Vec<Span<'static>> {
        match status {
            Loadable::Ready(v) => vec![
                Span::styled("● ", theme::accent_bold(theme::SUCCESS)),
                Span::raw(v.clone()),
            ],
            Loadable::Loading => vec![Span::styled(format!("{spinner} checking…"), theme::dim())],
            Loadable::Failed(e) => vec![
                Span::styled("● ", theme::accent_bold(theme::ERROR)),
                Span::styled(truncate(e, 24), theme::accent_bold(theme::ERROR)),
            ],
            Loadable::NotAsked => vec![Span::styled(configured_hint.to_string(), theme::dim())],
        }
    }

    pub(crate) fn radarr_line(&self, spinner: &str) -> Vec<Span<'static>> {
        let as_string = match &self.radarr {
            Loadable::Ready(s) => Loadable::Ready(format!("v{}", s.version)),
            Loadable::Loading => Loadable::Loading,
            Loadable::Failed(e) => Loadable::Failed(e.clone()),
            Loadable::NotAsked => Loadable::NotAsked,
        };
        Self::status_line(&as_string, "not configured", spinner)
    }

    pub(crate) fn sonarr_line(&self, spinner: &str) -> Vec<Span<'static>> {
        let as_string = match &self.sonarr {
            Loadable::Ready(s) => Loadable::Ready(format!("v{}", s.version)),
            Loadable::Loading => Loadable::Loading,
            Loadable::Failed(e) => Loadable::Failed(e.clone()),
            Loadable::NotAsked => Loadable::NotAsked,
        };
        Self::status_line(&as_string, "not configured", spinner)
    }

    pub(crate) fn plex_line(&self, spinner: &str) -> Vec<Span<'static>> {
        Self::status_line(&self.plex, "not configured", spinner)
    }

    pub(crate) fn qbit_line(&self, spinner: &str) -> Vec<Span<'static>> {
        Self::status_line(&self.qbit, "not configured", spinner)
    }

    pub(crate) fn nzbget_line(&self, spinner: &str) -> Vec<Span<'static>> {
        Self::status_line(&self.nzbget, "not configured", spinner)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}

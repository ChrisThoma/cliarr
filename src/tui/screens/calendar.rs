use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::event::Loadable;
use crate::tui::theme;

impl App {
    pub(crate) fn draw_calendar(&mut self, f: &mut Frame, area: Rect) {
        let block = theme::panel("Upcoming (14 days)", theme::ACCENT, true);
        let inner = block.inner(area);
        f.render_widget(block, area);

        match &self.calendar.entries {
            Loadable::Ready(entries) if entries.is_empty() => {
                f.render_widget(
                    Paragraph::new("nothing upcoming in the next 14 days").style(theme::dim()),
                    inner,
                );
            }
            Loadable::Ready(entries) => {
                let mut lines: Vec<Line> = Vec::new();
                let mut current_date = "";
                for e in entries {
                    if e.date != current_date {
                        current_date = &e.date;
                        if !lines.is_empty() {
                            lines.push(Line::raw(""));
                        }
                        lines.push(Line::styled(
                            pretty_date(&e.date),
                            theme::accent_bold(theme::ACCENT),
                        ));
                    }
                    let accent = if e.service == "radarr" { theme::RADARR } else { theme::SONARR };
                    lines.push(Line::from(vec![
                        Span::styled("  ● ", theme::accent_bold(accent)),
                        Span::raw(e.title.clone()),
                        Span::styled(format!("  {}", e.detail), theme::dim()),
                    ]));
                }
                f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
            }
            Loadable::Loading => {
                f.render_widget(
                    Paragraph::new(format!("{} loading…", self.spinner())).style(theme::dim()),
                    inner,
                );
            }
            Loadable::Failed(e) => {
                f.render_widget(
                    Paragraph::new(e.clone()).style(theme::accent_bold(theme::ERROR)),
                    inner,
                );
            }
            Loadable::NotAsked => {}
        }
    }
}

fn pretty_date(iso: &str) -> String {
    match iso.parse::<chrono::NaiveDate>() {
        Ok(d) => {
            let today = chrono::Local::now().date_naive();
            let label = if d == today {
                " · today"
            } else if d == today + chrono::Duration::days(1) {
                " · tomorrow"
            } else {
                ""
            };
            format!("{}{}", d.format("%A %-d %B"), label)
        }
        Err(_) => iso.to_string(),
    }
}

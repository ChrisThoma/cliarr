pub mod calendar;
pub mod dashboard;
pub mod downloads;
pub mod library;
pub mod missing;
pub mod modal;
pub mod search;

use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme;

/// Render per-service fetch errors on the first line of `area` and return
/// the remaining area for the screen's list. Screens that aggregate several
/// services use this so a failed service is reported instead of silently
/// rendering as "nothing here".
pub(crate) fn draw_fetch_errors(f: &mut Frame, area: Rect, errors: &[String]) -> Rect {
    if errors.is_empty() {
        return area;
    }
    let line = Rect { height: 1.min(area.height), ..area };
    f.render_widget(
        Paragraph::new(format!("✗ {}", errors.join(" · "))).style(theme::accent_bold(theme::ERROR)),
        line,
    );
    Rect {
        y: area.y.saturating_add(1),
        height: area.height.saturating_sub(1),
        ..area
    }
}

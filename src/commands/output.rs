use comfy_table::{presets, Attribute, Cell, CellAlignment, ContentArrangement, Table};
use serde::Serialize;

use crate::error::{CliarrError, Result};

pub fn table(headers: &[&str]) -> Table {
    let mut t = Table::new();
    t.load_preset(presets::UTF8_BORDERS_ONLY)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(
            headers
                .iter()
                .map(|h| Cell::new(h.to_uppercase()).add_attribute(Attribute::Bold))
                .collect::<Vec<_>>(),
        );
    t
}

pub fn right(s: impl ToString) -> Cell {
    Cell::new(s.to_string()).set_alignment(CellAlignment::Right)
}

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let out = serde_json::to_string_pretty(value)
        .map_err(|e| CliarrError::Other(format!("cannot serialize output: {e}")))?;
    crate::outln!("{out}");
    Ok(())
}

/// Write one line to stdout. `println!` panics when the reader goes away
/// (`cliarr movie list | head`); a broken pipe is normal termination for a
/// piped CLI, so exit quietly instead.
pub fn write_line(args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let res = out.write_fmt(args).and_then(|()| out.write_all(b"\n"));
    if let Err(e) = res {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("error: cannot write to stdout: {e}");
        std::process::exit(1);
    }
}

/// Drop-in `println!` replacement for command output; see [`write_line`].
#[macro_export]
macro_rules! outln {
    () => { $crate::commands::output::write_line(format_args!("")) };
    ($($arg:tt)*) => { $crate::commands::output::write_line(format_args!($($arg)*)) };
}

pub fn fmt_bytes(bytes: f64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{v:.0} {}", UNITS[unit])
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

pub fn fmt_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec <= 0.0 {
        "—".into()
    } else {
        format!("{}/s", fmt_bytes(bytes_per_sec))
    }
}

pub fn fmt_eta_secs(secs: i64) -> String {
    // qBittorrent uses 8640000 as "infinity"
    if secs <= 0 || secs >= 8_640_000 {
        return "—".into();
    }
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

pub fn check_mark(b: bool) -> &'static str {
    if b { "✓" } else { "✗" }
}

pub mod calendar;
pub mod config_cmd;
pub mod missing;
pub mod movie;
pub mod nzb;
pub mod output;
pub mod plex_cmd;
pub mod queue;
pub mod series;
pub mod torrents;

use std::io::{IsTerminal, Write};

use crate::error::{CliarrError, Result};

/// Read one trimmed line from stdin after printing a prompt.
pub fn prompt(msg: &str) -> Result<String> {
    print!("{msg}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| CliarrError::Other(format!("cannot read input: {e}")))?;
    Ok(line.trim().to_string())
}

/// Numbered pick list. Errors (rather than prompting) when stdin isn't a TTY,
/// listing the valid choices so scripts know what to pass.
pub fn choose<'a, T>(what: &str, items: &'a [T], label: impl Fn(&T) -> String) -> Result<&'a T> {
    if items.is_empty() {
        return Err(CliarrError::Other(format!("no {what} available")));
    }
    if items.len() == 1 {
        return Ok(&items[0]);
    }
    if !std::io::stdin().is_terminal() {
        let opts: Vec<String> = items.iter().map(&label).collect();
        return Err(CliarrError::Other(format!(
            "multiple {what} available, pass one explicitly: {}",
            opts.join(", ")
        )));
    }
    eprintln!("Select {what}:");
    for (i, item) in items.iter().enumerate() {
        eprintln!("  {}. {}", i + 1, label(item));
    }
    let answer = prompt("> ")?;
    let idx: usize = answer
        .parse()
        .map_err(|_| CliarrError::Other(format!("invalid selection: {answer}")))?;
    items
        .get(idx.saturating_sub(1))
        .ok_or_else(|| CliarrError::Other(format!("invalid selection: {answer}")))
}

/// Ask a yes/no question, defaulting to no. Non-TTY stdin means no.
pub fn confirm(msg: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    let answer = prompt(&format!("{msg} [y/N] "))?;
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

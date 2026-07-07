use serde_json::json;

use crate::api::Clients;
use crate::cli::PlexCmd;
use crate::commands::output;
use crate::error::Result;

pub async fn run(cmd: PlexCmd, clients: &Clients, json_out: bool) -> Result<()> {
    let plex = clients.plex()?;
    match cmd {
        PlexCmd::Status => {
            let (identity, sections, sessions) =
                tokio::try_join!(plex.identity(), plex.sections(), plex.sessions())?;
            if json_out {
                return output::print_json(&json!({
                    "identity": identity,
                    "sections": sections.directories,
                    "sessions": sessions.sessions,
                }));
            }
            crate::outln!(
                "{} — Plex {}",
                identity.friendly_name.as_deref().unwrap_or("Plex server"),
                identity.version
            );
            crate::outln!("\nLibraries:");
            for s in &sections.directories {
                crate::outln!("  {} ({})", s.title, s.kind);
            }
            if sessions.sessions.is_empty() {
                crate::outln!("\nNothing playing");
            } else {
                crate::outln!("\nNow playing:");
                for s in &sessions.sessions {
                    let user = s
                        .user
                        .as_ref()
                        .and_then(|u| u.title.clone())
                        .unwrap_or_else(|| "?".into());
                    let player = s
                        .player
                        .as_ref()
                        .and_then(|p| p.product.clone())
                        .unwrap_or_default();
                    let progress = match (s.view_offset, s.duration) {
                        (Some(o), Some(d)) if d > 0 => format!(" [{:.0}%]", o as f64 / d as f64 * 100.0),
                        _ => String::new(),
                    };
                    crate::outln!("  {} — {user} on {player}{progress}", s.display_title());
                }
            }
        }
    }
    Ok(())
}

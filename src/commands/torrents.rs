use crate::api::Clients;
use crate::cli::TorrentsCmd;
use crate::commands::{confirm, output};
use crate::error::{CliarrError, Result};

pub async fn run(cmd: Option<TorrentsCmd>, clients: &Clients, json: bool) -> Result<()> {
    let qbit = clients.qbit()?;
    match cmd.unwrap_or(TorrentsCmd::List { filter: None }) {
        TorrentsCmd::List { filter } => {
            let torrents = qbit.torrents(filter.as_deref()).await?;
            if json {
                return output::print_json(&torrents);
            }
            if torrents.is_empty() {
                crate::outln!("no torrents");
                return Ok(());
            }
            let mut t = output::table(&["hash", "name", "state", "progress", "size", "speed", "eta"]);
            for tor in &torrents {
                t.add_row(vec![
                    comfy_table::Cell::new(tor.hash.chars().take(8).collect::<String>()),
                    comfy_table::Cell::new(&tor.name),
                    comfy_table::Cell::new(&tor.state),
                    output::right(format!("{:.0}%", tor.progress * 100.0)),
                    output::right(output::fmt_bytes(tor.size as f64)),
                    output::right(output::fmt_speed(tor.dlspeed as f64)),
                    output::right(output::fmt_eta_secs(tor.eta)),
                ]);
            }
            crate::outln!("{t}");
        }
        TorrentsCmd::Pause { hashes } => {
            let hashes = expand_hashes(qbit, &hashes).await?;
            qbit.pause(&hashes).await?;
            crate::outln!("✓ Paused {} torrent(s)", hashes.len());
        }
        TorrentsCmd::Resume { hashes } => {
            let hashes = expand_hashes(qbit, &hashes).await?;
            qbit.resume(&hashes).await?;
            crate::outln!("✓ Resumed {} torrent(s)", hashes.len());
        }
        TorrentsCmd::Delete { hashes, delete_files } => {
            let hashes = expand_hashes(qbit, &hashes).await?;
            let extra = if delete_files { " and its data" } else { "" };
            if !confirm(&format!("Delete {} torrent(s){extra}?", hashes.len()))? {
                crate::outln!("aborted");
                return Ok(());
            }
            qbit.delete(&hashes, delete_files).await?;
            crate::outln!("✓ Deleted {} torrent(s)", hashes.len());
        }
    }
    Ok(())
}

/// Accept unambiguous hash prefixes so users can copy the short hash from
/// `torrents list`.
async fn expand_hashes(
    qbit: &crate::api::qbittorrent::QbitClient,
    prefixes: &[String],
) -> Result<Vec<String>> {
    if prefixes.is_empty() {
        return Err(CliarrError::Other("no torrent hashes given".into()));
    }
    let all = qbit.torrents(None).await?;
    let mut out = Vec::with_capacity(prefixes.len());
    for p in prefixes {
        let matches: Vec<&str> = all
            .iter()
            .filter(|t| t.hash.starts_with(&p.to_lowercase()))
            .map(|t| t.hash.as_str())
            .collect();
        match matches.len() {
            0 => return Err(CliarrError::Other(format!("no torrent matches hash {p}"))),
            1 => out.push(matches[0].to_string()),
            n => return Err(CliarrError::Other(format!("hash prefix {p} is ambiguous ({n} matches)"))),
        }
    }
    Ok(out)
}

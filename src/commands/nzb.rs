use crate::api::Clients;
use crate::cli::NzbCmd;
use crate::commands::{confirm, output};
use crate::error::{CliarrError, Result};

pub async fn run(cmd: Option<NzbCmd>, clients: &Clients, json: bool) -> Result<()> {
    let nzbget = clients.nzbget()?;
    match cmd.unwrap_or(NzbCmd::List) {
        NzbCmd::List => {
            let groups = nzbget.listgroups().await?;
            if json {
                return output::print_json(&groups);
            }
            if groups.is_empty() {
                crate::outln!("no downloads queued");
                return Ok(());
            }
            let mut t = output::table(&["id", "name", "status", "progress", "size", "category"]);
            for g in &groups {
                t.add_row(vec![
                    output::right(g.id),
                    comfy_table::Cell::new(&g.name),
                    comfy_table::Cell::new(&g.status),
                    output::right(format!("{:.0}%", g.progress())),
                    output::right(output::fmt_bytes(g.file_size_mb as f64 * 1024.0 * 1024.0)),
                    comfy_table::Cell::new(&g.category),
                ]);
            }
            crate::outln!("{t}");
        }
        NzbCmd::Pause { ids } => {
            require_ids(&ids)?;
            nzbget.pause(&ids).await?;
            crate::outln!("✓ Paused {} download(s)", ids.len());
        }
        NzbCmd::Resume { ids } => {
            require_ids(&ids)?;
            nzbget.resume(&ids).await?;
            crate::outln!("✓ Resumed {} download(s)", ids.len());
        }
        NzbCmd::Delete { ids } => {
            require_ids(&ids)?;
            if !confirm(&format!("Delete {} download(s) from the queue?", ids.len()))? {
                crate::outln!("aborted");
                return Ok(());
            }
            nzbget.delete(&ids).await?;
            crate::outln!("✓ Deleted {} download(s)", ids.len());
        }
    }
    Ok(())
}

fn require_ids(ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        Err(CliarrError::Other("no download ids given (see `cliarr nzb list`)".into()))
    } else {
        Ok(())
    }
}

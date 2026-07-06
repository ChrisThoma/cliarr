use serde::Serialize;

use crate::api::Clients;
use crate::cli::ArrService;
use crate::commands::output;
use crate::error::Result;

#[derive(Serialize)]
pub struct MissingEntry {
    pub service: &'static str,
    pub id: i64,
    pub title: String,
    pub detail: String,
}

pub async fn run(service: ArrService, clients: &Clients, json: bool) -> Result<()> {
    let mut entries: Vec<MissingEntry> = Vec::new();
    let mut totals: Vec<String> = Vec::new();

    if matches!(service, ArrService::Radarr | ArrService::All) {
        if let Some(radarr) = &clients.radarr {
            let paged = radarr.missing().await?;
            totals.push(format!("{} missing movies", paged.total_records));
            for m in paged.records {
                entries.push(MissingEntry {
                    service: "radarr",
                    id: m.id,
                    title: m.title.clone(),
                    detail: m.year.map(|y| format!("({y})")).unwrap_or_default(),
                });
            }
        } else if service == ArrService::Radarr {
            clients.radarr()?;
        }
    }
    if matches!(service, ArrService::Sonarr | ArrService::All) {
        if let Some(sonarr) = &clients.sonarr {
            let paged = sonarr.missing().await?;
            totals.push(format!("{} missing episodes", paged.total_records));
            for e in paged.records {
                entries.push(MissingEntry {
                    service: "sonarr",
                    id: e.id,
                    title: e.series_title().to_string(),
                    detail: format!("{} — {}", e.code(), e.title.clone().unwrap_or_default()),
                });
            }
        } else if service == ArrService::Sonarr {
            clients.sonarr()?;
        }
    }

    if json {
        return output::print_json(&entries);
    }
    if entries.is_empty() {
        println!("nothing missing 🎉");
        return Ok(());
    }
    let mut t = output::table(&["service", "id", "title", "detail"]);
    for e in &entries {
        t.add_row(vec![
            comfy_table::Cell::new(e.service),
            output::right(e.id),
            comfy_table::Cell::new(&e.title),
            comfy_table::Cell::new(&e.detail),
        ]);
    }
    println!("{t}");
    println!("{}", totals.join(", "));
    println!("hint: `cliarr movie search-missing` / `cliarr series search-missing` to trigger searches");
    Ok(())
}

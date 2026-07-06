use crate::api::Clients;
use crate::cli::{LibraryFilter, SeriesCmd};
use crate::commands::movie::{resolve_profile, resolve_root};
use crate::commands::{confirm, output};
use crate::error::{CliarrError, Result};

pub async fn run(cmd: SeriesCmd, clients: &Clients, json: bool) -> Result<()> {
    let sonarr = clients.sonarr()?;
    match cmd {
        SeriesCmd::Search { query } => {
            let results = sonarr.lookup(&query).await?;
            if json {
                return output::print_json(&results);
            }
            if results.is_empty() {
                println!("no results for \"{query}\"");
                return Ok(());
            }
            let mut t = output::table(&["title", "year", "tvdb", "status", "network", "in library"]);
            for s in &results {
                t.add_row(vec![
                    s.title.clone(),
                    s.year.map(|y| y.to_string()).unwrap_or_default(),
                    s.tvdb_id.to_string(),
                    s.status.clone().unwrap_or_default(),
                    s.network.clone().unwrap_or_default(),
                    if s.id > 0 { "✓".into() } else { String::new() },
                ]);
            }
            println!("{t}");
        }
        SeriesCmd::Add {
            tvdb_id,
            profile,
            root,
            no_search,
            unmonitored,
            no_season_folders,
        } => {
            let results = sonarr.lookup(&format!("tvdb:{tvdb_id}")).await?;
            let series = results
                .iter()
                .find(|s| s.tvdb_id == tvdb_id)
                .ok_or_else(|| CliarrError::Other(format!("no series found for tvdb:{tvdb_id}")))?;
            if series.id > 0 {
                return Err(CliarrError::Other(format!(
                    "\"{}\" is already in the library (id {})",
                    series.title, series.id
                )));
            }

            let profiles = sonarr.quality_profiles().await?;
            let profile_id = resolve_profile(&profiles, profile.as_deref())?;
            let roots = sonarr.root_folders().await?;
            let root_path = resolve_root(&roots, root.as_deref())?;

            let added = sonarr
                .add_series(
                    series,
                    profile_id,
                    &root_path,
                    !unmonitored,
                    !no_season_folders,
                    !no_search,
                )
                .await?;
            if json {
                return output::print_json(&added);
            }
            println!(
                "✓ Added: {} ({}){}",
                added.title,
                added.year.map(|y| y.to_string()).unwrap_or_default(),
                if no_search { "" } else { "; searching for episodes" }
            );
        }
        SeriesCmd::List { filter } => {
            let mut series = sonarr.series().await?;
            series.retain(|s| match filter {
                None => true,
                Some(LibraryFilter::Missing) => {
                    s.monitored.unwrap_or(false)
                        && s.statistics
                            .as_ref()
                            .map(|st| st.percent_of_episodes.unwrap_or(100.0) < 100.0)
                            .unwrap_or(false)
                }
                Some(LibraryFilter::Monitored) => s.monitored.unwrap_or(false),
                Some(LibraryFilter::Unmonitored) => !s.monitored.unwrap_or(false),
            });
            series.sort_by_key(|a| a.title.to_lowercase());
            if json {
                return output::print_json(&series);
            }
            let mut t = output::table(&["id", "title", "year", "monitored", "episodes", "size"]);
            for s in &series {
                let (eps, size) = s
                    .statistics
                    .as_ref()
                    .map(|st| {
                        (
                            format!(
                                "{}/{}",
                                st.episode_file_count.unwrap_or(0),
                                st.episode_count.unwrap_or(0)
                            ),
                            output::fmt_bytes(st.size_on_disk.unwrap_or(0) as f64),
                        )
                    })
                    .unwrap_or_default();
                t.add_row(vec![
                    output::right(s.id),
                    comfy_table::Cell::new(&s.title),
                    comfy_table::Cell::new(s.year.map(|y| y.to_string()).unwrap_or_default()),
                    comfy_table::Cell::new(output::check_mark(s.monitored.unwrap_or(false))),
                    output::right(eps),
                    output::right(size),
                ]);
            }
            println!("{t}");
            println!("{} series", series.len());
        }
        SeriesCmd::Remove {
            id,
            delete_files,
            exclude,
        } => {
            let all = sonarr.series().await?;
            let series = all
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| CliarrError::Other(format!("no series with id {id} in the library")))?;
            let extra = if delete_files { " and delete its files" } else { "" };
            if !confirm(&format!("Remove \"{}\"{extra}?", series.title))? {
                println!("aborted");
                return Ok(());
            }
            sonarr.delete_series(id, delete_files, exclude).await?;
            println!("✓ Removed: {}", series.title);
        }
        SeriesCmd::SearchMissing => {
            sonarr
                .command("MissingEpisodeSearch", serde_json::json!({}))
                .await?;
            println!("✓ Triggered search for all missing episodes");
        }
        SeriesCmd::Refresh { id } => {
            let extra = match id {
                Some(id) => serde_json::json!({ "seriesId": id }),
                None => serde_json::json!({}),
            };
            sonarr.command("RefreshSeries", extra).await?;
            println!("✓ Triggered refresh");
        }
    }
    Ok(())
}

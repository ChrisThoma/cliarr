use crate::api::Clients;
use crate::cli::{LibraryFilter, MovieCmd};
use crate::commands::{choose, confirm, output};
use crate::error::{CliarrError, Result};

pub async fn run(cmd: MovieCmd, clients: &Clients, json: bool) -> Result<()> {
    let radarr = clients.radarr()?;
    match cmd {
        MovieCmd::Search { query } => {
            let results = radarr.lookup(&query).await?;
            if json {
                return output::print_json(&results);
            }
            if results.is_empty() {
                println!("no results for \"{query}\"");
                return Ok(());
            }
            let mut t = output::table(&["title", "year", "tmdb", "status", "in library"]);
            for m in &results {
                t.add_row(vec![
                    m.title.clone(),
                    m.year.map(|y| y.to_string()).unwrap_or_default(),
                    m.tmdb_id.to_string(),
                    m.status.clone().unwrap_or_default(),
                    if m.id > 0 { "✓".into() } else { String::new() },
                ]);
            }
            println!("{t}");
        }
        MovieCmd::Add {
            tmdb_id,
            profile,
            root,
            no_search,
            unmonitored,
        } => {
            let results = radarr.lookup(&format!("tmdb:{tmdb_id}")).await?;
            let movie = results
                .iter()
                .find(|m| m.tmdb_id == tmdb_id)
                .ok_or_else(|| CliarrError::Other(format!("no movie found for tmdb:{tmdb_id}")))?;
            if movie.id > 0 {
                return Err(CliarrError::Other(format!(
                    "\"{}\" is already in the library (id {})",
                    movie.title, movie.id
                )));
            }

            let profiles = radarr.quality_profiles().await?;
            let profile_id = resolve_profile(&profiles, profile.as_deref())?;
            let roots = radarr.root_folders().await?;
            let root_path = resolve_root(&roots, root.as_deref())?;

            let added = radarr
                .add_movie(movie, profile_id, &root_path, !unmonitored, !no_search)
                .await?;
            if json {
                return output::print_json(&added);
            }
            println!(
                "✓ Added: {} ({}){}",
                added.title,
                added.year.map(|y| y.to_string()).unwrap_or_default(),
                if no_search { "" } else { " — searching for release" }
            );
        }
        MovieCmd::List { filter } => {
            let mut movies = radarr.movies().await?;
            movies.retain(|m| match filter {
                None => true,
                Some(LibraryFilter::Missing) => {
                    !m.has_file.unwrap_or(false) && m.monitored.unwrap_or(false)
                }
                Some(LibraryFilter::Monitored) => m.monitored.unwrap_or(false),
                Some(LibraryFilter::Unmonitored) => !m.monitored.unwrap_or(false),
            });
            movies.sort_by_key(|a| a.title.to_lowercase());
            if json {
                return output::print_json(&movies);
            }
            let mut t = output::table(&["id", "title", "year", "monitored", "file", "size"]);
            for m in &movies {
                t.add_row(vec![
                    output::right(m.id),
                    comfy_table::Cell::new(&m.title),
                    comfy_table::Cell::new(m.year.map(|y| y.to_string()).unwrap_or_default()),
                    comfy_table::Cell::new(output::check_mark(m.monitored.unwrap_or(false))),
                    comfy_table::Cell::new(output::check_mark(m.has_file.unwrap_or(false))),
                    output::right(output::fmt_bytes(m.size_on_disk.unwrap_or(0) as f64)),
                ]);
            }
            println!("{t}");
            println!("{} movies", movies.len());
        }
        MovieCmd::Remove {
            id,
            delete_files,
            exclude,
        } => {
            let movies = radarr.movies().await?;
            let movie = movies
                .iter()
                .find(|m| m.id == id)
                .ok_or_else(|| CliarrError::Other(format!("no movie with id {id} in the library")))?;
            let extra = if delete_files { " and delete its files" } else { "" };
            if !confirm(&format!("Remove \"{}\"{extra}?", movie.title))? {
                println!("aborted");
                return Ok(());
            }
            radarr.delete_movie(id, delete_files, exclude).await?;
            println!("✓ Removed: {}", movie.title);
        }
        MovieCmd::SearchMissing => {
            radarr
                .command("MissingMoviesSearch", serde_json::json!({}))
                .await?;
            println!("✓ Triggered search for all missing movies");
        }
        MovieCmd::Refresh { id } => {
            let extra = match id {
                Some(id) => serde_json::json!({ "movieIds": [id] }),
                None => serde_json::json!({}),
            };
            radarr.command("RefreshMovie", extra).await?;
            println!("✓ Triggered refresh");
        }
    }
    Ok(())
}

pub(crate) fn resolve_profile(
    profiles: &[crate::api::models::arr::QualityProfile],
    wanted: Option<&str>,
) -> Result<i64> {
    match wanted {
        Some(w) => profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(w) || p.id.to_string() == w)
            .map(|p| p.id)
            .ok_or_else(|| {
                let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
                CliarrError::Other(format!(
                    "no quality profile \"{w}\" — available: {}",
                    names.join(", ")
                ))
            }),
        None => choose("quality profile", profiles, |p| p.name.clone()).map(|p| p.id),
    }
}

pub(crate) fn resolve_root(
    roots: &[crate::api::models::arr::RootFolder],
    wanted: Option<&str>,
) -> Result<String> {
    match wanted {
        Some(w) => roots
            .iter()
            .find(|r| r.path.trim_end_matches('/') == w.trim_end_matches('/'))
            .map(|r| r.path.clone())
            .ok_or_else(|| {
                let paths: Vec<&str> = roots.iter().map(|r| r.path.as_str()).collect();
                CliarrError::Other(format!(
                    "no root folder \"{w}\" — available: {}",
                    paths.join(", ")
                ))
            }),
        None => choose("root folder", roots, |r| {
            match r.free_space {
                Some(free) => format!("{} ({} free)", r.path, output::fmt_bytes(free as f64)),
                None => r.path.clone(),
            }
        })
        .map(|r| r.path.clone()),
    }
}

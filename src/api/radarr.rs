use chrono::NaiveDate;
use serde_json::json;

use crate::api::arr_core::ArrCore;
use crate::api::models::arr::{Paged, QualityProfile, QueueItem, RootFolder, SystemStatus};
use crate::api::models::radarr::Movie;
use crate::config::ApiKeyService;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct RadarrClient {
    core: ArrCore,
}

impl RadarrClient {
    pub fn new(cfg: &ApiKeyService, http: reqwest::Client) -> Self {
        Self {
            core: ArrCore::new("radarr", &cfg.url, &cfg.api_key, http),
        }
    }

    pub fn core(&self) -> &ArrCore {
        &self.core
    }

    pub async fn system_status(&self) -> Result<SystemStatus> {
        self.core.get_json("/api/v3/system/status", &[]).await
    }

    pub async fn lookup(&self, term: &str) -> Result<Vec<Movie>> {
        self.core
            .get_json("/api/v3/movie/lookup", &[("term", term.to_string())])
            .await
    }

    pub async fn movies(&self) -> Result<Vec<Movie>> {
        self.core.get_json("/api/v3/movie", &[]).await
    }

    pub async fn quality_profiles(&self) -> Result<Vec<QualityProfile>> {
        self.core.get_json("/api/v3/qualityprofile", &[]).await
    }

    pub async fn root_folders(&self) -> Result<Vec<RootFolder>> {
        self.core.get_json("/api/v3/rootfolder", &[]).await
    }

    /// Add a movie from a lookup result. Radarr wants the lookup object back
    /// with library fields (profile, root, monitor options) filled in.
    pub async fn add_movie(
        &self,
        movie: &Movie,
        quality_profile_id: i64,
        root_folder: &str,
        monitored: bool,
        search_now: bool,
    ) -> Result<Movie> {
        let body = json!({
            "title": movie.title,
            "tmdbId": movie.tmdb_id,
            "year": movie.year,
            "qualityProfileId": quality_profile_id,
            "rootFolderPath": root_folder,
            "monitored": monitored,
            "images": movie.images,
            "addOptions": { "searchForMovie": search_now },
        });
        self.core.post_json("/api/v3/movie", &body).await
    }

    /// Update library fields via the bulk editor endpoint: it accepts partial
    /// updates, so we never round-trip (and truncate) the full movie object.
    pub async fn edit_movie(&self, id: i64, quality_profile_id: i64, monitored: bool) -> Result<()> {
        let body = json!({
            "movieIds": [id],
            "qualityProfileId": quality_profile_id,
            "monitored": monitored,
        });
        self.core.put("/api/v3/movie/editor", &body).await
    }

    pub async fn delete_movie(&self, id: i64, delete_files: bool, add_exclusion: bool) -> Result<()> {
        self.core
            .delete(
                &format!("/api/v3/movie/{id}"),
                &[
                    ("deleteFiles", delete_files.to_string()),
                    ("addImportExclusion", add_exclusion.to_string()),
                ],
            )
            .await
    }

    pub async fn queue(&self) -> Result<Vec<QueueItem>> {
        let paged: Paged<QueueItem> = self
            .core
            .get_json(
                "/api/v3/queue",
                &[
                    ("pageSize", "200".into()),
                    ("includeUnknownMovieItems", "true".into()),
                ],
            )
            .await?;
        Ok(paged.records)
    }

    pub async fn queue_delete(&self, id: i64, blocklist: bool, remove_from_client: bool) -> Result<()> {
        self.core
            .delete(
                &format!("/api/v3/queue/{id}"),
                &[
                    ("blocklist", blocklist.to_string()),
                    ("removeFromClient", remove_from_client.to_string()),
                ],
            )
            .await
    }

    pub async fn calendar(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<Movie>> {
        self.core
            .get_json(
                "/api/v3/calendar",
                &[
                    ("start", start.to_string()),
                    ("end", end.to_string()),
                    ("unmonitored", "false".into()),
                ],
            )
            .await
    }

    pub async fn missing(&self) -> Result<Paged<Movie>> {
        self.core
            .get_json(
                "/api/v3/wanted/missing",
                &[
                    ("pageSize", "100".into()),
                    ("sortKey", "physicalRelease".into()),
                    ("sortDirection", "descending".into()),
                    ("monitored", "true".into()),
                ],
            )
            .await
    }

    /// Fire a Radarr command, e.g. MissingMoviesSearch, RefreshMovie, MoviesSearch.
    pub async fn command(&self, name: &str, extra: serde_json::Value) -> Result<()> {
        let mut body = json!({ "name": name });
        if let (Some(obj), Some(extra_obj)) = (body.as_object_mut(), extra.as_object()) {
            obj.extend(extra_obj.clone());
        }
        self.core.post("/api/v3/command", &body).await
    }
}

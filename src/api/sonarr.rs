use chrono::NaiveDate;
use serde_json::json;

use crate::api::arr_core::ArrCore;
use crate::api::models::arr::{Paged, QueueItem};
use crate::api::models::sonarr::{Episode, Series};
use crate::config::ApiKeyService;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct SonarrClient {
    core: ArrCore,
}

/// Endpoints identical across Radarr/Sonarr (system status, profiles, root
/// folders, queue delete, commands) live on ArrCore and are reached through
/// this Deref; only series-specific calls are defined here.
impl std::ops::Deref for SonarrClient {
    type Target = ArrCore;

    fn deref(&self) -> &ArrCore {
        &self.core
    }
}

impl SonarrClient {
    pub fn new(cfg: &ApiKeyService, http: reqwest::Client) -> Self {
        Self {
            core: ArrCore::new("sonarr", &cfg.url, &cfg.api_key, http),
        }
    }

    pub fn core(&self) -> &ArrCore {
        &self.core
    }

    pub async fn lookup(&self, term: &str) -> Result<Vec<Series>> {
        self.core
            .get_json("/api/v3/series/lookup", &[("term", term.to_string())])
            .await
    }

    pub async fn series(&self) -> Result<Vec<Series>> {
        self.core.get_json("/api/v3/series", &[]).await
    }

    pub async fn add_series(
        &self,
        series: &Series,
        quality_profile_id: i64,
        root_folder: &str,
        monitored: bool,
        season_folder: bool,
        search_now: bool,
    ) -> Result<Series> {
        let body = json!({
            "title": series.title,
            "tvdbId": series.tvdb_id,
            "qualityProfileId": quality_profile_id,
            "rootFolderPath": root_folder,
            "monitored": monitored,
            "seasonFolder": season_folder,
            "seasons": series.seasons,
            "images": series.images,
            "addOptions": {
                "searchForMissingEpisodes": search_now,
                "monitor": if monitored { "all" } else { "none" },
            },
        });
        self.core.post_json("/api/v3/series", &body).await
    }

    /// Update library fields via the bulk editor endpoint: it accepts partial
    /// updates, so we never round-trip (and truncate) the full series object.
    pub async fn edit_series(&self, id: i64, quality_profile_id: i64, monitored: bool) -> Result<()> {
        let body = json!({
            "seriesIds": [id],
            "qualityProfileId": quality_profile_id,
            "monitored": monitored,
        });
        self.core.put("/api/v3/series/editor", &body).await
    }

    pub async fn delete_series(&self, id: i64, delete_files: bool, add_exclusion: bool) -> Result<()> {
        self.core
            .delete(
                &format!("/api/v3/series/{id}"),
                &[
                    ("deleteFiles", delete_files.to_string()),
                    ("addImportListExclusion", add_exclusion.to_string()),
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
                    ("includeUnknownSeriesItems", "true".into()),
                ],
            )
            .await?;
        Ok(paged.records)
    }

    pub async fn calendar(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<Episode>> {
        self.core
            .get_json(
                "/api/v3/calendar",
                &[
                    ("start", start.to_string()),
                    ("end", end.to_string()),
                    ("unmonitored", "false".into()),
                    ("includeSeries", "true".into()),
                ],
            )
            .await
    }

    pub async fn missing(&self) -> Result<Paged<Episode>> {
        self.core
            .get_json(
                "/api/v3/wanted/missing",
                &[
                    ("pageSize", "100".into()),
                    ("sortKey", "airDateUtc".into()),
                    ("sortDirection", "descending".into()),
                    ("includeSeries", "true".into()),
                    ("monitored", "true".into()),
                ],
            )
            .await
    }
}

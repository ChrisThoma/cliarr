use chrono::NaiveDate;
use serde_json::json;

use crate::api::arr_core::ArrCore;
use crate::api::models::arr::{Paged, QualityProfile, QueueItem, RootFolder, SystemStatus};
use crate::api::models::sonarr::{Episode, Series};
use crate::config::ApiKeyService;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct SonarrClient {
    core: ArrCore,
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

    pub async fn system_status(&self) -> Result<SystemStatus> {
        self.core.get_json("/api/v3/system/status", &[]).await
    }

    pub async fn lookup(&self, term: &str) -> Result<Vec<Series>> {
        self.core
            .get_json("/api/v3/series/lookup", &[("term", term.to_string())])
            .await
    }

    pub async fn series(&self) -> Result<Vec<Series>> {
        self.core.get_json("/api/v3/series", &[]).await
    }

    pub async fn quality_profiles(&self) -> Result<Vec<QualityProfile>> {
        self.core.get_json("/api/v3/qualityprofile", &[]).await
    }

    pub async fn root_folders(&self) -> Result<Vec<RootFolder>> {
        self.core.get_json("/api/v3/rootfolder", &[]).await
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

    /// Fire a Sonarr command, e.g. MissingEpisodeSearch, RefreshSeries, SeriesSearch.
    pub async fn command(&self, name: &str, extra: serde_json::Value) -> Result<()> {
        let mut body = json!({ "name": name });
        if let (Some(obj), Some(extra_obj)) = (body.as_object_mut(), extra.as_object()) {
            obj.extend(extra_obj.clone());
        }
        self.core.post("/api/v3/command", &body).await
    }
}

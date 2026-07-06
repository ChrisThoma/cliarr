//! Models shared between Radarr and Sonarr (their v3 APIs mirror each other).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    #[serde(default)]
    pub app_name: Option<String>,
    pub version: String,
    #[serde(default)]
    pub instance_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub cover_type: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub remote_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QualityProfile {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootFolder {
    pub id: i64,
    pub path: String,
    #[serde(default)]
    pub free_space: Option<i64>,
    #[serde(default)]
    pub accessible: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paged<T> {
    pub page: i64,
    pub page_size: i64,
    pub total_records: i64,
    pub records: Vec<T>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub timeleft: Option<String>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default)]
    pub sizeleft: Option<f64>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub download_client: Option<String>,
    #[serde(default)]
    pub tracked_download_state: Option<String>,
    #[serde(default)]
    pub tracked_download_status: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub movie_id: Option<i64>,
    #[serde(default)]
    pub series_id: Option<i64>,
}

impl QueueItem {
    pub fn progress(&self) -> Option<f64> {
        match (self.size, self.sizeleft) {
            (Some(size), Some(left)) if size > 0.0 => Some(((size - left) / size) * 100.0),
            _ => None,
        }
    }
}

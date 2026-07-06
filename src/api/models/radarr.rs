use serde::{Deserialize, Serialize};

use super::arr::Image;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Movie {
    /// 0 for lookup results not yet in the library.
    #[serde(default)]
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub year: Option<i32>,
    pub tmdb_id: i64,
    #[serde(default)]
    pub imdb_id: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub monitored: Option<bool>,
    #[serde(default)]
    pub has_file: Option<bool>,
    #[serde(default)]
    pub is_available: Option<bool>,
    #[serde(default)]
    pub runtime: Option<i64>,
    #[serde(default)]
    pub quality_profile_id: Option<i64>,
    #[serde(default)]
    pub size_on_disk: Option<i64>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub images: Vec<Image>,
    #[serde(default)]
    pub in_cinemas: Option<String>,
    #[serde(default)]
    pub physical_release: Option<String>,
    #[serde(default)]
    pub digital_release: Option<String>,
}

impl Movie {
    pub fn poster_remote_url(&self) -> Option<&str> {
        self.images
            .iter()
            .find(|i| i.cover_type == "poster")
            .and_then(|i| i.remote_url.as_deref().or(i.url.as_deref()))
    }
}

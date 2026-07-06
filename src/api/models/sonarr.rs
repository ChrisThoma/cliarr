use serde::{Deserialize, Serialize};

use super::arr::Image;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    /// 0 for lookup results not yet in the library.
    #[serde(default)]
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub tvdb_id: i64,
    #[serde(default)]
    pub imdb_id: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub monitored: Option<bool>,
    #[serde(default)]
    pub season_folder: Option<bool>,
    #[serde(default)]
    pub quality_profile_id: Option<i64>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub images: Vec<Image>,
    #[serde(default)]
    pub statistics: Option<SeriesStatistics>,
    #[serde(default)]
    pub seasons: Vec<Season>,
}

impl Series {
    pub fn poster_remote_url(&self) -> Option<&str> {
        self.images
            .iter()
            .find(|i| i.cover_type == "poster")
            .and_then(|i| i.remote_url.as_deref().or(i.url.as_deref()))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Season {
    pub season_number: i32,
    pub monitored: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesStatistics {
    #[serde(default)]
    pub episode_count: Option<i64>,
    #[serde(default)]
    pub episode_file_count: Option<i64>,
    #[serde(default)]
    pub size_on_disk: Option<i64>,
    #[serde(default)]
    pub percent_of_episodes: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: i64,
    #[serde(default)]
    pub series_id: i64,
    pub season_number: i32,
    pub episode_number: i32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub air_date_utc: Option<String>,
    #[serde(default)]
    pub has_file: Option<bool>,
    #[serde(default)]
    pub monitored: Option<bool>,
    #[serde(default)]
    pub series: Option<Series>,
}

impl Episode {
    pub fn code(&self) -> String {
        format!("S{:02}E{:02}", self.season_number, self.episode_number)
    }

    pub fn series_title(&self) -> &str {
        self.series.as_ref().map(|s| s.title.as_str()).unwrap_or("?")
    }
}

use serde::{Deserialize, Serialize};

/// Plex wraps everything in `MediaContainer`.
#[derive(Debug, Clone, Deserialize)]
pub struct Wrapped<T> {
    #[serde(rename = "MediaContainer")]
    pub media_container: T,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    #[serde(default)]
    pub friendly_name: Option<String>,
    pub version: String,
    #[serde(default)]
    pub machine_identifier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SectionList {
    #[serde(rename = "Directory", default)]
    pub directories: Vec<Section>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Section {
    pub key: String,
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionList {
    #[serde(default)]
    pub size: i64,
    #[serde(rename = "Metadata", default)]
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    #[serde(default)]
    pub title: Option<String>,
    /// For episodes: the show title.
    #[serde(default)]
    pub grandparent_title: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(rename = "User", default)]
    pub user: Option<PlexUser>,
    #[serde(rename = "Player", default)]
    pub player: Option<PlexPlayer>,
    #[serde(default)]
    pub view_offset: Option<i64>,
    #[serde(default)]
    pub duration: Option<i64>,
}

impl Session {
    pub fn display_title(&self) -> String {
        match (&self.grandparent_title, &self.title) {
            (Some(show), Some(ep)) => format!("{show} — {ep}"),
            (_, Some(t)) => t.clone(),
            _ => "?".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlexUser {
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlexPlayer {
    #[serde(default)]
    pub product: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Torrent {
    pub hash: String,
    pub name: String,
    pub state: String,
    /// 0.0–1.0
    pub progress: f64,
    /// bytes
    pub size: i64,
    /// bytes/s
    pub dlspeed: i64,
    pub upspeed: i64,
    /// seconds; 8640000 means "infinite" in qBittorrent
    pub eta: i64,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub num_seeds: Option<i64>,
    #[serde(default)]
    pub ratio: Option<f64>,
}

impl Torrent {
    pub fn is_paused(&self) -> bool {
        // 4.x uses pausedDL/pausedUP; 5.x renamed them to stoppedDL/stoppedUP
        self.state.starts_with("paused") || self.state.starts_with("stopped")
    }
}

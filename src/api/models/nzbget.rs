use serde::{Deserialize, Serialize};

/// One entry from NZBGet's `listgroups` RPC.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NzbGroup {
    #[serde(rename = "NZBID")]
    pub id: i64,
    #[serde(rename = "NZBName")]
    pub name: String,
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "FileSizeMB")]
    pub file_size_mb: i64,
    #[serde(rename = "RemainingSizeMB")]
    pub remaining_size_mb: i64,
    #[serde(rename = "DownloadedSizeMB", default)]
    pub downloaded_size_mb: i64,
    #[serde(rename = "Category", default)]
    pub category: String,
}

impl NzbGroup {
    pub fn progress(&self) -> f64 {
        if self.file_size_mb <= 0 {
            return 0.0;
        }
        ((self.file_size_mb - self.remaining_size_mb) as f64 / self.file_size_mb as f64) * 100.0
    }
}

/// Subset of NZBGet's `status` RPC result.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NzbStatus {
    #[serde(rename = "DownloadRate")]
    pub download_rate: i64,
    #[serde(rename = "RemainingSizeMB")]
    pub remaining_size_mb: i64,
    #[serde(rename = "DownloadPaused")]
    pub download_paused: bool,
}

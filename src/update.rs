//! Self-update against GitHub Releases, plus the passive daily update check.
//!
//! The passive check must never break or delay a normal command: every
//! failure path (network, parse, unwritable cache) is silently dropped, and
//! the network refresh is bounded by a short timeout.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::http;
use crate::error::{CliarrError, Result};

pub const GITHUB_API: &str = "https://api.github.com";
pub const REPO: &str = "ChrisThoma/cliarr";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Target triple this binary was built for (emitted by build.rs).
pub const TARGET: &str = env!("CLIARR_TARGET");
pub const CHECKSUMS_ASSET: &str = "SHA256SUMS";

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const PASSIVE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }

    /// Tag without the leading `v`.
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches('v')
    }
}

/// Release asset name for a target triple, as produced by release.yml.
pub fn asset_name(target: &str) -> String {
    if target.contains("windows") {
        format!("cliarr-{target}.exe")
    } else {
        format!("cliarr-{target}")
    }
}

pub async fn fetch_latest(client: &reqwest::Client, api_base: &str) -> Result<Release> {
    let url = format!("{}/repos/{REPO}/releases/latest", api_base.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CliarrError::Other(format!(
            "no releases published yet at https://github.com/{REPO}/releases"
        )));
    }
    let resp = http::check("GitHub", resp).await?;
    Ok(resp.json().await?)
}

/// The latest version if `tag` (with or without a leading `v`) is newer than
/// `current`; None when up to date or either side is unparseable.
pub fn is_newer(current: &str, tag: &str) -> Option<semver::Version> {
    let current = semver::Version::parse(current).ok()?;
    let latest = semver::Version::parse(tag.trim_start_matches('v')).ok()?;
    (latest > current).then_some(latest)
}

/// Expected hex digest for `name` from a `sha256sum`-format checksums file
/// (`<hex>  <name>`, optional `*` binary-mode marker before the name).
pub fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name).then(|| hash.to_ascii_lowercase())
    })
}

pub fn verify_checksum(bytes: &[u8], expected_hex: &str) -> Result<()> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected_hex.to_ascii_lowercase() {
        return Err(CliarrError::Other(format!(
            "checksum mismatch for downloaded binary (expected {expected_hex}, got {actual}); aborting update"
        )));
    }
    Ok(())
}

// ---- passive daily check ----------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckCache {
    /// Unix timestamp of the last completed check attempt.
    pub checked_at: u64,
    /// Latest known release version (no `v` prefix).
    pub latest: String,
}

impl CheckCache {
    pub fn is_stale(&self, now: u64) -> bool {
        now.saturating_sub(self.checked_at) > CHECK_INTERVAL.as_secs()
    }
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn cache_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "cliarr")
        .map(|d| d.cache_dir().join("update-check.json"))
}

pub fn read_cache() -> Option<CheckCache> {
    let bytes = std::fs::read(cache_path()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Record a completed check. Fail-silent: an unwritable cache dir only means
/// the next command re-checks.
pub fn write_cache(latest: &str) {
    let Some(path) = cache_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let cache = CheckCache { checked_at: now_unix(), latest: latest.to_string() };
    if let Ok(json) = serde_json::to_vec(&cache) {
        let _ = std::fs::write(path, json);
    }
}

fn checks_disabled() -> bool {
    std::env::var_os("CLIARR_NO_UPDATE_CHECK").is_some()
}

/// Query GitHub and rewrite the cache. On failure the cache timestamp is
/// still bumped (keeping the previous known version) so an offline machine or
/// a repo with no releases doesn't pay the network wait on every command.
async fn refresh() {
    let client = http::build_client();
    let latest = match fetch_latest(&client, GITHUB_API).await {
        Ok(release) => release.version().to_string(),
        // No releases exist (any more): don't keep advertising a cached one.
        Err(CliarrError::Other(_)) => CURRENT_VERSION.to_string(),
        // Transient failure: keep the previous answer, just bump the timestamp.
        Err(_) => read_cache().map(|c| c.latest).unwrap_or_else(|| CURRENT_VERSION.to_string()),
    };
    write_cache(&latest);
}

/// Refresh the cache if it's older than a day. No output; used by the TUI,
/// which prints the notice itself after the terminal is restored.
pub async fn passive_refresh() {
    if checks_disabled() {
        return;
    }
    if read_cache().is_none_or(|c| c.is_stale(now_unix())) {
        refresh().await;
    }
}

/// One-line stderr notice when the cached latest version is newer. Quiet when
/// stderr isn't a TTY or checks are disabled.
pub fn print_notice_if_cached_newer() {
    if checks_disabled() || !std::io::stderr().is_terminal() {
        return;
    }
    if let Some(cache) = read_cache()
        && let Some(latest) = is_newer(CURRENT_VERSION, &cache.latest)
    {
        eprintln!("cliarr v{latest} is available (you have v{CURRENT_VERSION}); run `cliarr update`");
    }
}

/// Daily check for CLI commands: refresh the cache if stale (bounded by a
/// short timeout, after command output has already been printed), then print
/// the notice. Never fails, never delays more than [`PASSIVE_TIMEOUT`].
pub async fn passive_check_and_notify() {
    if checks_disabled() || !std::io::stderr().is_terminal() {
        return;
    }
    if read_cache().is_none_or(|c| c.is_stale(now_unix())) {
        let _ = tokio::time::timeout(PASSIVE_TIMEOUT, refresh()).await;
    }
    print_notice_if_cached_newer();
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn is_newer_compares_semver() {
        assert_eq!(is_newer("0.1.0", "v0.2.0").unwrap().to_string(), "0.2.0");
        assert_eq!(is_newer("0.1.0", "0.1.1").unwrap().to_string(), "0.1.1");
        assert!(is_newer("0.2.0", "v0.2.0").is_none(), "equal is not newer");
        assert!(is_newer("0.2.0", "v0.1.9").is_none(), "older is not newer");
        // Releases beat their own prereleases.
        assert!(is_newer("0.2.0-rc.1", "v0.2.0").is_some());
        assert!(is_newer("0.2.0", "v0.2.0-rc.1").is_none());
        assert!(is_newer("0.1.0", "nightly").is_none(), "malformed tag is ignored");
        assert!(is_newer("not-a-version", "v9.9.9").is_none());
    }

    #[test]
    fn asset_names_follow_the_release_convention() {
        assert_eq!(asset_name("aarch64-apple-darwin"), "cliarr-aarch64-apple-darwin");
        assert_eq!(
            asset_name("x86_64-pc-windows-msvc"),
            "cliarr-x86_64-pc-windows-msvc.exe"
        );
    }

    #[test]
    fn cache_staleness_boundary_is_24h() {
        let cache = CheckCache { checked_at: 1_000_000, latest: "0.1.0".into() };
        assert!(!cache.is_stale(1_000_000 + 24 * 3600));
        assert!(cache.is_stale(1_000_000 + 24 * 3600 + 1));
        // Clock skew (checked_at in the future) must not read as stale.
        assert!(!cache.is_stale(999_000));
    }

    #[test]
    fn checksums_parse_and_verify() {
        let digest = format!("{:x}", Sha256::digest(b"binary contents"));
        let sums = format!(
            "{digest}  cliarr-aarch64-apple-darwin\naabbcc  *cliarr-x86_64-pc-windows-msvc.exe\n"
        );
        assert_eq!(checksum_for(&sums, "cliarr-aarch64-apple-darwin").as_deref(), Some(digest.as_str()));
        assert_eq!(
            checksum_for(&sums, "cliarr-x86_64-pc-windows-msvc.exe").as_deref(),
            Some("aabbcc"),
            "binary-mode marker is stripped"
        );
        assert!(checksum_for(&sums, "cliarr-x86_64-unknown-linux-gnu").is_none());

        assert!(verify_checksum(b"binary contents", &digest).is_ok());
        assert!(verify_checksum(b"tampered contents", &digest).is_err());
    }

    #[tokio::test]
    async fn fetch_latest_parses_a_release() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/repos/{REPO}/releases/latest")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v0.2.0",
                "assets": [
                    {"name": "cliarr-aarch64-apple-darwin",
                     "browser_download_url": "https://example.invalid/cliarr-aarch64-apple-darwin"},
                    {"name": "SHA256SUMS",
                     "browser_download_url": "https://example.invalid/SHA256SUMS"}
                ]
            })))
            .mount(&server)
            .await;

        let release = fetch_latest(&http::build_client(), &server.uri()).await.unwrap();
        assert_eq!(release.tag_name, "v0.2.0");
        assert_eq!(release.version(), "0.2.0");
        assert!(release.asset("cliarr-aarch64-apple-darwin").is_some());
        assert!(release.asset(CHECKSUMS_ASSET).is_some());
        assert!(release.asset("cliarr-mystery-triple").is_none());
    }

    #[tokio::test]
    async fn fetch_latest_maps_404_to_no_releases() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let err = fetch_latest(&http::build_client(), &server.uri()).await.unwrap_err();
        assert!(err.to_string().contains("no releases"), "got: {err}");
    }
}

use std::time::Duration;

use url::Url;

use crate::error::{CliarrError, Result};

/// Shared HTTP client: sane timeout, cookie store (needed for qBittorrent
/// session auth), identifiable user-agent.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .cookie_store(true)
        .user_agent(concat!("cliarr/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("failed to build HTTP client")
}

/// Join `path` onto `base`, preserving any path prefix in the base URL
/// (Synology reverse proxies often serve services under e.g. `/radarr`).
pub fn join_url(base: &str, path: &str) -> Result<Url> {
    let mut b = Url::parse(base.trim_end_matches('/'))?;
    let joined = format!("{}/{}", b.path().trim_end_matches('/'), path.trim_start_matches('/'));
    b.set_path(&joined);
    Ok(b)
}

/// Map non-2xx responses to typed errors. 401/403 become `Auth`.
pub async fn check(service: &'static str, resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(CliarrError::Auth { service });
    }
    let body = resp.text().await.unwrap_or_default();
    let body = if body.chars().count() > 300 {
        format!("{}…", body.chars().take(300).collect::<String>())
    } else {
        body
    };
    Err(CliarrError::Api {
        service,
        status: status.as_u16(),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_preserves_base_path_prefix() {
        let u = join_url("http://nas.local:7878", "/api/v3/movie").unwrap();
        assert_eq!(u.as_str(), "http://nas.local:7878/api/v3/movie");

        let u = join_url("http://nas.local/radarr/", "api/v3/movie").unwrap();
        assert_eq!(u.as_str(), "http://nas.local/radarr/api/v3/movie");
    }
}

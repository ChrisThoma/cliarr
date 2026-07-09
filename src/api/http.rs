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
    // Read the body even on 401/403: the server's own message is what tells a
    // bad credential apart from e.g. qBittorrent's Host-header-validation
    // rejection or an IP ban, all of which arrive as 401/403.
    let body = resp.text().await.unwrap_or_default();
    Err(response_error(service, status, &body))
}

/// Pure status+body → error mapping (no I/O), so the auth-vs-api distinction
/// and body passthrough are unit-testable without a live server.
fn response_error(service: &'static str, status: reqwest::StatusCode, body: &str) -> CliarrError {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return CliarrError::Auth {
            service,
            detail: auth_detail(body),
        };
    }
    CliarrError::Api {
        service,
        status: status.as_u16(),
        body: truncate(body.trim()),
    }
}

/// Human-facing detail for an auth failure: the server's own response body when
/// it said something, else a generic hint. An empty `()` would be useless.
fn auth_detail(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "check the API key/credentials in your config".to_string()
    } else {
        truncate(trimmed)
    }
}

fn truncate(s: &str) -> String {
    if s.chars().count() > 300 {
        format!("{}…", s.chars().take(300).collect::<String>())
    } else {
        s.to_string()
    }
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

    use reqwest::StatusCode;

    // A 401 from qBittorrent Host-header validation carries the body
    // "Unauthorized", which must reach the user — it's what distinguishes a
    // security-gate rejection from an actual bad password. Regression guard for
    // the bug where 401/403 collapsed to a generic "check your credentials".
    #[test]
    fn auth_error_surfaces_401_body() {
        let e = response_error("qbittorrent", StatusCode::UNAUTHORIZED, "Unauthorized");
        let msg = e.to_string();
        assert!(matches!(e, CliarrError::Auth { .. }), "should classify as Auth");
        assert!(msg.contains("Unauthorized"), "server body must survive; got: {msg}");
    }

    // A 403 IP-ban message must likewise pass through verbatim.
    #[test]
    fn auth_error_surfaces_403_ban_body() {
        let body = "Your IP address has been banned after too many failed authentication attempts.";
        let e = response_error("qbittorrent", StatusCode::FORBIDDEN, body);
        assert!(e.to_string().contains("banned"), "ban reason must survive; got: {e}");
    }

    // An empty auth body falls back to the actionable hint rather than "()".
    #[test]
    fn auth_error_empty_body_falls_back_to_hint() {
        let e = response_error("radarr", StatusCode::UNAUTHORIZED, "");
        let msg = e.to_string();
        assert!(msg.contains("check the API key/credentials"), "got: {msg}");
        assert!(!msg.contains("()"), "must not render an empty detail; got: {msg}");
    }

    // Non-auth failures stay Api errors and keep their (truncated) body.
    #[test]
    fn non_auth_status_is_api_error_with_body() {
        let e = response_error("sonarr", StatusCode::INTERNAL_SERVER_ERROR, "boom");
        match e {
            CliarrError::Api { status, body, .. } => {
                assert_eq!(status, 500);
                assert_eq!(body, "boom");
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[test]
    fn long_bodies_are_truncated() {
        let long = "x".repeat(500);
        let out = truncate(&long);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= 301);
    }
}

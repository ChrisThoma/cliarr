use tokio::sync::Mutex;

use crate::api::http::{check, join_url};
use crate::api::models::qbit::Torrent;
use crate::config::UserPassService;
use crate::error::{CliarrError, Result};

const SERVICE: &str = "qbittorrent";

/// qBittorrent WebUI API v2 client. Auth is a session cookie from
/// /api/v2/auth/login (captured by the shared cookie store); any 403 triggers
/// one re-login and retry. qBittorrent 5.x renamed pause/resume to
/// stop/start, so we probe the WebAPI version once and pick endpoints.
#[derive(Debug)]
pub struct QbitClient {
    base: String,
    username: String,
    password: String,
    http: reqwest::Client,
    /// None = not probed yet; Some(true) = 5.x-style stop/start endpoints.
    uses_stop_start: Mutex<Option<bool>>,
}

impl QbitClient {
    pub fn new(cfg: &UserPassService, http: reqwest::Client) -> Self {
        Self {
            base: cfg.url.clone(),
            username: cfg.username.clone(),
            password: cfg.password.clone(),
            http,
            uses_stop_start: Mutex::new(None),
        }
    }

    pub async fn login(&self) -> Result<()> {
        let url = join_url(&self.base, "/api/v2/auth/login")?;
        let resp = self
            .http
            .post(url)
            .form(&[("username", self.username.as_str()), ("password", self.password.as_str())])
            .send()
            .await?;
        let resp = check(SERVICE, resp).await?;
        // qBittorrent returns 200 with body "Fails." on bad credentials.
        let body = resp.text().await.unwrap_or_default();
        if body.trim() != "Ok." {
            return Err(CliarrError::Auth { service: SERVICE });
        }
        Ok(())
    }

    /// GET that logs in and retries once on 403 (expired/missing session).
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<reqwest::Response> {
        let url = join_url(&self.base, path)?;
        let resp = self.http.get(url.clone()).query(query).send().await?;
        if resp.status() == reqwest::StatusCode::FORBIDDEN {
            self.login().await?;
            let resp = self.http.get(url).query(query).send().await?;
            return check(SERVICE, resp).await;
        }
        check(SERVICE, resp).await
    }

    async fn post_form(&self, path: &str, form: &[(&str, String)]) -> Result<()> {
        let url = join_url(&self.base, path)?;
        let resp = self.http.post(url.clone()).form(form).send().await?;
        if resp.status() == reqwest::StatusCode::FORBIDDEN {
            self.login().await?;
            let resp = self.http.post(url).form(form).send().await?;
            check(SERVICE, resp).await?;
            return Ok(());
        }
        check(SERVICE, resp).await?;
        Ok(())
    }

    pub async fn webapi_version(&self) -> Result<String> {
        Ok(self.get("/api/v2/app/webapiVersion", &[]).await?.text().await?)
    }

    pub async fn app_version(&self) -> Result<String> {
        Ok(self.get("/api/v2/app/version", &[]).await?.text().await?)
    }

    /// Health probe (explicit login, then version) shared by `config test`
    /// and the TUI dashboard; the login surfaces bad credentials directly
    /// instead of as a 403 retry.
    pub async fn health_version(&self) -> Result<String> {
        self.login().await?;
        Ok(self.app_version().await?.trim().to_string())
    }

    async fn stop_start_endpoints(&self) -> Result<bool> {
        let mut cached = self.uses_stop_start.lock().await;
        if let Some(v) = *cached {
            return Ok(v);
        }
        // WebAPI >= 2.11 (qBittorrent 5.0) uses stop/start.
        let ver = self.webapi_version().await?;
        let mut parts = ver.trim().split('.');
        let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(2);
        let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let v = major > 2 || (major == 2 && minor >= 11);
        *cached = Some(v);
        Ok(v)
    }

    pub async fn torrents(&self, filter: Option<&str>) -> Result<Vec<Torrent>> {
        let mut query = vec![("sort", "added_on".to_string()), ("reverse", "true".to_string())];
        if let Some(f) = filter {
            query.push(("filter", f.to_string()));
        }
        Ok(self.get("/api/v2/torrents/info", &query).await?.json().await?)
    }

    pub async fn pause(&self, hashes: &[String]) -> Result<()> {
        let path = if self.stop_start_endpoints().await? {
            "/api/v2/torrents/stop"
        } else {
            "/api/v2/torrents/pause"
        };
        self.post_form(path, &[("hashes", hashes.join("|"))]).await
    }

    pub async fn resume(&self, hashes: &[String]) -> Result<()> {
        let path = if self.stop_start_endpoints().await? {
            "/api/v2/torrents/start"
        } else {
            "/api/v2/torrents/resume"
        };
        self.post_form(path, &[("hashes", hashes.join("|"))]).await
    }

    pub async fn delete(&self, hashes: &[String], delete_files: bool) -> Result<()> {
        self.post_form(
            "/api/v2/torrents/delete",
            &[
                ("hashes", hashes.join("|")),
                ("deleteFiles", delete_files.to_string()),
            ],
        )
        .await
    }
}

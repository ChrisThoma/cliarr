use serde::de::DeserializeOwned;

use crate::api::http::{check, join_url};
use crate::api::models::plex::{Identity, SectionList, SessionList, Wrapped};
use crate::config::TokenService;
use crate::error::Result;

const SERVICE: &str = "plex";

/// Read-only Plex client. JSON via Accept header; X-Plex-Token auth.
#[derive(Debug, Clone)]
pub struct PlexClient {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl PlexClient {
    pub fn new(cfg: &TokenService, http: reqwest::Client) -> Self {
        Self {
            base: cfg.url.clone(),
            token: cfg.token.clone(),
            http,
        }
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = join_url(&self.base, path)?;
        let resp = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .header("X-Plex-Token", &self.token)
            .send()
            .await?;
        Ok(check(SERVICE, resp).await?.json().await?)
    }

    pub async fn identity(&self) -> Result<Identity> {
        let w: Wrapped<Identity> = self.get_json("/identity").await?;
        Ok(w.media_container)
    }

    pub async fn sections(&self) -> Result<SectionList> {
        let w: Wrapped<SectionList> = self.get_json("/library/sections").await?;
        Ok(w.media_container)
    }

    pub async fn sessions(&self) -> Result<SessionList> {
        let w: Wrapped<SessionList> = self.get_json("/status/sessions").await?;
        Ok(w.media_container)
    }
}

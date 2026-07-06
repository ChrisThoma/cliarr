//! Shared request plumbing for Radarr/Sonarr (identical v3 API conventions).

use serde::de::DeserializeOwned;

use crate::api::http::{check, join_url};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct ArrCore {
    pub service: &'static str,
    base: String,
    key: String,
    http: reqwest::Client,
}

impl ArrCore {
    pub fn new(service: &'static str, url: &str, api_key: &str, http: reqwest::Client) -> Self {
        Self {
            service,
            base: url.to_string(),
            key: api_key.to_string(),
            http,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn api_key(&self) -> &str {
        &self.key
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let url = join_url(&self.base, path)?;
        let resp = self
            .http
            .get(url)
            .header("X-Api-Key", &self.key)
            .query(query)
            .send()
            .await?;
        Ok(check(self.service, resp).await?.json().await?)
    }

    pub async fn post_json<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        let url = join_url(&self.base, path)?;
        let resp = self
            .http
            .post(url)
            .header("X-Api-Key", &self.key)
            .json(body)
            .send()
            .await?;
        Ok(check(self.service, resp).await?.json().await?)
    }

    /// POST where we don't care about the response body.
    pub async fn post(&self, path: &str, body: &serde_json::Value) -> Result<()> {
        let url = join_url(&self.base, path)?;
        let resp = self
            .http
            .post(url)
            .header("X-Api-Key", &self.key)
            .json(body)
            .send()
            .await?;
        check(self.service, resp).await?;
        Ok(())
    }

    /// PUT where we don't care about the response body.
    pub async fn put(&self, path: &str, body: &serde_json::Value) -> Result<()> {
        let url = join_url(&self.base, path)?;
        let resp = self
            .http
            .put(url)
            .header("X-Api-Key", &self.key)
            .json(body)
            .send()
            .await?;
        check(self.service, resp).await?;
        Ok(())
    }

    pub async fn delete(&self, path: &str, query: &[(&str, String)]) -> Result<()> {
        let url = join_url(&self.base, path)?;
        let resp = self
            .http
            .delete(url)
            .header("X-Api-Key", &self.key)
            .query(query)
            .send()
            .await?;
        check(self.service, resp).await?;
        Ok(())
    }
}

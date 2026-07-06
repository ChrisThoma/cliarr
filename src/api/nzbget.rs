use serde::de::DeserializeOwned;
use serde_json::json;

use crate::api::http::{check, join_url};
use crate::api::models::nzbget::{NzbGroup, NzbStatus};
use crate::config::UserPassService;
use crate::error::{CliarrError, Result};

const SERVICE: &str = "nzbget";

/// NZBGet JSON-RPC client (POST to /jsonrpc with HTTP Basic auth).
#[derive(Debug, Clone)]
pub struct NzbgetClient {
    base: String,
    username: String,
    password: String,
    http: reqwest::Client,
}

impl NzbgetClient {
    pub fn new(cfg: &UserPassService, http: reqwest::Client) -> Self {
        Self {
            base: cfg.url.clone(),
            username: cfg.username.clone(),
            password: cfg.password.clone(),
            http,
        }
    }

    async fn rpc<T: DeserializeOwned>(&self, method: &str, params: serde_json::Value) -> Result<T> {
        let url = join_url(&self.base, "/jsonrpc")?;
        let resp = self
            .http
            .post(url)
            .basic_auth(&self.username, Some(&self.password))
            .json(&json!({ "method": method, "params": params }))
            .send()
            .await?;
        let body: serde_json::Value = check(SERVICE, resp).await?.json().await?;
        if let Some(err) = body.get("error").filter(|e| !e.is_null()) {
            return Err(CliarrError::Api {
                service: SERVICE,
                status: 200,
                body: format!("RPC error from {method}: {err}"),
            });
        }
        let result = body
            .get("result")
            .cloned()
            .ok_or_else(|| CliarrError::Other(format!("nzbget: no result from {method}")))?;
        serde_json::from_value(result)
            .map_err(|e| CliarrError::Other(format!("nzbget: bad {method} response: {e}")))
    }

    pub async fn version(&self) -> Result<String> {
        self.rpc("version", json!([])).await
    }

    pub async fn status(&self) -> Result<NzbStatus> {
        self.rpc("status", json!([])).await
    }

    pub async fn listgroups(&self) -> Result<Vec<NzbGroup>> {
        self.rpc("listgroups", json!([0])).await
    }

    /// `editqueue` with GroupPause / GroupResume / GroupDelete etc.
    pub async fn edit_queue(&self, command: &str, ids: &[i64]) -> Result<bool> {
        self.rpc("editqueue", json!([command, "", ids])).await
    }

    pub async fn pause(&self, ids: &[i64]) -> Result<bool> {
        self.edit_queue("GroupPause", ids).await
    }

    pub async fn resume(&self, ids: &[i64]) -> Result<bool> {
        self.edit_queue("GroupResume", ids).await
    }

    pub async fn delete(&self, ids: &[i64]) -> Result<bool> {
        self.edit_queue("GroupDelete", ids).await
    }
}

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CliarrError, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub radarr: Option<ApiKeyService>,
    pub sonarr: Option<ApiKeyService>,
    pub plex: Option<TokenService>,
    pub qbittorrent: Option<UserPassService>,
    pub nzbget: Option<UserPassService>,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyService {
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenService {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPassService {
    pub url: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub poster_protocol: PosterProtocol,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            poster_protocol: PosterProtocol::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PosterProtocol {
    #[default]
    Auto,
    Kitty,
    Iterm2,
    Sixel,
    Halfblocks,
    Off,
}

impl Config {
    pub fn default_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "cliarr")
            .ok_or_else(|| CliarrError::Config("cannot determine config directory".into()))?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    pub fn path(override_path: Option<&Path>) -> Result<PathBuf> {
        match override_path {
            Some(p) => Ok(p.to_path_buf()),
            None => Self::default_path(),
        }
    }

    pub fn load(override_path: Option<&Path>) -> Result<Self> {
        let path = Self::path(override_path)?;
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            CliarrError::Config(format!(
                "cannot read {} ({e}) — run `cliarr config init` to create it",
                path.display()
            ))
        })?;
        toml::from_str(&raw)
            .map_err(|e| CliarrError::Config(format!("invalid config {}: {e}", path.display())))
    }

    pub fn save(&self, override_path: Option<&Path>) -> Result<PathBuf> {
        let path = Self::path(override_path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CliarrError::Config(format!("cannot create {}: {e}", parent.display())))?;
        }
        let raw = toml::to_string_pretty(self)
            .map_err(|e| CliarrError::Config(format!("cannot serialize config: {e}")))?;
        std::fs::write(&path, raw)
            .map_err(|e| CliarrError::Config(format!("cannot write {}: {e}", path.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(path)
    }

    /// Redacted copy for `config show` — secrets replaced with asterisks.
    pub fn redacted(&self) -> Self {
        fn mask(s: &str) -> String {
            if s.len() <= 4 {
                "****".into()
            } else {
                format!("{}****", &s[..4])
            }
        }
        let mut c = self.clone();
        if let Some(r) = &mut c.radarr {
            r.api_key = mask(&r.api_key);
        }
        if let Some(s) = &mut c.sonarr {
            s.api_key = mask(&s.api_key);
        }
        if let Some(p) = &mut c.plex {
            p.token = mask(&p.token);
        }
        if let Some(q) = &mut c.qbittorrent {
            q.password = mask(&q.password);
        }
        if let Some(n) = &mut c.nzbget {
            n.password = mask(&n.password);
        }
        c
    }
}

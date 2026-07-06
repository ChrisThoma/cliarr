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
                "cannot read {} ({e}); run `cliarr config init` to create it",
                path.display()
            ))
        })?;
        toml::from_str(&raw)
            .map_err(|e| CliarrError::Config(format!("invalid config {}: {e}", path.display())))
    }

    /// Like `load`, but a missing file yields defaults. Any other problem
    /// (malformed TOML, permissions) still errors, so `config init` can't
    /// silently replace a broken config with a fresh one.
    pub fn load_or_default(override_path: Option<&Path>) -> Result<Self> {
        let path = Self::path(override_path)?;
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => {
                return Err(CliarrError::Config(format!(
                    "cannot read {} ({e})",
                    path.display()
                )));
            }
        };
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
        // Write secrets to a 0600 temp file first, then rename over the
        // target: no window where the file is world-readable or half-written.
        let tmp = path.with_extension("toml.tmp");
        let _ = std::fs::remove_file(&tmp);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let write_err =
            |e: std::io::Error| CliarrError::Config(format!("cannot write {}: {e}", tmp.display()));
        {
            use std::io::Write;
            let mut file = opts.open(&tmp).map_err(write_err)?;
            file.write_all(raw.as_bytes()).map_err(write_err)?;
            file.sync_all().map_err(write_err)?;
        }
        std::fs::rename(&tmp, &path)
            .map_err(|e| CliarrError::Config(format!("cannot write {}: {e}", path.display())))?;
        Ok(path)
    }

    /// Redacted copy for `config show`: secrets replaced with asterisks.
    pub fn redacted(&self) -> Self {
        fn mask(s: &str) -> String {
            if s.chars().count() <= 4 {
                "****".into()
            } else {
                let head: String = s.chars().take(4).collect();
                format!("{head}****")
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

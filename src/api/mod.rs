pub mod arr_core;
pub mod http;
pub mod models;
pub mod nzbget;
pub mod plex;
pub mod qbittorrent;
pub mod radarr;
pub mod sonarr;

use std::sync::Arc;

use crate::config::Config;
use crate::error::{CliarrError, Result};

/// Bundle of clients for the configured services. Unconfigured services are
/// None; accessors return a typed NotConfigured error.
#[derive(Clone)]
pub struct Clients {
    pub radarr: Option<Arc<radarr::RadarrClient>>,
    pub sonarr: Option<Arc<sonarr::SonarrClient>>,
    pub plex: Option<Arc<plex::PlexClient>>,
    pub qbit: Option<Arc<qbittorrent::QbitClient>>,
    pub nzbget: Option<Arc<nzbget::NzbgetClient>>,
}

impl Clients {
    pub fn from_config(config: &Config) -> Self {
        let http = http::build_client();
        Self {
            radarr: config
                .radarr
                .as_ref()
                .map(|c| Arc::new(radarr::RadarrClient::new(c, http.clone()))),
            sonarr: config
                .sonarr
                .as_ref()
                .map(|c| Arc::new(sonarr::SonarrClient::new(c, http.clone()))),
            plex: config
                .plex
                .as_ref()
                .map(|c| Arc::new(plex::PlexClient::new(c, http.clone()))),
            qbit: config
                .qbittorrent
                .as_ref()
                .map(|c| Arc::new(qbittorrent::QbitClient::new(c, http.clone()))),
            nzbget: config
                .nzbget
                .as_ref()
                .map(|c| Arc::new(nzbget::NzbgetClient::new(c, http.clone()))),
        }
    }

    pub fn radarr(&self) -> Result<&radarr::RadarrClient> {
        self.radarr.as_deref().ok_or(CliarrError::NotConfigured("radarr"))
    }

    pub fn sonarr(&self) -> Result<&sonarr::SonarrClient> {
        self.sonarr.as_deref().ok_or(CliarrError::NotConfigured("sonarr"))
    }

    pub fn plex(&self) -> Result<&plex::PlexClient> {
        self.plex.as_deref().ok_or(CliarrError::NotConfigured("plex"))
    }

    pub fn qbit(&self) -> Result<&qbittorrent::QbitClient> {
        self.qbit.as_deref().ok_or(CliarrError::NotConfigured("qbittorrent"))
    }

    pub fn nzbget(&self) -> Result<&nzbget::NzbgetClient> {
        self.nzbget.as_deref().ok_or(CliarrError::NotConfigured("nzbget"))
    }
}

//! Spawned async fetches. Every fetch resolves to a `DataMsg` sent back on
//! the event channel; the UI never awaits network calls directly.

use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;

use crate::api::Clients;
use crate::tui::event::{DataMsg, Event};

fn send(tx: &UnboundedSender<Event>, msg: DataMsg) {
    let _ = tx.send(Event::Data(msg));
}

fn err_str<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn dashboard(tx: UnboundedSender<Event>, clients: Arc<Clients>) {
    if let Some(radarr) = clients.radarr.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(&tx, DataMsg::RadarrHealth(radarr.system_status().await.map_err(err_str)));
        });
    }
    if let Some(sonarr) = clients.sonarr.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(&tx, DataMsg::SonarrHealth(sonarr.system_status().await.map_err(err_str)));
        });
    }
    if let Some(plex) = clients.plex.clone() {
        let tx2 = tx.clone();
        let plex2 = plex.clone();
        tokio::spawn(async move {
            let result = plex2
                .identity()
                .await
                .map(|i| format!("v{}", i.version))
                .map_err(err_str);
            send(&tx2, DataMsg::PlexHealth(result));
        });
        let tx2 = tx.clone();
        tokio::spawn(async move {
            send(&tx2, DataMsg::PlexSessions(plex.sessions().await.map_err(err_str)));
        });
    }
    if let Some(qbit) = clients.qbit.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = async {
                qbit.login().await?;
                qbit.app_version().await
            }
            .await
            .map(|v| v.trim().to_string())
            .map_err(err_str);
            send(&tx, DataMsg::QbitHealth(result));
        });
    }
    if let Some(nzbget) = clients.nzbget.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(
                &tx,
                DataMsg::NzbHealth(nzbget.version().await.map(|v| format!("v{v}")).map_err(err_str)),
            );
        });
    }
}

pub fn movies(tx: UnboundedSender<Event>, clients: Arc<Clients>) {
    if let Some(radarr) = clients.radarr.clone() {
        tokio::spawn(async move {
            send(&tx, DataMsg::RadarrMovies(radarr.movies().await.map_err(err_str)));
        });
    }
}

pub fn series(tx: UnboundedSender<Event>, clients: Arc<Clients>) {
    if let Some(sonarr) = clients.sonarr.clone() {
        tokio::spawn(async move {
            send(&tx, DataMsg::SonarrSeries(sonarr.series().await.map_err(err_str)));
        });
    }
}

pub fn lookup_movies(tx: UnboundedSender<Event>, clients: Arc<Clients>, term: String, seq: u64) {
    if let Some(radarr) = clients.radarr.clone() {
        tokio::spawn(async move {
            let result = radarr.lookup(&term).await.map_err(err_str);
            send(&tx, DataMsg::RadarrLookup { seq, result });
        });
    }
}

pub fn lookup_series(tx: UnboundedSender<Event>, clients: Arc<Clients>, term: String, seq: u64) {
    if let Some(sonarr) = clients.sonarr.clone() {
        tokio::spawn(async move {
            let result = sonarr.lookup(&term).await.map_err(err_str);
            send(&tx, DataMsg::SonarrLookup { seq, result });
        });
    }
}

/// Quality profiles + root folders for the add modal.
pub fn add_options(tx: UnboundedSender<Event>, clients: Arc<Clients>, for_movies: bool) {
    tokio::spawn(async move {
        let result = if for_movies {
            match clients.radarr() {
                Ok(radarr) => tokio::try_join!(radarr.quality_profiles(), radarr.root_folders()),
                Err(e) => Err(e),
            }
        } else {
            match clients.sonarr() {
                Ok(sonarr) => tokio::try_join!(sonarr.quality_profiles(), sonarr.root_folders()),
                Err(e) => Err(e),
            }
        };
        send(&tx, DataMsg::AddOptions(result.map_err(err_str)));
    });
}

pub fn downloads(tx: UnboundedSender<Event>, clients: Arc<Clients>) {
    if let Some(radarr) = clients.radarr.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(&tx, DataMsg::RadarrQueue(radarr.queue().await.map_err(err_str)));
        });
    }
    if let Some(sonarr) = clients.sonarr.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(&tx, DataMsg::SonarrQueue(sonarr.queue().await.map_err(err_str)));
        });
    }
    if let Some(qbit) = clients.qbit.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(&tx, DataMsg::Torrents(qbit.torrents(None).await.map_err(err_str)));
        });
    }
    if let Some(nzbget) = clients.nzbget.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(&tx, DataMsg::NzbGroups(nzbget.listgroups().await.map_err(err_str)));
        });
    }
}

pub fn calendar(tx: UnboundedSender<Event>, clients: Arc<Clients>, days: i64) {
    tokio::spawn(async move {
        let result = crate::commands::calendar::fetch(days, &clients).await.map_err(err_str);
        send(&tx, DataMsg::Calendar(result));
    });
}

pub fn missing(tx: UnboundedSender<Event>, clients: Arc<Clients>) {
    if let Some(radarr) = clients.radarr.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(&tx, DataMsg::RadarrMissing(radarr.missing().await.map_err(err_str)));
        });
    }
    if let Some(sonarr) = clients.sonarr.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            send(&tx, DataMsg::SonarrMissing(sonarr.missing().await.map_err(err_str)));
        });
    }
}

/// Fire-and-forget action; completion arrives as `ActionDone` for the toast.
/// `origin` is the tab the action was launched from, so the right data gets
/// refreshed even if the user switches tabs before it completes.
pub fn action<F>(tx: UnboundedSender<Event>, origin: crate::tui::app::Tab, desc: String, fut: F)
where
    F: std::future::Future<Output = crate::error::Result<()>> + Send + 'static,
{
    tokio::spawn(async move {
        let result = fut.await.map_err(err_str);
        send(&tx, DataMsg::ActionDone { origin, desc, result });
    });
}

use crossterm::event::KeyEvent;

use crate::api::models::arr::{Paged, QualityProfile, QueueItem, RootFolder, SystemStatus};
use crate::api::models::nzbget::NzbGroup;
use crate::api::models::plex::SessionList;
use crate::api::models::qbit::Torrent;
use crate::api::models::radarr::Movie;
use crate::api::models::sonarr::{Episode, Series};
use crate::commands::calendar::CalendarEntry;

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Resize,
    Tick,
    Data(DataMsg),
    Poster { url: String, result: Result<image::DynamicImage, String> },
}

/// Completed async fetches. Errors are pre-stringified so the message stays
/// Send + cheap to store.
#[derive(Debug)]
pub enum DataMsg {
    RadarrHealth(Result<SystemStatus, String>),
    SonarrHealth(Result<SystemStatus, String>),
    PlexHealth(Result<String, String>),
    QbitHealth(Result<String, String>),
    NzbHealth(Result<String, String>),

    RadarrMovies(Result<Vec<Movie>, String>),
    SonarrSeries(Result<Vec<Series>, String>),

    /// `seq` echoes SearchState::seq at request time; stale responses
    /// (an older search finishing after a newer one) are dropped.
    RadarrLookup { seq: u64, result: Result<Vec<Movie>, String> },
    SonarrLookup { seq: u64, result: Result<Vec<Series>, String> },
    AddOptions(Result<(Vec<QualityProfile>, Vec<RootFolder>), String>),

    RadarrQueue(Result<Vec<QueueItem>, String>),
    SonarrQueue(Result<Vec<QueueItem>, String>),
    Torrents(Result<Vec<Torrent>, String>),
    NzbGroups(Result<Vec<NzbGroup>, String>),

    Calendar(Result<Vec<CalendarEntry>, String>),

    RadarrMissing(Result<Paged<Movie>, String>),
    SonarrMissing(Result<Paged<Episode>, String>),

    PlexSessions(Result<SessionList, String>),

    /// A fire-and-forget action (add, delete, pause…) finished.
    ActionDone {
        desc: String,
        result: Result<(), String>,
    },
}

/// Async data slot: one shape for every fetched field.
#[derive(Debug, Clone, Default)]
pub enum Loadable<T> {
    #[default]
    NotAsked,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Loadable::Ready(t) => Some(t),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Loadable::Loading)
    }

    pub fn set(&mut self, result: Result<T, String>) {
        *self = match result {
            Ok(t) => Loadable::Ready(t),
            Err(e) => Loadable::Failed(e),
        };
    }
}

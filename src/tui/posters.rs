//! Poster art: protocol detection, disk + memory cache, async downloads.

use std::collections::HashMap;

use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;
use tokio::sync::mpsc::UnboundedSender;

use crate::cache;
use crate::config::PosterProtocol;
use crate::tui::event::Event;

const MAX_CACHED: usize = 60;
const MAX_CONCURRENT_DOWNLOADS: usize = 4;

// Ready dominates the map anyway; boxing would just add indirection.
#[allow(clippy::large_enum_variant)]
enum PosterState {
    Loading,
    Ready(StatefulProtocol),
    Failed,
}

pub struct PosterManager {
    picker: Option<Picker>,
    states: HashMap<String, PosterState>,
    order: Vec<String>,
    tx: UnboundedSender<Event>,
    http: reqwest::Client,
    semaphore: std::sync::Arc<tokio::sync::Semaphore>,
}

impl PosterManager {
    /// Must run BEFORE the terminal enters raw mode/alternate screen: the
    /// stdio query writes escape sequences and reads the reply.
    pub fn detect(setting: PosterProtocol, tx: UnboundedSender<Event>, http: reqwest::Client) -> Self {
        let picker = match setting {
            PosterProtocol::Off => None,
            PosterProtocol::Auto => Picker::from_query_stdio().ok(),
            forced => {
                // Query for font size where possible; fall back to a typical
                // cell size, then pin the protocol the user asked for.
                let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
                picker.set_protocol_type(match forced {
                    PosterProtocol::Kitty => ratatui_image::picker::ProtocolType::Kitty,
                    PosterProtocol::Iterm2 => ratatui_image::picker::ProtocolType::Iterm2,
                    PosterProtocol::Sixel => ratatui_image::picker::ProtocolType::Sixel,
                    _ => ratatui_image::picker::ProtocolType::Halfblocks,
                });
                Some(picker)
            }
        };
        Self {
            picker,
            states: HashMap::new(),
            order: Vec::new(),
            tx,
            http,
            semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_DOWNLOADS)),
        }
    }

    pub fn enabled(&self) -> bool {
        self.picker.is_some()
    }

    /// Ask for a poster; kicks off a download unless cached/in-flight.
    pub fn request(&mut self, url: &str) {
        if self.picker.is_none() || self.states.contains_key(url) {
            return;
        }
        self.states.insert(url.to_string(), PosterState::Loading);
        let url = url.to_string();
        let tx = self.tx.clone();
        let http = self.http.clone();
        let semaphore = self.semaphore.clone();
        tokio::spawn(async move {
            let _permit = semaphore.acquire().await;
            let result = load_poster(&http, &url).await;
            let _ = tx.send(Event::Poster { url, result });
        });
    }

    pub fn on_loaded(&mut self, url: &str, result: Result<image::DynamicImage, String>) {
        let Some(picker) = &mut self.picker else { return };
        let state = match result {
            Ok(img) => PosterState::Ready(picker.new_resize_protocol(img)),
            Err(_) => PosterState::Failed,
        };
        self.states.insert(url.to_string(), state);
        self.order.retain(|u| u != url);
        self.order.push(url.to_string());
        // Evict oldest decoded posters beyond the cap (disk cache still has them).
        while self.order.len() > MAX_CACHED {
            let evict = self.order.remove(0);
            self.states.remove(&evict);
        }
    }

    /// Render the poster for `url` into `area` (requests it if needed).
    /// Returns false when nothing is drawable yet.
    pub fn render(&mut self, f: &mut Frame, area: Rect, url: &str) -> bool {
        if self.picker.is_none() {
            return false;
        }
        self.request(url);
        match self.states.get_mut(url) {
            Some(PosterState::Ready(protocol)) => {
                f.render_stateful_widget(StatefulImage::default(), area, protocol);
                true
            }
            _ => false,
        }
    }
}

async fn load_poster(http: &reqwest::Client, url: &str) -> Result<image::DynamicImage, String> {
    let path = cache::poster_path(url).map_err(|e| e.to_string())?;
    if let Ok(bytes) = tokio::fs::read(&path).await
        && let Ok(img) = image::load_from_memory(&bytes)
    {
        return Ok(img);
    }
    // Cache miss or corrupt entry: drop it and download fresh.
    let _ = tokio::fs::remove_file(&path).await;
    let resp = http.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    // Cache only after a successful decode so corrupt bytes never persist.
    let _ = tokio::fs::write(&path, &bytes).await;
    Ok(img)
}

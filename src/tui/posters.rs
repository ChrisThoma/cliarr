//! Poster art: protocol detection, disk + memory cache, async downloads.

use std::collections::HashMap;

use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::FontSize;
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
    /// Must run AFTER the terminal enters raw mode and BEFORE the input
    /// reader spawns: the auto query talks over the shared tty, and running
    /// inside raw mode makes any late termios restore from the query a
    /// no-op instead of un-rawing the session.
    pub fn detect(setting: PosterProtocol, tx: UnboundedSender<Event>, http: reqwest::Client) -> Self {
        let picker = match setting {
            PosterProtocol::Off => None,
            PosterProtocol::Auto => query_in_subprocess(),
            forced => {
                // Never query stdio here: the user already chose the
                // protocol, and the query's only upside is a font-size
                // guess. Take the typical-cell-size default and pin.
                let mut picker = Picker::halfblocks();
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

/// Argv[1] sentinel for the internal protocol-query subprocess mode.
pub const QUERY_ARG: &str = "__poster-query";

/// Subprocess side of auto-detection: run the stdio query and report the
/// result on stderr (stdout carries the escape-sequence dialogue with the
/// terminal). Never returns.
pub fn query_and_report() -> ! {
    let Ok(picker) = Picker::from_query_stdio() else {
        std::process::exit(1);
    };
    let proto = match picker.protocol_type() {
        ProtocolType::Kitty => "kitty",
        ProtocolType::Iterm2 => "iterm2",
        ProtocolType::Sixel => "sixel",
        ProtocolType::Halfblocks => "halfblocks",
    };
    let font = picker.font_size();
    eprintln!("{proto} {} {}", font.width, font.height);
    std::process::exit(0);
}

/// Run `Picker::from_query_stdio` in a killable child process sharing our
/// tty. The in-process version leaks a thread blocked on stdin whenever the
/// terminal never answers the query (ttyd, odd multiplexers); that thread
/// then steals keystrokes and rewrites termios mid-session. A child can be
/// waited on with a deadline, and its stuck reader dies with it.
fn query_in_subprocess() -> Option<Picker> {
    use std::io::Read;
    use std::time::{Duration, Instant};

    let exe = std::env::current_exe().ok()?;
    let mut child = std::process::Command::new(exe)
        .arg(QUERY_ARG)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let mut stderr = child.stderr.take()?;

    // The child's own query timeout is 2s; give it headroom, then kill.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut out = String::new();
                stderr.read_to_string(&mut out).ok()?;
                let mut parts = out.split_whitespace();
                let proto = match parts.next()? {
                    "kitty" => ProtocolType::Kitty,
                    "iterm2" => ProtocolType::Iterm2,
                    "sixel" => ProtocolType::Sixel,
                    "halfblocks" => ProtocolType::Halfblocks,
                    _ => return None,
                };
                let width = parts.next()?.parse().ok()?;
                let height = parts.next()?.parse().ok()?;
                #[allow(deprecated)] // reconstructing a query result, not guessing
                let mut picker = Picker::from_fontsize(FontSize::new(width, height));
                picker.set_protocol_type(proto);
                return Some(picker);
            }
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return None,
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

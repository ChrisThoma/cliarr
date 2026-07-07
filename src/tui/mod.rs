pub mod app;
pub mod event;
pub mod fetch;
pub mod posters;
pub mod screens;
pub mod theme;

use std::sync::Arc;

use futures::StreamExt;

use crate::api::Clients;
use crate::config::Config;
use crate::error::{CliarrError, Result};
use crate::tui::app::App;
use crate::tui::event::Event;
use crate::tui::posters::PosterManager;

pub async fn run(config: Config, initial_query: Option<String>) -> Result<()> {
    let clients = Arc::new(Clients::from_config(&config));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // Poster protocol detection must query stdio BEFORE raw mode/alt screen.
    let posters = PosterManager::detect(
        config.ui.poster_protocol,
        tx.clone(),
        crate::api::http::build_client(),
    );

    // Input reader.
    let input_tx = tx.clone();
    tokio::spawn(async move {
        let mut events = crossterm::event::EventStream::new();
        while let Some(ev) = events.next().await {
            let msg = match ev {
                Ok(crossterm::event::Event::Key(key))
                    if key.kind != crossterm::event::KeyEventKind::Release =>
                {
                    Event::Key(key)
                }
                Ok(crossterm::event::Event::Resize(_, _)) => Event::Resize,
                Ok(_) => continue,
                // The tty input stream died: a TUI that can't hear keys can't
                // even be quit, so shut down instead of freezing.
                Err(_) => Event::InputClosed,
            };
            if input_tx.send(msg).is_err() {
                break;
            }
        }
    });

    // Ticker: spinners, toast expiry, auto-refresh.
    let tick_tx = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            interval.tick().await;
            if tick_tx.send(Event::Tick).is_err() {
                break;
            }
        }
    });

    let mut terminal = ratatui::init();
    let mut app = App::new(clients, tx, posters);
    app.load_tab();
    if let Some(query) = initial_query {
        app.start_with_query(query);
    }

    let result = loop {
        if let Err(e) = terminal.draw(|f| app.draw(f)) {
            break Err(CliarrError::Other(format!("draw failed: {e}")));
        }
        let Some(ev) = rx.recv().await else { break Ok(()) };
        app.handle(ev);
        // Coalesce whatever else is queued before redrawing.
        while let Ok(ev) = rx.try_recv() {
            app.handle(ev);
        }
        if app.should_quit {
            break Ok(());
        }
    };

    ratatui::restore();
    result
}

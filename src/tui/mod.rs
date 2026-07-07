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
    // Bail before raw mode / the input reader: crossterm's EventStream
    // panics without a tty, and a TUI drawn into a pipe is garbage anyway.
    {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            return Err(CliarrError::Other(
                "the interactive TUI needs a terminal; for scripted output use a subcommand \
                 like `cliarr movie list` (see `cliarr --help`)"
                    .into(),
            ));
        }
    }

    // Refresh the daily update check while the user is in the TUI; the
    // notice itself is printed after the terminal is restored.
    tokio::spawn(crate::update::passive_refresh());

    let clients = Arc::new(Clients::from_config(&config));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // Raw mode first: the poster query below may leak a thread that later
    // restores the termios it saw on entry, so that snapshot must already
    // be raw (see PosterManager::detect).
    let mut terminal = ratatui::try_init()
        .map_err(|e| CliarrError::Other(format!("cannot initialize the terminal: {e}")))?;

    let posters = PosterManager::detect(
        config.ui.poster_protocol,
        tx.clone(),
        crate::api::http::build_client(),
    );

    // A late reply to the poster query must not reach the app as key
    // events; drain whatever is pending before the input reader starts.
    if config.ui.poster_protocol == crate::config::PosterProtocol::Auto {
        while crossterm::event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
            let _ = crossterm::event::read();
        }
    }

    // Input reader: must spawn after the poster query, which reads stdin.
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
    crate::update::print_notice_if_cached_newer();
    result
}

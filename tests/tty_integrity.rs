//! The TUI must stay responsive in a terminal that never answers the poster
//! protocol query (ttyd, odd multiplexers, bare PTYs). ratatui-image's
//! `from_query_stdio` leaves a detached thread blocked on stdin when the
//! query times out; when it finally wakes it restores the pre-query (cooked)
//! termios, killing keyboard input for the whole session. A plain PTY
//! reproduces that deterministically: it never replies to escape queries.

use std::io::Read;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// Launch the TUI in a silent PTY, wait past the poster query timeout,
/// then drive it with keys and require a clean quit. If the tty was
/// flipped back to cooked mode, `q` gets line-buffered, never reaches
/// the app, and the child outlives the deadline.
fn tui_quits_cleanly(poster_protocol: &str) {
    let config_path = std::env::temp_dir().join(format!(
        "cliarr-tty-test-{}-{poster_protocol}.toml",
        std::process::id()
    ));
    std::fs::write(&config_path, format!("[ui]\nposter_protocol = \"{poster_protocol}\"\n"))
        .unwrap();

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })
        .unwrap();

    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_cliarr"));
    cmd.args(["--config", config_path.to_str().unwrap()]);
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    // Drain the child's output so it never blocks on a full PTY buffer.
    let mut reader = pair.master.try_clone_reader().unwrap();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while matches!(reader.read(&mut buf), Ok(n) if n > 0) {}
    });

    // Outlive protocol detection entirely (startup + the query's 2s stdin
    // timeout, with margin for a loaded test host). Keys typed while the
    // query is still awaiting its reply are legitimately consumed as
    // response bytes — the guarantee under test is that input works, and
    // stays working, once detection has concluded.
    std::thread::sleep(Duration::from_secs(5));

    let mut writer = pair.master.take_writer().unwrap();
    use std::io::Write;
    // Down x3 (arrows are what leaked as ^[[B in the field), then leave the
    // omnibox with Esc, then quit. Gaps let crossterm see Esc as a lone key.
    for key in [b"\x1b[B" as &[u8], b"\x1b[B", b"\x1b[B", b"\x1b", b"q"] {
        writer.write_all(key).unwrap();
        writer.flush().unwrap();
        std::thread::sleep(Duration::from_millis(150));
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait().unwrap() {
            Some(status) => {
                assert!(status.success(), "cliarr exited with {status:?}");
                break;
            }
            None if Instant::now() > deadline => {
                child.kill().unwrap();
                panic!(
                    "cliarr ({poster_protocol}) ignored `q` for 5s: tty likely \
                     flipped back to cooked mode by the poster protocol query"
                );
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let _ = std::fs::remove_file(&config_path);
}

#[test]
fn survives_silent_terminal_with_forced_protocol() {
    tui_quits_cleanly("halfblocks");
}

#[test]
fn survives_silent_terminal_with_auto_protocol() {
    tui_quits_cleanly("auto");
}

#[test]
fn survives_silent_terminal_with_posters_off() {
    tui_quits_cleanly("off");
}

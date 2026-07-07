use std::ffi::OsString;
use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "cliarr",
    version,
    about = "Manage your home media stack from the terminal",
    long_about = "Manage your home media stack from the terminal.\n\nRun without a subcommand to launch the interactive TUI.\nAny bare words launch the TUI already searching: `cliarr dune part two`.",
    override_usage = "cliarr [OPTIONS] [QUERY]...\n       cliarr [OPTIONS] <COMMAND>"
)]
pub struct Cli {
    /// Search query; launches the TUI with this search already running.
    /// Filled by the query fallback parse, never by this parser (a positional
    /// here would swallow `movie list` after a global flag like `--json`).
    #[arg(skip)]
    pub query: Vec<String>,

    /// Output JSON instead of tables
    #[arg(long, global = true)]
    pub json: bool,

    /// Path to the config file (default: ~/.config/cliarr/config.toml)
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Fallback parser for the search-first form: everything after the flags is
/// the query, verbatim.
#[derive(Debug, Parser)]
#[command(name = "cliarr", version)]
struct QueryCli {
    #[arg(value_name = "QUERY", trailing_var_arg = true)]
    query: Vec<String>,

    #[arg(long)]
    json: bool,

    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

impl Cli {
    /// Parse in two stages: subcommands win, and only an unknown first word
    /// (`cliarr dune part two`) falls back to being a TUI search query. This
    /// keeps global flags working in both positions (`cliarr --json movie
    /// list` and `cliarr movie list --json`).
    pub fn parse_args() -> Self {
        Self::try_parse_args(std::env::args_os()).unwrap_or_else(|e| e.exit())
    }

    pub fn try_parse_args(
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Result<Self, clap::Error> {
        let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
        match Self::try_parse_from(&args) {
            Ok(cli) => Ok(cli),
            Err(e) if e.kind() == ErrorKind::InvalidSubcommand => {
                let q = QueryCli::try_parse_from(&args)?;
                Ok(Cli { query: q.query, json: q.json, config: q.config, command: None })
            }
            Err(e) => Err(e),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Manage cliarr configuration
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Movies (Radarr)
    Movie {
        #[command(subcommand)]
        cmd: MovieCmd,
    },
    /// TV series (Sonarr)
    Series {
        #[command(subcommand)]
        cmd: SeriesCmd,
    },
    /// Radarr/Sonarr download queues
    Queue {
        #[command(subcommand)]
        cmd: Option<QueueCmd>,
        /// Which service's queue to show
        #[arg(long, value_enum, default_value_t = ArrService::All)]
        service: ArrService,
    },
    /// Upcoming releases from Radarr and Sonarr
    Calendar {
        /// How many days ahead to show
        #[arg(long, default_value_t = 7)]
        days: i64,
    },
    /// Wanted/missing items
    Missing {
        #[arg(long, value_enum, default_value_t = ArrService::All)]
        service: ArrService,
    },
    /// qBittorrent torrents
    Torrents {
        #[command(subcommand)]
        cmd: Option<TorrentsCmd>,
    },
    /// NZBGet downloads
    Nzb {
        #[command(subcommand)]
        cmd: Option<NzbCmd>,
    },
    /// Plex server status
    Plex {
        #[command(subcommand)]
        cmd: PlexCmd,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Interactively create or update the config file
    Init,
    /// Print the current config (secrets redacted)
    Show,
    /// Ping every configured service and report connectivity
    Test,
}

#[derive(Debug, Subcommand)]
pub enum MovieCmd {
    /// Look up a movie by name
    Search { query: String },
    /// Add a movie to Radarr by TMDB id
    Add {
        tmdb_id: i64,
        /// Quality profile (name or id); prompts if omitted
        #[arg(long)]
        profile: Option<String>,
        /// Root folder path; prompts if omitted
        #[arg(long)]
        root: Option<String>,
        /// Don't search for the movie after adding it
        #[arg(long)]
        no_search: bool,
        /// Add unmonitored
        #[arg(long)]
        unmonitored: bool,
    },
    /// List movies in the library
    List {
        #[arg(long, value_enum)]
        filter: Option<LibraryFilter>,
    },
    /// Remove a movie from Radarr
    Remove {
        id: i64,
        /// Also delete the movie files on disk
        #[arg(long)]
        delete_files: bool,
        /// Add a list-import exclusion so it doesn't come back
        #[arg(long)]
        exclude: bool,
    },
    /// Trigger a search for all missing movies
    SearchMissing,
    /// Refresh/rescan one movie, or the whole library if no id is given
    Refresh { id: Option<i64> },
}

#[derive(Debug, Subcommand)]
pub enum SeriesCmd {
    /// Look up a series by name
    Search { query: String },
    /// Add a series to Sonarr by TVDB id
    Add {
        tvdb_id: i64,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        root: Option<String>,
        /// Don't search for episodes after adding
        #[arg(long)]
        no_search: bool,
        /// Add unmonitored
        #[arg(long)]
        unmonitored: bool,
        /// Don't create season folders
        #[arg(long)]
        no_season_folders: bool,
    },
    /// List series in the library
    List {
        #[arg(long, value_enum)]
        filter: Option<LibraryFilter>,
    },
    /// Remove a series from Sonarr
    Remove {
        id: i64,
        #[arg(long)]
        delete_files: bool,
        #[arg(long)]
        exclude: bool,
    },
    /// Trigger a search for all missing episodes
    SearchMissing,
    /// Refresh/rescan one series, or the whole library if no id is given
    Refresh { id: Option<i64> },
}

#[derive(Debug, Subcommand)]
pub enum QueueCmd {
    /// List queue items (default)
    List,
    /// Remove a queue item
    Remove {
        id: i64,
        /// Which service the queue item belongs to
        #[arg(long, value_enum)]
        service: ArrServiceOne,
        /// Blocklist the release so it isn't grabbed again
        #[arg(long)]
        blocklist: bool,
        /// Also remove the download from the download client
        #[arg(long)]
        remove_from_client: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum TorrentsCmd {
    /// List torrents (default)
    List {
        /// Filter: downloading, seeding, paused, completed, errored, all…
        #[arg(long)]
        filter: Option<String>,
    },
    /// Pause torrents by (partial) hash
    Pause { hashes: Vec<String> },
    /// Resume torrents by (partial) hash
    Resume { hashes: Vec<String> },
    /// Delete torrents by (partial) hash
    Delete {
        hashes: Vec<String>,
        /// Also delete downloaded data
        #[arg(long)]
        delete_files: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum NzbCmd {
    /// List queued downloads (default)
    List,
    /// Pause downloads by id
    Pause { ids: Vec<i64> },
    /// Resume downloads by id
    Resume { ids: Vec<i64> },
    /// Delete downloads by id
    Delete { ids: Vec<i64> },
}

#[derive(Debug, Subcommand)]
pub enum PlexCmd {
    /// Server info, library sections and active sessions
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ArrService {
    Radarr,
    Sonarr,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ArrServiceOne {
    Radarr,
    Sonarr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LibraryFilter {
    Missing,
    Monitored,
    Unmonitored,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_args(args.iter().copied())
            .unwrap_or_else(|e| panic!("{args:?} must parse: {e}"))
    }

    #[test]
    fn clap_debug_assert() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
        QueryCli::command().debug_assert();
    }

    #[test]
    fn global_flags_work_before_and_after_subcommands() {
        for args in [
            &["cliarr", "--json", "movie", "list"],
            &["cliarr", "movie", "list", "--json"],
        ] {
            let cli = parse(args);
            assert!(cli.json, "{args:?} must set --json");
            assert!(cli.query.is_empty(), "{args:?} must not become a query");
            assert!(
                matches!(cli.command, Some(Commands::Movie { cmd: MovieCmd::List { .. } })),
                "{args:?} must parse as `movie list`, got {:?}",
                cli.command
            );
        }
    }

    #[test]
    fn global_config_flag_before_subcommand() {
        let cli = parse(&["cliarr", "--config", "/tmp/c.toml", "queue"]);
        assert_eq!(cli.config.as_deref(), Some(std::path::Path::new("/tmp/c.toml")));
        assert!(matches!(cli.command, Some(Commands::Queue { .. })));
    }

    #[test]
    fn bare_words_become_a_search_query() {
        let cli = parse(&["cliarr", "dune", "part", "two"]);
        assert!(cli.command.is_none());
        assert_eq!(cli.query, ["dune", "part", "two"]);
    }

    #[test]
    fn flags_still_parse_ahead_of_a_query() {
        let cli = parse(&["cliarr", "--json", "dune"]);
        assert!(cli.json);
        assert!(cli.command.is_none());
        assert_eq!(cli.query, ["dune"]);
    }

    #[test]
    fn no_args_launches_plain_tui() {
        let cli = parse(&["cliarr"]);
        assert!(cli.command.is_none());
        assert!(cli.query.is_empty());
    }

    #[test]
    fn unknown_flags_are_still_errors_not_queries() {
        assert!(Cli::try_parse_args(["cliarr", "--nope"]).is_err());
    }
}

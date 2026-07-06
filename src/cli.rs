use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "cliarr",
    version,
    about = "Manage your home media stack from the terminal",
    long_about = "Manage your home media stack from the terminal.\n\nRun without a subcommand to launch the interactive TUI.\nAny bare words launch the TUI already searching: `cliarr dune part two`.",
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    /// Search query; launches the TUI with this search already running
    #[arg(value_name = "QUERY", trailing_var_arg = true)]
    pub query: Vec<String>,

    /// Output JSON instead of tables
    #[arg(long, global = true)]
    pub json: bool,

    /// Path to the config file (default: ~/.config/cliarr/config.toml)
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Disable colored output
    #[arg(long, global = true, env = "NO_COLOR")]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
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

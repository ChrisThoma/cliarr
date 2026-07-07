use cliarr::api::Clients;
use cliarr::cli::{Cli, Commands};
use cliarr::commands;
use cliarr::config::Config;
use cliarr::error::Result;
use cliarr::tui;

#[tokio::main]
async fn main() {
    // Internal mode: the TUI re-invokes itself with this sentinel to run
    // the poster protocol query in a killable process (see tui::posters).
    if std::env::args().nth(1).as_deref() == Some(tui::posters::QUERY_ARG) {
        tui::posters::query_and_report();
    }

    let cli = Cli::parse_args();
    if let Err(e) = run(cli).await {
        eprintln!("error: {e}");
        std::process::exit(e.exit_code());
    }
}

async fn run(cli: Cli) -> Result<()> {
    let config_path = cli.config.as_deref();

    let Some(command) = cli.command else {
        let config = Config::load(config_path)?;
        let query = if cli.query.is_empty() {
            None
        } else {
            Some(cli.query.join(" "))
        };
        return tui::run(config, query).await;
    };

    // `config` and `update` must work before a config file exists.
    if let Commands::Config { cmd } = command {
        return commands::config_cmd::run(cmd, config_path, cli.json).await;
    }
    if let Commands::Update { check } = command {
        return commands::update::run(check, cli.json).await;
    }

    let config = Config::load(config_path)?;
    let clients = Clients::from_config(&config);
    let json = cli.json;

    let result = match command {
        Commands::Config { .. } | Commands::Update { .. } => unreachable!(),
        Commands::Movie { cmd } => commands::movie::run(cmd, &clients, json).await,
        Commands::Series { cmd } => commands::series::run(cmd, &clients, json).await,
        Commands::Queue { cmd, service } => commands::queue::run(cmd, service, &clients, json).await,
        Commands::Calendar { days } => commands::calendar::run(days, &clients, json).await,
        Commands::Missing { service } => commands::missing::run(service, &clients, json).await,
        Commands::Torrents { cmd } => commands::torrents::run(cmd, &clients, json).await,
        Commands::Nzb { cmd } => commands::nzb::run(cmd, &clients, json).await,
        Commands::Plex { cmd } => commands::plex_cmd::run(cmd, &clients, json).await,
    };

    cliarr::update::passive_check_and_notify().await;
    result
}

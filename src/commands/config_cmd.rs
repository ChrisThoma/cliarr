use std::path::Path;

use crate::api::Clients;
use crate::cli::ConfigCmd;
use crate::commands::{output, prompt};
use crate::config::{ApiKeyService, Config, TokenService, UserPassService};
use crate::error::Result;

pub async fn run(cmd: ConfigCmd, config_path: Option<&Path>, json: bool) -> Result<()> {
    match cmd {
        ConfigCmd::Init => init(config_path).await,
        ConfigCmd::Show => show(config_path, json),
        ConfigCmd::Test => test(config_path, json).await,
    }
}

fn show(config_path: Option<&Path>, json: bool) -> Result<()> {
    let config = Config::load(config_path)?.redacted();
    if json {
        return output::print_json(&config);
    }
    let path = Config::path(config_path)?;
    println!("# {}", path.display());
    print!(
        "{}",
        toml::to_string_pretty(&config).unwrap_or_else(|e| format!("<serialize error: {e}>"))
    );
    Ok(())
}

async fn init(config_path: Option<&Path>) -> Result<()> {
    let mut config = Config::load(config_path).unwrap_or_default();
    println!("cliarr setup — press Enter to skip a service or keep the current value.\n");

    if ask_service("Radarr")? {
        let current = config.radarr.take();
        let url = ask_url("Radarr URL", current.as_ref().map(|c| c.url.as_str()), "http://nas.local:7878")?;
        let api_key = ask_secret("Radarr API key (Settings → General)", current.as_ref().map(|c| c.api_key.as_str()))?;
        config.radarr = Some(ApiKeyService { url, api_key });
    }
    if ask_service("Sonarr")? {
        let current = config.sonarr.take();
        let url = ask_url("Sonarr URL", current.as_ref().map(|c| c.url.as_str()), "http://nas.local:8989")?;
        let api_key = ask_secret("Sonarr API key (Settings → General)", current.as_ref().map(|c| c.api_key.as_str()))?;
        config.sonarr = Some(ApiKeyService { url, api_key });
    }
    if ask_service("Plex")? {
        let current = config.plex.take();
        let url = ask_url("Plex URL", current.as_ref().map(|c| c.url.as_str()), "http://plexbox.local:32400")?;
        let token = ask_secret("Plex token (X-Plex-Token)", current.as_ref().map(|c| c.token.as_str()))?;
        config.plex = Some(TokenService { url, token });
    }
    if ask_service("qBittorrent")? {
        let current = config.qbittorrent.take();
        config.qbittorrent = Some(ask_userpass("qBittorrent", current, "http://nas.local:8080")?);
    }
    if ask_service("NZBGet")? {
        let current = config.nzbget.take();
        config.nzbget = Some(ask_userpass("NZBGet", current, "http://nas.local:6789")?);
    }

    let path = config.save(config_path)?;
    println!("\nSaved {}", path.display());
    println!("Testing connectivity…\n");
    test_config(&config, false).await
}

fn ask_service(name: &str) -> Result<bool> {
    let answer = prompt(&format!("Configure {name}? [Y/n] "))?;
    Ok(answer.is_empty() || answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

fn ask_url(label: &str, current: Option<&str>, example: &str) -> Result<String> {
    let hint = current.unwrap_or(example);
    loop {
        let answer = prompt(&format!("  {label} [{hint}]: "))?;
        let value = if answer.is_empty() {
            match current {
                Some(c) => c.to_string(),
                None => example.to_string(),
            }
        } else {
            answer
        };
        if url::Url::parse(&value).is_ok() {
            return Ok(value);
        }
        eprintln!("  not a valid URL, try again (e.g. {example})");
    }
}

fn ask_secret(label: &str, current: Option<&str>) -> Result<String> {
    let hint = if current.is_some() { " [keep current]" } else { "" };
    loop {
        let answer = prompt(&format!("  {label}{hint}: "))?;
        if !answer.is_empty() {
            return Ok(answer);
        }
        if let Some(c) = current {
            return Ok(c.to_string());
        }
        eprintln!("  required");
    }
}

fn ask_userpass(name: &str, current: Option<UserPassService>, example_url: &str) -> Result<UserPassService> {
    let url = ask_url(&format!("{name} URL"), current.as_ref().map(|c| c.url.as_str()), example_url)?;
    let username = ask_secret(&format!("{name} username"), current.as_ref().map(|c| c.username.as_str()))?;
    let password = ask_secret(&format!("{name} password"), current.as_ref().map(|c| c.password.as_str()))?;
    Ok(UserPassService { url, username, password })
}

async fn test(config_path: Option<&Path>, json: bool) -> Result<()> {
    let config = Config::load(config_path)?;
    test_config(&config, json).await
}

#[derive(serde::Serialize)]
struct TestResult {
    service: &'static str,
    configured: bool,
    ok: bool,
    detail: String,
}

async fn test_config(config: &Config, json: bool) -> Result<()> {
    let clients = Clients::from_config(config);

    let (radarr, sonarr, plex, qbit, nzbget) = tokio::join!(
        async {
            match &clients.radarr {
                None => None,
                Some(c) => Some(c.system_status().await.map(|s| format!("Radarr {}", s.version))),
            }
        },
        async {
            match &clients.sonarr {
                None => None,
                Some(c) => Some(c.system_status().await.map(|s| format!("Sonarr {}", s.version))),
            }
        },
        async {
            match &clients.plex {
                None => None,
                Some(c) => Some(c.identity().await.map(|i| {
                    format!("Plex {} ({})", i.version, i.friendly_name.unwrap_or_default())
                })),
            }
        },
        async {
            match &clients.qbit {
                None => None,
                Some(c) => Some(
                    async {
                        c.login().await?;
                        let v = c.app_version().await?;
                        Ok(format!("qBittorrent {}", v.trim()))
                    }
                    .await,
                ),
            }
        },
        async {
            match &clients.nzbget {
                None => None,
                Some(c) => Some(c.version().await.map(|v| format!("NZBGet {v}"))),
            }
        },
    );

    let results: Vec<TestResult> = [
        ("radarr", radarr),
        ("sonarr", sonarr),
        ("plex", plex),
        ("qbittorrent", qbit),
        ("nzbget", nzbget),
    ]
    .into_iter()
    .map(|(service, outcome)| match outcome {
        None => TestResult {
            service,
            configured: false,
            ok: false,
            detail: "not configured".into(),
        },
        Some(Ok(detail)) => TestResult {
            service,
            configured: true,
            ok: true,
            detail,
        },
        Some(Err(e)) => TestResult {
            service,
            configured: true,
            ok: false,
            detail: e.to_string(),
        },
    })
    .collect();

    if json {
        return output::print_json(&results);
    }

    let mut any_fail = false;
    for r in &results {
        let mark = if !r.configured {
            "·"
        } else if r.ok {
            "✓"
        } else {
            any_fail = true;
            "✗"
        };
        println!("{mark} {:<12} {}", r.service, r.detail);
    }
    if any_fail {
        return Err(crate::error::CliarrError::Other(
            "one or more services failed the connectivity test".into(),
        ));
    }
    Ok(())
}

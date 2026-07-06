# cliarr

A fast terminal client for your home media stack: **Radarr**, **Sonarr**, **Plex**, **qBittorrent** and **NZBGet**, all over their LAN HTTP APIs. No SSH into the NAS required.

It's two tools in one binary:

- **Scriptable subcommands:** `cliarr movie add`, `cliarr queue`, `cliarr torrents pause ...` with `--json` output for piping.
- **A full TUI:** run `cliarr` with no arguments and you land in a live search box: just start typing and merged movie + series results appear as you type (`cliarr dune` even launches with the search already running). Dashboard, library browser, calendar, unified downloads and missing-items screens sit behind tabs `2`–`6`. Poster art renders inline in terminals that support it (kitty, iTerm2, sixel) with a unicode fallback everywhere else.

## Setup

```sh
cargo install --git https://github.com/ChrisThoma/cliarr
cliarr config init       # interactive wizard, tests each connection
cliarr config test       # re-check connectivity any time
```

(Or from a clone: `cargo install --path .`)

Configuration lives at `~/.config/cliarr/config.toml` (written `0600`). Every service is optional: configure only what you run, and each has its own URL so services can live on different machines:

```toml
[radarr]
url = "http://nas.local:7878"
api_key = "..."          # Radarr → Settings → General

[sonarr]
url = "http://nas.local:8989"
api_key = "..."

[plex]
url = "http://plexbox.local:32400"
token = "..."            # X-Plex-Token

[qbittorrent]
url = "http://nas.local:8080"
username = "admin"
password = "..."

[nzbget]
url = "http://nas.local:6789"
username = "nzbget"
password = "..."

[ui]
poster_protocol = "auto"   # auto | kitty | iterm2 | sixel | halfblocks | off
```

Reverse-proxy path prefixes (e.g. `http://nas.local/radarr`) are supported.

## CLI

```
cliarr config   init | show | test
cliarr movie    search <query> | add <tmdbId> [--profile P] [--root PATH] [--no-search]
                | list [--filter missing|monitored|unmonitored]
                | remove <id> [--delete-files] [--exclude]
                | search-missing | refresh [<id>]
cliarr series   ... (same shape; add takes a TVDB id)
cliarr queue    [--service radarr|sonarr|all]
cliarr queue    remove <id> --service radarr|sonarr [--blocklist] [--remove-from-client]
cliarr calendar [--days N]
cliarr missing  [--service radarr|sonarr|all]
cliarr torrents list [--filter downloading|...] | pause|resume|delete <hash...> [--delete-files]
cliarr nzb      list | pause|resume|delete <id...>
cliarr plex     status
```

Add `--json` to any list command for machine-readable output. Torrent hashes accept unambiguous prefixes (copy the short hash from `torrents list`). `movie add` / `series add` prompt for quality profile and root folder when run interactively, or list valid values when scripted.

Exit codes: `0` ok · `1` API/network error · `2` config or usage error.

## TUI

| Key | Action |
|-----|--------|
| *type* | on the Search screen, results appear live as you type (movies + series merged) |
| `1`–`6`, `Tab` | switch screen (Search · Dashboard · Library · Calendar · Downloads · Missing) |
| `j`/`k`, `↑`/`↓` | move selection (arrows work even while typing) |
| `Esc` | stop typing; `/` starts again |
| `m` / `s` | movies ↔ series (Library screen) |
| `a` / `Enter` | add selected search result (opens profile/root-folder picker) |
| `S` / `A` | search for release(s): selected / all missing |
| `p` / `P` | pause / resume torrent or NZB |
| `d` | delete (confirm dialog; `f` toggles delete-files) |
| `b` | blocklist an arr queue item |
| `r` | refresh current screen |
| `?` | help · `q` quit |

The Downloads screen unifies the Radarr/Sonarr queues with qBittorrent and NZBGet and auto-refreshes every 5 seconds. Works with qBittorrent 4.x and 5.x (the WebAPI rename of pause/stop is detected automatically).

## Development

```sh
cargo test      # wiremock-based API client tests, no live services needed
cargo clippy
```

Poster images are cached in `~/.cache/cliarr/posters/`.

## License

[MIT](LICENSE)

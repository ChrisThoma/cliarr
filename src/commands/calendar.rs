use chrono::{Duration, Local, NaiveDate};
use serde::Serialize;

use crate::api::Clients;
use crate::commands::output;
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEntry {
    pub date: String,
    pub service: &'static str,
    pub title: String,
    pub detail: String,
}

pub async fn run(days: i64, clients: &Clients, json: bool) -> Result<()> {
    let entries = fetch(days, clients).await?;
    if json {
        return output::print_json(&entries);
    }
    if entries.is_empty() {
        println!("nothing upcoming in the next {days} days");
        return Ok(());
    }
    let mut current_date = String::new();
    for e in &entries {
        if e.date != current_date {
            current_date = e.date.clone();
            println!("\n{current_date}");
        }
        println!("  [{}] {} {}", e.service, e.title, e.detail);
    }
    Ok(())
}

pub async fn fetch(days: i64, clients: &Clients) -> Result<Vec<CalendarEntry>> {
    let start = Local::now().date_naive();
    let end = start + Duration::days(days);
    let mut entries: Vec<CalendarEntry> = Vec::new();

    if let Some(radarr) = &clients.radarr {
        for m in radarr.calendar(start, end).await? {
            // A movie can have several release dates in range; prefer the earliest in-window one.
            let date = [&m.digital_release, &m.physical_release, &m.in_cinemas]
                .into_iter()
                .flatten()
                .filter_map(|d| parse_date(d))
                .filter(|d| *d >= start && *d <= end)
                .min();
            if let Some(date) = date {
                entries.push(CalendarEntry {
                    date: date.to_string(),
                    service: "radarr",
                    title: m.title.clone(),
                    detail: m.year.map(|y| format!("({y})")).unwrap_or_default(),
                });
            }
        }
    }
    if let Some(sonarr) = &clients.sonarr {
        for e in sonarr.calendar(start, end).await? {
            let date = e
                .air_date_utc
                .as_deref()
                .and_then(parse_local_date)
                .map(|d| d.to_string())
                .unwrap_or_default();
            entries.push(CalendarEntry {
                date,
                service: "sonarr",
                title: e.series_title().to_string(),
                detail: format!("{} — {}", e.code(), e.title.clone().unwrap_or_default()),
            });
        }
    }

    entries.sort_by(|a, b| a.date.cmp(&b.date).then(a.title.cmp(&b.title)));
    Ok(entries)
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    // Dates arrive as either "2026-07-06" or full RFC3339 timestamps.
    s.get(..10).and_then(|d| d.parse().ok())
}

/// Air times are instants (e.g. "2026-07-07T01:00:00Z" for a 9pm ET airing);
/// convert to the user's timezone so the episode lands on the right day.
/// Radarr release dates stay on `parse_date`: they are calendar dates encoded
/// as UTC midnight, which local conversion would shift to the previous day.
fn parse_local_date(s: &str) -> Option<NaiveDate> {
    match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(dt) => Some(dt.with_timezone(&Local).date_naive()),
        Err(_) => parse_date(s),
    }
}

use serde::Serialize;

use crate::api::models::arr::QueueItem;
use crate::api::Clients;
use crate::cli::{ArrService, ArrServiceOne, QueueCmd};
use crate::commands::output;
use crate::error::Result;

#[derive(Serialize)]
struct TaggedQueueItem {
    service: &'static str,
    #[serde(flatten)]
    item: QueueItem,
}

pub async fn run(cmd: Option<QueueCmd>, service: ArrService, clients: &Clients, json: bool) -> Result<()> {
    match cmd.unwrap_or(QueueCmd::List) {
        QueueCmd::List => list(service, clients, json).await,
        QueueCmd::Remove {
            id,
            service,
            blocklist,
            remove_from_client,
        } => {
            match service {
                ArrServiceOne::Radarr => {
                    clients.radarr()?.queue_delete(id, blocklist, remove_from_client).await?
                }
                ArrServiceOne::Sonarr => {
                    clients.sonarr()?.queue_delete(id, blocklist, remove_from_client).await?
                }
            }
            crate::outln!(
                "✓ Removed queue item {id}{}",
                if blocklist { " (blocklisted)" } else { "" }
            );
            Ok(())
        }
    }
}

async fn list(service: ArrService, clients: &Clients, json: bool) -> Result<()> {
    let mut items: Vec<TaggedQueueItem> = Vec::new();

    if matches!(service, ArrService::Radarr | ArrService::All) {
        if let Some(radarr) = &clients.radarr {
            for item in radarr.queue().await? {
                items.push(TaggedQueueItem { service: "radarr", item });
            }
        } else if service == ArrService::Radarr {
            clients.radarr()?;
        }
    }
    if matches!(service, ArrService::Sonarr | ArrService::All) {
        if let Some(sonarr) = &clients.sonarr {
            for item in sonarr.queue().await? {
                items.push(TaggedQueueItem { service: "sonarr", item });
            }
        } else if service == ArrService::Sonarr {
            clients.sonarr()?;
        }
    }

    if json {
        return output::print_json(&items);
    }
    if items.is_empty() {
        crate::outln!("queue is empty");
        return Ok(());
    }
    let mut t = output::table(&["id", "service", "title", "status", "progress", "time left"]);
    for TaggedQueueItem { service, item } in &items {
        let progress = item
            .progress()
            .map(|p| format!("{p:.0}%"))
            .unwrap_or_default();
        t.add_row(vec![
            output::right(item.id),
            comfy_table::Cell::new(*service),
            comfy_table::Cell::new(item.title.clone().unwrap_or_default()),
            comfy_table::Cell::new(item.status.clone().unwrap_or_default()),
            output::right(progress),
            comfy_table::Cell::new(item.timeleft.clone().unwrap_or_default()),
        ]);
    }
    crate::outln!("{t}");
    Ok(())
}

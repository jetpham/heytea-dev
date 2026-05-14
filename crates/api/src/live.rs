use crate::db;
use heytea_core::StatusResponse;
use sqlx::postgres::PgListener;
use std::time::Duration;
use tokio::sync::broadcast;

const STATUS_CHANNEL: &str = "heytea_status_updated";

pub fn spawn_status_listener(
    database_url: String,
    pool: sqlx::PgPool,
    sender: broadcast::Sender<StatusResponse>,
) {
    tokio::spawn(async move {
        loop {
            if let Err(error) = listen_once(&database_url, &pool, &sender).await {
                tracing::error!(?error, "status notification listener failed");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    });
}

async fn listen_once(
    database_url: &str,
    pool: &sqlx::PgPool,
    sender: &broadcast::Sender<StatusResponse>,
) -> anyhow::Result<()> {
    let mut listener = PgListener::connect(database_url).await?;
    listener.listen(STATUS_CHANNEL).await?;
    tracing::info!(
        channel = STATUS_CHANNEL,
        "listening for status notifications"
    );

    loop {
        let notification = listener.recv().await?;
        tracing::debug!(
            channel = notification.channel(),
            payload = notification.payload(),
            "received status notification"
        );

        match db::status(pool).await {
            Ok(status) => {
                let _ = sender.send(status);
            }
            Err(error) => tracing::warn!(?error, "status notification could not load status"),
        }
    }
}

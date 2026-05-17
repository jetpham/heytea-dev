use crate::db;
use sqlx::postgres::PgListener;
use std::time::Duration;
use tokio::sync::broadcast;

const STATUS_CHANNEL: &str = "heytea_status_updated";

pub fn spawn_status_listener(
    database_url: String,
    pool: sqlx::PgPool,
    sender: broadcast::Sender<()>,
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
    sender: &broadcast::Sender<()>,
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

        if db::schema_ready(pool).await {
            let _ = sender.send(());
        } else {
            tracing::warn!("status notification received before schema was ready");
        }
    }
}

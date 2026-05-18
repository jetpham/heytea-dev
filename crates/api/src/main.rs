mod db;
mod error;
mod live;
mod openapi;
mod routes;

use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::{env, net::SocketAddr, time::Duration};
use tokio::sync::broadcast;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub status_events: broadcast::Sender<()>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://heytea:heytea@localhost:5432/heytea".to_string());
    let bind = env::var("HEYTEA_API_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let max_connections = env_u32("HEYTEA_API_MAX_CONNECTIONS", 10);
    let addr: SocketAddr = bind.parse()?;

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;

    let (status_events, _) = broadcast::channel(128);
    live::spawn_status_listener(database_url, pool.clone(), status_events.clone());

    let app = app(AppState {
        pool,
        status_events,
    });
    tracing::info!(%addr, "starting heytea api");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn app(state: AppState) -> Router {
    routes::router(state)
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

mod assets;
mod error;
mod routes;
mod templates;

use crate::assets::Assets;
use axum::Router;
use std::{env, net::SocketAddr, path::PathBuf, time::Duration};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Clone)]
pub struct AppState {
    pub api_url: String,
    pub mcp_url: String,
    pub public_api_url: String,
    pub client: reqwest::Client,
    pub assets: Assets,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let bind = env::var("HEYTEA_SITE_BIND").unwrap_or_else(|_| "127.0.0.1:3100".to_string());
    let addr: SocketAddr = bind.parse()?;
    let state = AppState {
        api_url: env::var("HEYTEA_API_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string()),
        mcp_url: env::var("HEYTEA_MCP_URL").unwrap_or_else(|_| "http://127.0.0.1:3001".to_string()),
        public_api_url: env::var("HEYTEA_PUBLIC_API_URL")
            .unwrap_or_else(|_| "https://api.heytea.dev".to_string()),
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(4))
            .build()?,
        assets: Assets::load(asset_root()),
    };

    let app = app(state);
    tracing::info!(%addr, "starting heytea site");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn app(state: AppState) -> Router {
    routes::router(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

fn asset_root() -> PathBuf {
    env::var_os("HEYTEA_SITE_ASSETS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("apps/assets/dist")
        })
}

use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, net::SocketAddr};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Clone)]
struct AppState {
    client: heytea::Client,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(rename = "jsonrpc")]
    _jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let api_url =
        env::var("HEYTEA_API_URL").unwrap_or_else(|_| "https://api.heytea.dev".to_string());
    let bind = env::var("HEYTEA_MCP_BIND").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
    let addr: SocketAddr = bind.parse()?;
    let state = AppState {
        client: heytea::Client::new(api_url)?,
    };

    let app = Router::new()
        .route("/", post(handle_rpc))
        .route("/mcp", post(handle_rpc))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    tracing::info!(%addr, "starting heytea mcp server");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_rpc(
    State(state): State<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone();
    let result = dispatch(&state, &request).await;
    match result {
        Ok(result) => Json(JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }),
        Err(error) => Json(JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(json!({ "code": -32000, "message": error.to_string() })),
        }),
    }
}

async fn dispatch(state: &AppState, request: &JsonRpcRequest) -> anyhow::Result<Value> {
    match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": { "name": "heytea.dev", "version": "0.1.0" },
            "capabilities": { "tools": {} }
        })),
        "tools/list" => Ok(json!({
            "tools": [
                { "name": "get_status", "description": "Get current status for the configured HeyTea Downtown Metreon location", "inputSchema": { "type": "object", "properties": {} } },
                { "name": "get_wait_time", "description": "Get current wait time", "inputSchema": { "type": "object", "properties": {} } },
                { "name": "get_notice", "description": "Get current store notice", "inputSchema": { "type": "object", "properties": {} } },
                { "name": "get_closing_notice", "description": "Get current closing notice", "inputSchema": { "type": "object", "properties": {} } },
                { "name": "get_history", "description": "Get bucketed historical wait-time data", "inputSchema": { "type": "object", "properties": { "range": { "type": "string" }, "bucket": { "type": "string" } } } }
            ]
        })),
        "tools/call" => {
            let params = request.params.clone().unwrap_or_default();
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = params.get("arguments").cloned().unwrap_or_default();
            let value = match name {
                "get_status" => serde_json::to_value(state.client.status().await?)?,
                "get_wait_time" => serde_json::to_value(state.client.wait_time().await?)?,
                "get_notice" => serde_json::to_value(state.client.notice().await?)?,
                "get_closing_notice" => serde_json::to_value(state.client.closing_notice().await?)?,
                "get_history" => {
                    let range = arguments
                        .get("range")
                        .and_then(Value::as_str)
                        .unwrap_or("24h");
                    let bucket = arguments.get("bucket").and_then(Value::as_str);
                    serde_json::to_value(state.client.history(range, bucket).await?)?
                }
                other => anyhow::bail!("unknown tool: {other}"),
            };
            Ok(
                json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value)? }] }),
            )
        }
        other => anyhow::bail!("unsupported method: {other}"),
    }
}

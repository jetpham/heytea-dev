use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, net::SocketAddr};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Clone)]
struct AppState {
    client: heytea::Client,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: Option<String>,
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

#[derive(Debug)]
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: message.into(),
        }
    }

    fn method_not_found(message: impl Into<String>) -> Self {
        Self {
            code: -32601,
            message: message.into(),
        }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        tracing::warn!(error = %error, "mcp tool call failed");
        Self {
            code: -32000,
            message: error.to_string(),
        }
    }
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
        .route("/", get(mcp_info).post(handle_rpc))
        .route("/mcp", get(mcp_info).post(handle_rpc))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    tracing::info!(%addr, "starting heytea mcp server");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn mcp_info() -> Json<Value> {
    Json(json!({
        "name": "heytea.dev",
        "description": "Public anonymous MCP tools for the HeyTea Downtown Metreon live status API.",
        "transport": "streamable-http",
        "endpoint": "/mcp"
    }))
}

async fn handle_rpc(
    State(state): State<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone();
    let result = if request.jsonrpc.as_deref() == Some("2.0") {
        dispatch(&state, &request).await
    } else {
        Err(RpcError::invalid_request("jsonrpc must be \"2.0\""))
    };
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
            error: Some(json!({ "code": error.code, "message": error.message })),
        }),
    }
}

async fn dispatch(state: &AppState, request: &JsonRpcRequest) -> Result<Value, RpcError> {
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
                { "name": "get_history", "description": "Get one-minute historical wait-time data", "inputSchema": { "type": "object", "properties": { "range": { "type": "string", "enum": ["today", "1h", "6h", "24h", "7d", "30d", "1y"], "default": "today" } } } }
            ]
        })),
        "tools/call" => {
            let params = request
                .params
                .as_ref()
                .and_then(Value::as_object)
                .ok_or_else(|| RpcError::invalid_params("params must be an object"))?;
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("params.name must be a string"))?;
            let arguments = params.get("arguments").unwrap_or(&Value::Null);
            if !arguments.is_null() && !arguments.is_object() {
                return Err(RpcError::invalid_params(
                    "params.arguments must be an object",
                ));
            }
            let value = match name {
                "get_status" => {
                    serde_json::to_value(state.client.status().await.map_err(RpcError::internal)?)
                        .map_err(RpcError::internal)?
                }
                "get_wait_time" => serde_json::to_value(
                    state.client.wait_time().await.map_err(RpcError::internal)?,
                )
                .map_err(RpcError::internal)?,
                "get_notice" => {
                    serde_json::to_value(state.client.notice().await.map_err(RpcError::internal)?)
                        .map_err(RpcError::internal)?
                }
                "get_closing_notice" => serde_json::to_value(
                    state
                        .client
                        .closing_notice()
                        .await
                        .map_err(RpcError::internal)?,
                )
                .map_err(RpcError::internal)?,
                "get_history" => {
                    let range = arguments
                        .get("range")
                        .and_then(Value::as_str)
                        .unwrap_or("today");
                    if !matches!(range, "today" | "1h" | "6h" | "24h" | "7d" | "30d" | "1y") {
                        return Err(RpcError::invalid_params("unsupported history range"));
                    }
                    serde_json::to_value(
                        state
                            .client
                            .history(range)
                            .await
                            .map_err(RpcError::internal)?,
                    )
                    .map_err(RpcError::internal)?
                }
                other => return Err(RpcError::method_not_found(format!("unknown tool: {other}"))),
            };
            Ok(
                json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).map_err(RpcError::internal)? }] }),
            )
        }
        other => Err(RpcError::method_not_found(format!(
            "unsupported method: {other}"
        ))),
    }
}

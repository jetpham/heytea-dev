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

fn slug_argument(arguments: &Value) -> Result<&str, RpcError> {
    arguments
        .get("slug")
        .and_then(Value::as_str)
        .filter(|slug| !slug.is_empty())
        .ok_or_else(|| RpcError::invalid_params("arguments.slug must be a non-empty string"))
}

fn number_argument(arguments: &Value, name: &str) -> Result<f64, RpcError> {
    arguments
        .get(name)
        .and_then(Value::as_f64)
        .ok_or_else(|| RpcError::invalid_params(format!("arguments.{name} must be a number")))
}

fn distance_miles(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let earth_radius_miles = 3958.8_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    earth_radius_miles * c
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
        "description": "Public anonymous MCP tools for HeyTea locations, wait times, and notices.",
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
                { "name": "list_locations", "description": "List public HeyTea locations with current open state and pickup wait", "inputSchema": { "type": "object", "properties": {} } },
                { "name": "find_nearest_location", "description": "Find the nearest public HeyTea location to latitude and longitude", "inputSchema": { "type": "object", "required": ["latitude", "longitude"], "properties": { "latitude": { "type": "number" }, "longitude": { "type": "number" } } } },
                { "name": "get_status", "description": "Get current status for a HeyTea location slug", "inputSchema": { "type": "object", "required": ["slug"], "properties": { "slug": { "type": "string" } } } },
                { "name": "get_wait_time", "description": "Get current wait time for a HeyTea location slug", "inputSchema": { "type": "object", "required": ["slug"], "properties": { "slug": { "type": "string" } } } },
                { "name": "get_notice", "description": "Get current store notice for a HeyTea location slug", "inputSchema": { "type": "object", "required": ["slug"], "properties": { "slug": { "type": "string" } } } },
                { "name": "get_closing_notice", "description": "Get current closing notice for a HeyTea location slug", "inputSchema": { "type": "object", "required": ["slug"], "properties": { "slug": { "type": "string" } } } },
                { "name": "get_history", "description": "Get one-minute historical wait-time data for a HeyTea location slug", "inputSchema": { "type": "object", "required": ["slug"], "properties": { "slug": { "type": "string" }, "range": { "type": "string", "enum": ["today", "1h", "6h", "24h", "7d"], "default": "today" } } } }
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
                "list_locations" => serde_json::to_value(
                    state.client.locations().await.map_err(RpcError::internal)?,
                )
                .map_err(RpcError::internal)?,
                "find_nearest_location" => {
                    let latitude = number_argument(arguments, "latitude")?;
                    let longitude = number_argument(arguments, "longitude")?;
                    let locations = state.client.locations().await.map_err(RpcError::internal)?;
                    let nearest = locations
                        .locations
                        .into_iter()
                        .filter_map(|location| {
                            let lat = location.latitude?;
                            let lon = location.longitude?;
                            Some((distance_miles(latitude, longitude, lat, lon), location))
                        })
                        .min_by(|left, right| left.0.total_cmp(&right.0));
                    serde_json::to_value(nearest.map(|(distance_miles, location)| {
                        json!({ "distanceMiles": distance_miles, "location": location })
                    }))
                    .map_err(RpcError::internal)?
                }
                "get_status" => {
                    let slug = slug_argument(arguments)?;
                    serde_json::to_value(
                        state
                            .client
                            .status_for_location(slug)
                            .await
                            .map_err(RpcError::internal)?,
                    )
                    .map_err(RpcError::internal)?
                }
                "get_wait_time" => {
                    let slug = slug_argument(arguments)?;
                    serde_json::to_value(
                        state
                            .client
                            .wait_time_for_location(slug)
                            .await
                            .map_err(RpcError::internal)?,
                    )
                    .map_err(RpcError::internal)?
                }
                "get_notice" => {
                    let slug = slug_argument(arguments)?;
                    serde_json::to_value(
                        state
                            .client
                            .notice_for_location(slug)
                            .await
                            .map_err(RpcError::internal)?,
                    )
                    .map_err(RpcError::internal)?
                }
                "get_closing_notice" => {
                    let slug = slug_argument(arguments)?;
                    serde_json::to_value(
                        state
                            .client
                            .closing_notice_for_location(slug)
                            .await
                            .map_err(RpcError::internal)?,
                    )
                    .map_err(RpcError::internal)?
                }
                "get_history" => {
                    let slug = slug_argument(arguments)?;
                    let range = arguments
                        .get("range")
                        .and_then(Value::as_str)
                        .unwrap_or("today");
                    if !matches!(range, "today" | "1h" | "6h" | "24h" | "7d") {
                        return Err(RpcError::invalid_params("unsupported history range"));
                    }
                    serde_json::to_value(
                        state
                            .client
                            .history_for_location(slug, range)
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

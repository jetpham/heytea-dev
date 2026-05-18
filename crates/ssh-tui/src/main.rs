use anyhow::Context;
use chrono::{DateTime, Utc};
use russh::keys::{load_secret_key, PublicKey};
use russh::server::{Auth, Config, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, Pty};
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const DEFAULT_BIND: &str = "127.0.0.1:2222";
const DEFAULT_HOST_KEY: &str = "/var/lib/heytea-ssh-tui/ssh_host_ed25519_key";
const DEFAULT_MAX_SESSIONS: usize = 8;
const DEFAULT_REFRESH_SECONDS: u64 = 15;

#[derive(Clone)]
struct AppServer {
    shared: Arc<Shared>,
}

struct Shared {
    client: heytea::Client,
    sessions: Mutex<HashMap<usize, ClientSession>>,
    semaphore: Arc<Semaphore>,
    next_id: AtomicUsize,
    refresh: Duration,
}

struct ClientSession {
    channel: ChannelId,
    handle: russh::server::Handle,
    cols: u16,
    rows: u16,
    query: String,
    selected: usize,
    _permit: OwnedSemaphorePermit,
}

struct HandlerState {
    shared: Arc<Shared>,
    id: usize,
}

struct SessionSnapshot {
    channel: ChannelId,
    handle: russh::server::Handle,
    cols: u16,
    rows: u16,
    query: String,
    selected: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let api_url = env::var("HEYTEA_API_URL").unwrap_or_else(|_| "https://api.heytea.dev".into());
    let bind = env::var("HEYTEA_SSH_TUI_BIND").unwrap_or_else(|_| DEFAULT_BIND.into());
    let host_key = env::var_os("HEYTEA_SSH_TUI_HOST_KEY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_HOST_KEY));
    let max_sessions = env_usize("HEYTEA_SSH_TUI_MAX_SESSIONS", DEFAULT_MAX_SESSIONS);
    let refresh = Duration::from_secs(env_u64(
        "HEYTEA_SSH_TUI_REFRESH_SECONDS",
        DEFAULT_REFRESH_SECONDS,
    ));

    let key = load_secret_key(&host_key, None)
        .with_context(|| format!("failed to load SSH host key at {}", host_key.display()))?;
    let config = Arc::new(Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        auth_rejection_time: Duration::from_millis(100),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![key],
        nodelay: true,
        ..Default::default()
    });

    let shared = Arc::new(Shared {
        client: heytea::Client::new(api_url)?,
        sessions: Mutex::new(HashMap::new()),
        semaphore: Arc::new(Semaphore::new(max_sessions)),
        next_id: AtomicUsize::new(1),
        refresh,
    });
    spawn_refresh_loop(shared.clone());

    let addresses = bind
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.parse::<SocketAddr>())
        .collect::<Result<Vec<_>, _>>()?;
    if addresses.is_empty() {
        anyhow::bail!("HEYTEA_SSH_TUI_BIND did not contain any listen addresses");
    }

    let mut tasks = Vec::new();
    for address in addresses {
        let mut server = AppServer {
            shared: shared.clone(),
        };
        let config = config.clone();
        tracing::info!(%address, max_sessions, "starting heytea ssh tui");
        tasks.push(tokio::spawn(async move {
            server.run_on_address(config, address).await
        }));
    }

    for task in tasks {
        task.await??;
    }
    Ok(())
}

impl Server for AppServer {
    type Handler = HandlerState;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Self::Handler {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        HandlerState {
            shared: self.shared.clone(),
            id,
        }
    }
}

impl Handler for HandlerState {
    type Error = anyhow::Error;

    async fn auth_none(&mut self, _: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, _: &str, _: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::UnsupportedMethod)
    }

    async fn auth_publickey(&mut self, _: &str, _: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(Auth::UnsupportedMethod)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let Ok(permit) = self.shared.semaphore.clone().try_acquire_owned() else {
            tracing::warn!(session_id = self.id, "rejecting SSH TUI session over limit");
            return Ok(false);
        };
        let session = ClientSession {
            channel: channel.id(),
            handle: session.handle(),
            cols: 80,
            rows: 24,
            query: String::new(),
            selected: 0,
            _permit: permit,
        };
        self.shared.sessions.lock().await.insert(self.id, session);
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        update_size(&self.shared, self.id, col_width, row_height).await;
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        render_session(&self.shared, self.id).await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        render_session(&self.shared, self.id).await;
        self.shared.sessions.lock().await.remove(&self.id);
        session.close(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if handle_input(&self.shared, self.id, data).await {
            let _ = session
                .handle()
                .data(channel, b"\x1b[?25h\r\nbye\r\n".to_vec())
                .await;
            self.shared.sessions.lock().await.remove(&self.id);
            session.close(channel)?;
            return Ok(());
        }
        render_session(&self.shared, self.id).await;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _: ChannelId,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        update_size(&self.shared, self.id, col_width, row_height).await;
        render_session(&self.shared, self.id).await;
        Ok(())
    }

    async fn channel_close(&mut self, _: ChannelId, _: &mut Session) -> Result<(), Self::Error> {
        self.shared.sessions.lock().await.remove(&self.id);
        Ok(())
    }
}

impl Drop for HandlerState {
    fn drop(&mut self) {
        let shared = self.shared.clone();
        let id = self.id;
        tokio::spawn(async move {
            shared.sessions.lock().await.remove(&id);
        });
    }
}

fn spawn_refresh_loop(shared: Arc<Shared>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(shared.refresh).await;
            let ids = shared
                .sessions
                .lock()
                .await
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for id in ids {
                render_session(&shared, id).await;
            }
        }
    });
}

async fn update_size(shared: &Shared, id: usize, cols: u32, rows: u32) {
    if let Some(session) = shared.sessions.lock().await.get_mut(&id) {
        session.cols = cols.clamp(40, 240) as u16;
        session.rows = rows.clamp(12, 80) as u16;
    }
}

async fn handle_input(shared: &Shared, id: usize, data: &[u8]) -> bool {
    let mut sessions = shared.sessions.lock().await;
    let Some(session) = sessions.get_mut(&id) else {
        return false;
    };

    match data {
        b"q" | b"Q" | b"\x03" | b"\x04" => return true,
        b"\x1b[A" => session.selected = session.selected.saturating_sub(1),
        b"\x1b[B" => session.selected = session.selected.saturating_add(1),
        b"\x15" => {
            session.query.clear();
            session.selected = 0;
        }
        b"\x7f" | b"\x08" => {
            session.query.pop();
            session.selected = 0;
        }
        _ => {
            for byte in data {
                if (0x20..=0x7e).contains(byte) && session.query.len() < 64 {
                    session.query.push(*byte as char);
                    session.selected = 0;
                }
            }
        }
    }
    false
}

async fn render_session(shared: &Shared, id: usize) {
    let Some(snapshot) = snapshot(shared, id).await else {
        return;
    };
    let body = match render_body(shared, &snapshot).await {
        Ok(body) => body,
        Err(error) => render_error(&snapshot, &error.to_string()),
    };
    if snapshot
        .handle
        .data(snapshot.channel, body.into_bytes())
        .await
        .is_err()
    {
        shared.sessions.lock().await.remove(&id);
    }
}

async fn snapshot(shared: &Shared, id: usize) -> Option<SessionSnapshot> {
    let sessions = shared.sessions.lock().await;
    let session = sessions.get(&id)?;
    Some(SessionSnapshot {
        channel: session.channel,
        handle: session.handle.clone(),
        cols: session.cols,
        rows: session.rows,
        query: session.query.clone(),
        selected: session.selected,
    })
}

async fn render_body(shared: &Shared, snapshot: &SessionSnapshot) -> anyhow::Result<String> {
    let locations = shared.client.locations().await?;
    let mut matches = locations
        .locations
        .iter()
        .filter(|location| matches_query(location, &snapshot.query))
        .collect::<Vec<_>>();
    matches.sort_by_key(|location| location.name.to_ascii_lowercase());
    let selected = snapshot.selected.min(matches.len().saturating_sub(1));
    let selected_location = matches.get(selected).copied();
    let status = if let Some(location) = selected_location {
        shared.client.status_for_location(&location.slug).await.ok()
    } else {
        None
    };
    let history = if let Some(location) = selected_location {
        shared
            .client
            .history_for_location(&location.slug, "today")
            .await
            .ok()
    } else {
        None
    };

    let rows = snapshot.rows.max(12) as usize;
    let cols = snapshot.cols.max(40) as usize;
    let mut out = String::new();
    out.push_str("\x1b[?25l\x1b[2J\x1b[H");
    push_line(&mut out, cols, "heytea.dev", true);
    push_line(
        &mut out,
        cols,
        "anonymous readonly ssh tui - type to search, arrows move, q quits",
        false,
    );
    push_line(&mut out, cols, "", false);
    push_line(
        &mut out,
        cols,
        &format!("search: {}", empty_marker(&snapshot.query)),
        false,
    );

    if let Some(location) = selected_location {
        push_line(&mut out, cols, "", false);
        push_line(&mut out, cols, &location.name.to_ascii_lowercase(), true);
        push_line(&mut out, cols, &location.address, false);
        if let Some(status) = status.as_ref() {
            push_line(&mut out, cols, &status_line(status), false);
            push_line(
                &mut out,
                cols,
                &format!("observed: {}", format_time(status.observed_at)),
                false,
            );
            if !status.notices.is_empty() {
                push_line(
                    &mut out,
                    cols,
                    &format!("notice: {}", status.notices.join(" / ")),
                    false,
                );
            }
            if !status.closing_notices.is_empty() {
                push_line(
                    &mut out,
                    cols,
                    &format!("closing: {}", status.closing_notices.join(" / ")),
                    false,
                );
            }
        } else {
            push_line(&mut out, cols, "status: not ready", false);
        }
        if let Some(history) = history.as_ref() {
            push_line(
                &mut out,
                cols,
                &format!("today: {}", sparkline(history, cols.saturating_sub(8))),
                false,
            );
        }
    } else {
        push_line(&mut out, cols, "", false);
        push_line(&mut out, cols, "no matching locations", true);
    }

    push_line(&mut out, cols, "", false);
    let used_rows = out.matches("\r\n").count();
    let list_rows = rows.saturating_sub(used_rows + 2).max(3);
    for (index, location) in matches.iter().take(list_rows).enumerate() {
        let prefix = if index == selected { ">" } else { " " };
        let wait = location
            .pickup_wait_minutes
            .map(|value| format!("{value}m"))
            .unwrap_or_else(|| "--".into());
        push_line(
            &mut out,
            cols,
            &format!(
                "{prefix} {:<28} {:>4}  {}",
                location.name, wait, location.slug
            ),
            index == selected,
        );
    }
    if matches.len() > list_rows {
        push_line(
            &mut out,
            cols,
            &format!("... {} more", matches.len() - list_rows),
            false,
        );
    }
    push_line(&mut out, cols, "", false);
    push_line(&mut out, cols, "https://heytea.dev", false);
    Ok(out)
}

fn render_error(snapshot: &SessionSnapshot, message: &str) -> String {
    let mut out = String::new();
    out.push_str("\x1b[?25l\x1b[2J\x1b[H");
    push_line(&mut out, snapshot.cols as usize, "heytea.dev", true);
    push_line(
        &mut out,
        snapshot.cols as usize,
        "service is warming up",
        false,
    );
    push_line(&mut out, snapshot.cols as usize, message, false);
    push_line(&mut out, snapshot.cols as usize, "press q to quit", false);
    out
}

fn matches_query(location: &heytea::LocationResponse, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let query = query.to_ascii_lowercase();
    location.name.to_ascii_lowercase().contains(&query)
        || location.slug.to_ascii_lowercase().contains(&query)
        || location.address.to_ascii_lowercase().contains(&query)
}

fn status_line(status: &heytea::StatusResponse) -> String {
    if status.is_open == Some(false) {
        return "status: closed".into();
    }
    match status.pickup_wait_minutes {
        Some(wait) => format!("status: open, pickup wait {wait} minutes"),
        None => "status: open, wait not available".into(),
    }
}

fn sparkline(history: &heytea::HistoryResponse, width: usize) -> String {
    let values = history
        .points
        .iter()
        .filter_map(|point| point.avg_pickup_wait_minutes)
        .map(|value| value.round().max(0.0) as usize)
        .collect::<Vec<_>>();
    if values.is_empty() || width == 0 {
        return "no data".into();
    }
    let start = values.len().saturating_sub(width);
    let values = &values[start..];
    let max = values.iter().copied().max().unwrap_or(1).max(1);
    let steps = b" .:-=+*#%@";
    values
        .iter()
        .map(|value| {
            let index = value * (steps.len() - 1) / max;
            steps[index] as char
        })
        .collect()
}

fn push_line(out: &mut String, cols: usize, value: &str, bold: bool) {
    if bold {
        out.push_str("\x1b[1m");
    }
    out.push_str(&truncate(value, cols));
    if bold {
        out.push_str("\x1b[0m");
    }
    out.push_str("\r\n");
}

fn truncate(value: &str, cols: usize) -> String {
    let mut output = String::new();
    for ch in value.chars().take(cols.saturating_sub(1)) {
        output.push(ch);
    }
    output
}

fn empty_marker(value: &str) -> &str {
    if value.is_empty() {
        "type a city, store, or slug"
    } else {
        value
    }
}

fn format_time(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

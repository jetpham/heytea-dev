use crate::{assets::AssetTags, routes::ApiCheck};
use askama::Template;
use chrono::{DateTime, Utc};
use chrono_tz::America::Los_Angeles;
use heytea_core::{HistoryResponse, ReadyResponse, StatusResponse};
use std::fmt::Write as _;

const DASHBOARD_CSS: &str = include_str!("../templates/dashboard.css");
const DASHBOARD_JS: &str = include_str!("../templates/dashboard.js");
const DASHBOARD_FONT_B64: &str = include_str!("../templates/atkinson-ascii.woff2.b64");
const FAVICON_SVG: &str = include_str!("../templates/heyteafavi.svg");

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub stream_url: String,
    pub favicon_href: String,
    pub inline_css: String,
    pub inline_js: &'static str,
    pub pickup_wait: String,
    pub making_cups: String,
    pub making_orders: String,
    pub observed: String,
    pub open: String,
    pub closing_notice: String,
    pub trend_points: String,
    pub trend_data: String,
    pub trend_minute: i64,
}

impl DashboardTemplate {
    pub fn new(
        status: Option<StatusResponse>,
        history: Option<HistoryResponse>,
        stream_url: String,
    ) -> Self {
        let values = history
            .as_ref()
            .map(|history| {
                history
                    .points
                    .iter()
                    .filter_map(|point| point.avg_pickup_wait_minutes)
                    .map(|value| value.round() as i32)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let trend_max = nice_axis(values.iter().copied().max().unwrap_or(0));
        let trend_points = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let x = if values.len() < 2 {
                    100.0
                } else {
                    index as f64 * 100.0 / (values.len() - 1) as f64
                };
                let y = 38.0 - *value as f64 / trend_max as f64 * 30.0;
                format!("{x:.1},{y:.1}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        let trend_data = values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let trend_minute = status
            .as_ref()
            .map(|status| status.observed_at.timestamp() / 60)
            .unwrap_or_default();

        Self {
            stream_url,
            favicon_href: svg_data_uri(FAVICON_SVG),
            inline_css: format!(
                "@font-face{{font-family:a;src:url(data:font/woff2;base64,{}) format('woff2');font-display:block}}{}",
                DASHBOARD_FONT_B64.trim(),
                DASHBOARD_CSS
            ),
            inline_js: DASHBOARD_JS,
            pickup_wait: minutes(
                status
                    .as_ref()
                    .and_then(|status| status.pickup_wait_minutes),
            ),
            making_cups: count(status.as_ref().and_then(|status| status.making_cups)),
            making_orders: count(status.as_ref().and_then(|status| status.making_orders)),
            observed: status
                .as_ref()
                .map(|status| time(status.observed_at))
                .unwrap_or_else(|| "after first poll".to_string()),
            open: match status.as_ref().and_then(|status| status.is_open) {
                Some(true) => "yes".to_string(),
                Some(false) => "no".to_string(),
                None => "unknown".to_string(),
            },
            closing_notice: clip(
                status
                    .as_ref()
                    .and_then(|status| status.closing_notice.as_deref()),
                "No active closing notice",
            ),
            trend_points,
            trend_data,
            trend_minute,
        }
    }
}

#[derive(Template)]
#[template(path = "status.html")]
pub struct StatusTemplate {
    pub assets: AssetTags,
    pub operational: bool,
    pub summary: String,
    pub database: String,
    pub freshness: String,
    pub store_open: String,
    pub pickup_wait: String,
    pub checked: String,
}

impl StatusTemplate {
    pub fn new(
        ready: ApiCheck<ReadyResponse>,
        status: ApiCheck<StatusResponse>,
        assets: AssetTags,
    ) -> Self {
        let fresh = status.ok
            && status
                .value
                .as_ref()
                .map(|status| !status.stale)
                .unwrap_or(false);
        let operational =
            ready.ok && ready.value.as_ref().map(|ready| ready.ok).unwrap_or(false) && fresh;

        Self {
            assets,
            operational,
            summary: if operational {
                "Operational"
            } else {
                "Degraded"
            }
            .to_string(),
            database: if ready
                .value
                .as_ref()
                .map(|ready| ready.database)
                .unwrap_or(false)
            {
                "ready"
            } else {
                "not ready"
            }
            .to_string(),
            freshness: if fresh {
                "fresh"
            } else {
                "stale or unavailable"
            }
            .to_string(),
            store_open: match status.value.as_ref().and_then(|status| status.is_open) {
                Some(true) => "yes".to_string(),
                Some(false) => "no".to_string(),
                None => "unknown".to_string(),
            },
            pickup_wait: minutes(
                status
                    .value
                    .as_ref()
                    .and_then(|status| status.pickup_wait_minutes),
            ),
            checked: time(Utc::now()),
        }
    }
}

#[derive(Template)]
#[template(path = "docs.html")]
pub struct DocsTemplate {
    pub assets: AssetTags,
}

fn minutes(value: Option<i32>) -> String {
    value
        .map(|value| format!("{value} min"))
        .unwrap_or_else(|| "-".to_string())
}

fn count(value: Option<i32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn time(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&Los_Angeles)
        .format("%-I:%M:%S %p")
        .to_string()
}

fn clip(value: Option<&str>, fallback: &str) -> String {
    let Some(value) = value else {
        return fallback.to_string();
    };
    let cleaned = value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();
    let clipped = cleaned.chars().take(180).collect::<String>();
    if cleaned.chars().count() > 180 {
        format!("{clipped}...")
    } else {
        clipped
    }
}

fn svg_data_uri(svg: &str) -> String {
    let mut uri = String::from("data:image/svg+xml,");
    for byte in svg.trim().bytes() {
        match byte {
            b' '
            | b'"'
            | b'#'
            | b'%'
            | b'&'
            | b'<'
            | b'>'
            | b'?'
            | b'`'
            | b'{'
            | b'}'
            | b'|'
            | b'\''
            | b'\\'
            | b'^'
            | 0..=31
            | 127..=255 => {
                write!(&mut uri, "%{byte:02X}").expect("write to string");
            }
            _ => uri.push(byte as char),
        }
    }
    uri
}

fn nice_axis(value: i32) -> i32 {
    match value {
        ..=5 => 5,
        6..=10 => 10,
        11..=15 => 15,
        16..=20 => 20,
        21..=30 => 30,
        31..=45 => 45,
        46..=60 => 60,
        _ => ((value + 29) / 30) * 30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heytea_core::HistoryPoint;

    #[test]
    fn dashboard_payload_stays_small() {
        let now = Utc::now();
        let status = StatusResponse {
            name: "Downtown Metreon".to_string(),
            address: "165 4th St, San Francisco, CA 94103".to_string(),
            is_open: Some(true),
            pickup_wait_minutes: Some(8),
            delivery_estimate_minutes: Some(12),
            making_cups: Some(4),
            making_orders: Some(3),
            notice: None,
            closing_notice: None,
            observed_at: now,
            stale: false,
            stale_after: now,
        };
        let history = HistoryResponse {
            range: "24h".to_string(),
            generated_at: now,
            points: (0..48)
                .map(|index| HistoryPoint {
                    start: now,
                    end: now,
                    avg_pickup_wait_minutes: Some((index % 18) as f64),
                    min_pickup_wait_minutes: Some(0),
                    max_pickup_wait_minutes: Some(18),
                    avg_delivery_estimate_minutes: None,
                    avg_making_cups: None,
                    avg_making_orders: None,
                    sample_count: 1,
                })
                .collect(),
        };
        let template = DashboardTemplate::new(
            Some(status),
            Some(history),
            "https://api.heytea.dev/stream".to_string(),
        );
        let html = template.render().expect("render dashboard");
        assert!(html.len() < 40 * 1024);
    }
}

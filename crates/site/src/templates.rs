use crate::assets::AssetTags;
use askama::Template;
use chrono::{DateTime, Utc};
use chrono_tz::America::Los_Angeles;
use chrono_tz::Tz;
use heytea_core::{HistoryResponse, LocationResponse, LocationsResponse, StatusResponse};
use std::fmt::Write as _;

const DASHBOARD_CSS: &str = include_str!("../templates/dashboard.css");
const DASHBOARD_JS: &str = include_str!("../templates/dashboard.js");
const FINDER_JS: &str = include_str!("../templates/finder.js");
const DASHBOARD_FONT_B64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/dashboard-font.woff2.b64"));
const FAVICON_SVG: &str = include_str!("../templates/heyteafavi.svg");

#[derive(Debug, Clone)]
pub(crate) struct DashboardView {
    pub(crate) closed: bool,
    pub(crate) open: bool,
    pub(crate) status_line: String,
    pub(crate) wait_minutes: i32,
    pub(crate) observed_at: String,
    pub(crate) trend_points: String,
    pub(crate) trend_data: String,
    pub(crate) trend_minute: i64,
}

impl DashboardView {
    pub(crate) fn new(
        status: Option<&StatusResponse>,
        history: Option<&HistoryResponse>,
        location_name: &str,
        timezone: &str,
    ) -> Self {
        let values = history
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
            .map(|status| status.observed_at.timestamp() / 60)
            .unwrap_or_default();
        let closed = status.and_then(|status| status.is_open) != Some(true);
        let open = !closed;
        let wait_minutes = status
            .and_then(|status| status.pickup_wait_minutes)
            .unwrap_or_default();
        let observed_at = status
            .map(|status| status.observed_at.to_rfc3339())
            .unwrap_or_default();
        let status_line = if closed {
            "closed".to_string()
        } else {
            let status = status.expect("open status exists");
            format!(
                "the wait time at heytea {} is {} as of {}, which was {}",
                location_name.to_ascii_lowercase(),
                minute_words(wait_minutes),
                compact_time(status.observed_at, timezone),
                age_words(status.observed_at)
            )
        };

        Self {
            closed,
            open,
            status_line,
            wait_minutes,
            observed_at,
            trend_points,
            trend_data,
            trend_minute,
        }
    }
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub stream_url: String,
    pub canonical_url: String,
    pub location_name: String,
    pub timezone: String,
    pub favicon_href: String,
    pub inline_css: String,
    pub inline_js: &'static str,
    pub evil: bool,
    pub closed: bool,
    pub open: bool,
    pub status_line: String,
    pub wait_minutes: i32,
    pub observed_at: String,
    pub trend_points: String,
    pub trend_data: String,
    pub trend_minute: i64,
}

impl DashboardTemplate {
    pub fn new(
        status: Option<StatusResponse>,
        history: Option<HistoryResponse>,
        stream_url: String,
        location: LocationResponse,
        evil: bool,
    ) -> Self {
        let view = DashboardView::new(
            status.as_ref(),
            history.as_ref(),
            &location.name,
            &location.timezone,
        );

        Self {
            stream_url,
            canonical_url: format!("https://heytea.dev/{}", location.slug),
            location_name: location.name.to_ascii_lowercase(),
            timezone: location.timezone,
            favicon_href: svg_data_uri(&favicon_svg()),
            inline_css: dashboard_css(),
            inline_js: DASHBOARD_JS,
            evil,
            closed: view.closed,
            open: view.open,
            status_line: view.status_line,
            wait_minutes: view.wait_minutes,
            observed_at: view.observed_at,
            trend_points: view.trend_points,
            trend_data: view.trend_data,
            trend_minute: view.trend_minute,
        }
    }
}

#[derive(Template)]
#[template(path = "finder.html")]
pub struct FinderTemplate {
    pub favicon_href: String,
    pub inline_css: String,
    pub inline_js: &'static str,
    pub locations_json: String,
    pub evil: bool,
}

impl FinderTemplate {
    pub fn new(locations: Option<LocationsResponse>, evil: bool) -> Self {
        Self {
            favicon_href: svg_data_uri(&favicon_svg()),
            inline_css: dashboard_css(),
            inline_js: FINDER_JS,
            evil,
            locations_json: safe_json(&locations.unwrap_or_else(|| LocationsResponse {
                generated_at: Utc::now(),
                locations: Vec::new(),
            })),
        }
    }
}

pub(crate) fn favicon_svg() -> String {
    with_white_background(FAVICON_SVG)
}

#[derive(Template)]
#[template(path = "status.html")]
pub struct StatusTemplate {
    pub assets: AssetTags,
    pub summary: String,
    pub summary_class: String,
    pub checked: String,
    pub components: Vec<StatusComponent>,
}

pub struct StatusComponent {
    pub name: String,
    pub target: String,
    pub state: String,
    pub class_name: String,
    pub uptime: String,
    pub latency: String,
}

#[derive(Template)]
#[template(path = "docs.html")]
pub struct DocsTemplate {
    pub assets: AssetTags,
}

pub(crate) fn site_time(value: DateTime<Utc>) -> String {
    value
        .with_timezone(&Los_Angeles)
        .format("%-I:%M:%S %p")
        .to_string()
}

fn minute_words(value: i32) -> String {
    if value == 1 {
        "1 minute".to_string()
    } else {
        format!("{value} minutes")
    }
}

fn age_words(value: DateTime<Utc>) -> String {
    let seconds = Utc::now().signed_duration_since(value).num_seconds().max(0);
    if seconds == 1 {
        "1 second ago".to_string()
    } else {
        format!("{seconds} seconds ago")
    }
}

fn compact_time(value: DateTime<Utc>, timezone: &str) -> String {
    let timezone = timezone.parse::<Tz>().unwrap_or(Los_Angeles);
    value
        .with_timezone(&timezone)
        .format("%-I:%M:%S%P")
        .to_string()
}

fn dashboard_css() -> String {
    format!(
        "@font-face{{font-family:a;src:url(data:font/woff2;base64,{}) format('woff2');font-display:block}}{}",
        DASHBOARD_FONT_B64.trim(),
        DASHBOARD_CSS
    )
}

fn safe_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .expect("serialize template json")
        .replace('<', "\\u003c")
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

fn with_white_background(svg: &str) -> String {
    let svg = svg.trim();
    let Some((open_tag, body)) = svg.split_once('>') else {
        return svg.to_string();
    };
    format!(r##"{open_tag}><rect width="100%" height="100%" fill="#fff"/>{body}"##)
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
            is_estimate: Some(true),
            text: None,
            notices: Vec::new(),
            closing_notices: Vec::new(),
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
        let location = LocationResponse {
            slug: "downtown-metreon".to_string(),
            name: "Downtown Metreon".to_string(),
            address: "165 4th St, San Francisco, CA 94103".to_string(),
            latitude: Some(37.784),
            longitude: Some(-122.403),
            timezone: "America/Los_Angeles".to_string(),
            is_enabled: Some(true),
            support_takeaway: Some(true),
            is_open: Some(true),
            pickup_wait_minutes: Some(8),
            observed_at: Some(now),
            stale: false,
            stale_after: Some(now),
        };
        let template = DashboardTemplate::new(
            Some(status),
            Some(history),
            "https://api.heytea.dev/locations/downtown-metreon/stream".to_string(),
            location,
            false,
        );
        let html = template.render().expect("render dashboard");
        assert!(html.len() < 40 * 1024);
        assert!(html.contains("the wait time at heytea downtown metreon is 8 minutes as of "));
        assert!(html.contains(", which was "));
        assert!(html.contains("data-observed-at="));
        assert!(html.contains("data:font/woff2;base64,"));
        assert!(html.contains("data:image/svg+xml,"));
        assert!(html.contains("addEventListener(\"load\""));
        assert!(!html.contains("rel=\"stylesheet\""));
        assert!(!html.contains("type=\"module\""));
        assert!(!html.contains("/assets/"));
        assert!(!html.contains("/a.woff2"));
        assert!(!html.contains("/icon.svg"));
        assert!(!html.contains("rel=\"manifest\""));
    }

    #[test]
    fn evil_footer_is_conditional() {
        let evil = FinderTemplate::new(None, true)
            .render()
            .expect("render finder");
        let normal = FinderTemplate::new(None, false)
            .render()
            .expect("render finder");

        assert!(evil.contains("i know you're evil"));
        assert!(!normal.contains("i know you're evil"));
    }
}

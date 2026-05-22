use crate::{
    assets::AssetTags,
    geoip::{miles_between, Coordinates, GeoAttribution, InferredPlace},
};
use askama::Template;
use chrono::{DateTime, Timelike, Utc};
use chrono_tz::America::Los_Angeles;
use chrono_tz::Tz;
use heytea_core::{HistoryResponse, LocationResponse, LocationsResponse, StatusResponse};
use std::cmp::Ordering;
use std::fmt::Write as _;

const DASHBOARD_CSS: &str = include_str!("../templates/dashboard.css");
const DASHBOARD_JS: &str = include_str!("../templates/dashboard.js");
const FINDER_JS: &str = include_str!("../templates/finder.js");
const DASHBOARD_FONT_B64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/dashboard-font.woff2.b64"));
const FAVICON_SVG: &str = include_str!("../templates/heyteafavi.svg");

#[derive(Debug, Clone)]
pub(crate) struct DashboardView {
    pub(crate) managed: bool,
    pub(crate) open: bool,
    pub(crate) graph_hidden: bool,
    pub(crate) status_line: String,
    pub(crate) wait_minutes: i32,
    pub(crate) observed_at: String,
    pub(crate) trend_points: String,
    pub(crate) trend_data: String,
    pub(crate) comparison_points: String,
    pub(crate) comparison_data: String,
    pub(crate) trend_minute: i64,
}

impl DashboardView {
    pub(crate) fn new(
        status: Option<&StatusResponse>,
        history: Option<&HistoryResponse>,
        location: &LocationResponse,
    ) -> Self {
        let today_values = history
            .map(|history| {
                history
                    .points
                    .iter()
                    .filter_map(|point| {
                        point.avg_pickup_wait_minutes.map(|value| {
                            (
                                minute_of_day(point.start, &location.timezone),
                                value.round() as i32,
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let comparison_values = history
            .map(|history| {
                history
                    .comparison_points
                    .iter()
                    .filter_map(|point| {
                        point
                            .avg_pickup_wait_minutes
                            .map(|value| (point.minute_of_day, value.round() as i32))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let trend_max = nice_axis(
            today_values
                .iter()
                .chain(comparison_values.iter())
                .map(|(_, value)| *value)
                .max()
                .unwrap_or(0),
        );
        let trend_points = svg_points(&today_values, trend_max);
        let trend_data = series_data(&today_values);
        let comparison_points = svg_points(&comparison_values, trend_max);
        let comparison_data = series_data(&comparison_values);
        let trend_minute = status
            .map(|status| status.observed_at.timestamp() / 60)
            .unwrap_or_default();
        let managed = location.is_managed;
        let observed_open = status.and_then(|status| status.is_open);
        let closed = observed_open.or(location.is_open) == Some(false);
        let open = observed_open == Some(true);
        let graph_hidden = !open || today_values.is_empty();
        let wait_minutes = status
            .and_then(|status| status.pickup_wait_minutes)
            .unwrap_or_default();
        let observed_at = status
            .map(|status| status.observed_at.to_rfc3339())
            .unwrap_or_default();
        let location_name = location.name.to_ascii_lowercase();
        let status_line = if closed {
            "closed".to_string()
        } else if !open {
            format!("waiting for the first tracked wait observation at heytea {location_name}.")
        } else {
            let status = status.expect("open status exists");
            format!(
                "the wait time at heytea {} is {} as of {}, which was {}",
                location_name,
                minute_words(wait_minutes),
                compact_time(status.observed_at, &location.timezone),
                age_words(status.observed_at)
            )
        };

        Self {
            managed,
            open,
            graph_hidden,
            status_line,
            wait_minutes,
            observed_at,
            trend_points,
            trend_data,
            comparison_points,
            comparison_data,
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
    pub managed: bool,
    pub favicon_href: String,
    pub inline_css: String,
    pub inline_js: &'static str,
    pub evil: bool,
    pub open: bool,
    pub graph_hidden: bool,
    pub status_line: String,
    pub wait_minutes: i32,
    pub observed_at: String,
    pub trend_points: String,
    pub trend_data: String,
    pub comparison_points: String,
    pub comparison_data: String,
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
        let view = DashboardView::new(status.as_ref(), history.as_ref(), &location);
        let canonical_url = format!("https://heytea.dev/{}", location.slug);

        Self {
            stream_url,
            canonical_url,
            location_name: location.name.to_ascii_lowercase(),
            timezone: location.timezone,
            managed: view.managed,
            favicon_href: svg_data_uri(&favicon_svg()),
            inline_css: dashboard_css(),
            inline_js: DASHBOARD_JS,
            evil,
            open: view.open,
            graph_hidden: view.graph_hidden,
            status_line: view.status_line,
            wait_minutes: view.wait_minutes,
            observed_at: view.observed_at,
            trend_points: view.trend_points,
            trend_data: view.trend_data,
            comparison_points: view.comparison_points,
            comparison_data: view.comparison_data,
            trend_minute: view.trend_minute,
        }
    }
}

#[derive(Template)]
#[template(path = "about.html")]
pub struct AboutTemplate {
    pub favicon_href: String,
    pub inline_css: String,
    pub evil: bool,
}

#[derive(Template)]
#[template(path = "finder.html")]
pub struct FinderTemplate {
    pub favicon_href: String,
    pub inline_css: String,
    pub inline_js: &'static str,
    pub rows: Vec<FinderLocationRow>,
    pub location_status: String,
    pub show_geo_attribution: bool,
    pub geo_attribution_name: String,
    pub geo_attribution_url: String,
    pub evil: bool,
}

pub struct FinderLocationRow {
    pub slug: String,
    pub name: String,
    pub search_text: String,
    pub status_text: String,
    pub latitude: String,
    pub longitude: String,
    pub distance: String,
    pub distance_text: String,
    pub distance_hidden: bool,
    pub open_sort: u8,
    pub wait_sort: i32,
    pub name_sort: String,
}

impl FinderTemplate {
    pub fn new(
        locations: Option<LocationsResponse>,
        inferred: Option<&InferredPlace>,
        attribution: Option<&GeoAttribution>,
        evil: bool,
    ) -> Self {
        let mut locations = locations
            .map(|locations| locations.locations)
            .unwrap_or_default();
        let here = inferred.map(|place| place.coordinates);
        sort_locations(&mut locations, here);
        let rows = locations
            .into_iter()
            .map(|location| FinderLocationRow::new(location, here))
            .collect();
        let location_status = match inferred {
            Some(place) => format!(
                "server location guess: {}; showing nearest stores first.",
                place.label
            ),
            None => "server location guess unavailable; showing open stores first.".to_string(),
        };
        let show_geo_attribution = inferred.is_some() && attribution.is_some();
        let (geo_attribution_name, geo_attribution_url) = attribution
            .map(|attribution| (attribution.name.clone(), attribution.url.clone()))
            .unwrap_or_default();

        Self {
            favicon_href: svg_data_uri(&favicon_svg()),
            inline_css: dashboard_css(),
            inline_js: FINDER_JS,
            rows,
            location_status,
            show_geo_attribution,
            geo_attribution_name,
            geo_attribution_url,
            evil,
        }
    }
}

impl FinderLocationRow {
    fn new(location: LocationResponse, here: Option<Coordinates>) -> Self {
        let coordinates = location_coordinates(&location);
        let distance = here
            .zip(coordinates)
            .map(|(here, location)| miles_between(here, location));
        let name_sort = location.name.to_ascii_lowercase();
        let search_text = format!("{} {} {}", location.name, location.slug, location.address)
            .to_ascii_lowercase();
        let status_text = finder_status(&location);
        let open_sort = open_sort(&location);
        let wait_sort = wait_sort(&location);

        Self {
            slug: location.slug,
            name: name_sort.clone(),
            search_text,
            status_text,
            latitude: coordinates
                .map(|coordinates| coordinates.latitude.to_string())
                .unwrap_or_default(),
            longitude: coordinates
                .map(|coordinates| coordinates.longitude.to_string())
                .unwrap_or_default(),
            distance: distance
                .map(|distance| format!("{distance:.6}"))
                .unwrap_or_default(),
            distance_text: distance
                .map(|distance| format!("{distance:.1} mi"))
                .unwrap_or_default(),
            distance_hidden: distance.is_none(),
            open_sort,
            wait_sort,
            name_sort,
        }
    }
}

fn sort_locations(locations: &mut [LocationResponse], here: Option<Coordinates>) {
    locations.sort_by(|a, b| match here {
        Some(here) => distance_sort(a, here)
            .total_cmp(&distance_sort(b, here))
            .then_with(|| default_location_order(a, b)),
        None => default_location_order(a, b),
    });
}

fn distance_sort(location: &LocationResponse, here: Coordinates) -> f64 {
    location_coordinates(location)
        .map(|coordinates| miles_between(here, coordinates))
        .unwrap_or(f64::INFINITY)
}

fn default_location_order(a: &LocationResponse, b: &LocationResponse) -> Ordering {
    open_sort(a)
        .cmp(&open_sort(b))
        .then_with(|| wait_sort(a).cmp(&wait_sort(b)))
        .then_with(|| a.name.cmp(&b.name))
}

fn open_sort(location: &LocationResponse) -> u8 {
    if location.is_open == Some(true) {
        0
    } else {
        1
    }
}

fn wait_sort(location: &LocationResponse) -> i32 {
    location.pickup_wait_minutes.unwrap_or(i32::MAX)
}

fn finder_status(location: &LocationResponse) -> String {
    match location.is_open {
        Some(true) => location
            .pickup_wait_minutes
            .map(minute_words)
            .unwrap_or_else(|| "wait unknown".to_string()),
        Some(false) => "closed".to_string(),
        None => "status unknown".to_string(),
    }
}

fn location_coordinates(location: &LocationResponse) -> Option<Coordinates> {
    let coordinates = Coordinates {
        latitude: location.latitude?,
        longitude: location.longitude?,
    };
    (coordinates.latitude.is_finite()
        && coordinates.longitude.is_finite()
        && (-90.0..=90.0).contains(&coordinates.latitude)
        && (-180.0..=180.0).contains(&coordinates.longitude))
    .then_some(coordinates)
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

fn minute_of_day(value: DateTime<Utc>, timezone: &str) -> i32 {
    let timezone = timezone.parse::<Tz>().unwrap_or(Los_Angeles);
    let local = value.with_timezone(&timezone);
    local.hour() as i32 * 60 + local.minute() as i32
}

fn svg_points(values: &[(i32, i32)], max_value: i32) -> String {
    values
        .iter()
        .map(|(minute, value)| {
            let minute = (*minute).clamp(0, 1439) as f64;
            let x = minute * 100.0 / 1439.0;
            let y = 38.0 - *value as f64 / max_value.max(1) as f64 * 30.0;
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn series_data(values: &[(i32, i32)]) -> String {
    values
        .iter()
        .map(|(minute, value)| format!("{minute}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn dashboard_css() -> String {
    format!(
        "@font-face{{font-family:a;src:url(data:font/woff2;base64,{}) format('woff2');font-display:block}}{}",
        DASHBOARD_FONT_B64.trim(),
        DASHBOARD_CSS
    )
}

pub(crate) fn svg_data_uri(svg: &str) -> String {
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
    use heytea_core::{HistoryComparisonPoint, HistoryPoint};

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
            comparison_points: (0..48)
                .map(|index| HistoryComparisonPoint {
                    minute_of_day: index * 30,
                    avg_pickup_wait_minutes: Some((index % 12) as f64),
                    sample_count: 7,
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
            is_managed: true,
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
    fn dashboard_waits_for_first_tracked_observation() {
        let mut location = test_location("beverly-hills", "Beverly Hills", 34.069, -118.4, 0);
        location.pickup_wait_minutes = None;
        location.observed_at = None;
        location.stale = true;
        location.stale_after = None;

        let template = DashboardTemplate::new(
            None,
            None,
            "https://api.heytea.dev/locations/beverly-hills/stream".to_string(),
            location,
            false,
        );
        let html = template.render().expect("render dashboard");

        assert!(html
            .contains("waiting for the first tracked wait observation at heytea beverly hills."));
        assert!(html
            .contains("data-stream-url=\"https://api.heytea.dev/locations/beverly-hills/stream\""));
        assert!(html.contains("<svg id=\"graph\""));
        assert!(!html.contains("request managed tracking"));
        assert!(!html.contains("live waits load only while this page is open"));
        assert!(!html.contains("id=\"tracking-request\""));
        assert!(!html.contains("jet@extremist.software"));
        assert!(!html.contains("mailto:jet"));
    }

    #[test]
    fn about_template_renders_origin_story() {
        let template = AboutTemplate {
            favicon_href: svg_data_uri(&favicon_svg()),
            inline_css: dashboard_css(),
            evil: false,
        };
        let html = template.render().expect("render about");

        assert!(html.contains("about heytea.dev"));
        assert!(html.contains("heytea.dev started because i wanted to meet friends at HeyTea"));
        assert!(html.contains("https://heytea.dev/about"));
    }

    #[test]
    fn evil_footer_is_conditional() {
        let evil = FinderTemplate::new(None, None, None, true)
            .render()
            .expect("render finder");
        let normal = FinderTemplate::new(None, None, None, false)
            .render()
            .expect("render finder");

        assert!(evil.contains("i know you're evil"));
        assert!(!normal.contains("i know you're evil"));
    }

    #[test]
    fn finder_renders_server_ordered_rows() {
        let now = Utc::now();
        let mut far = test_location("far", "Far Store", 34.0522, -118.2437, 4);
        far.is_managed = false;
        far.pickup_wait_minutes = None;
        far.observed_at = None;
        far.stale = true;
        far.stale_after = None;
        let locations = LocationsResponse {
            generated_at: now,
            locations: vec![
                far,
                test_location("near", "Near Store", 37.784, -122.403, 8),
            ],
        };
        let inferred = InferredPlace {
            coordinates: Coordinates {
                latitude: 37.784,
                longitude: -122.403,
            },
            label: "San Francisco, California, United States".to_string(),
        };
        let attribution = GeoAttribution {
            name: "DB-IP".to_string(),
            url: "https://db-ip.com".to_string(),
        };
        let template =
            FinderTemplate::new(Some(locations), Some(&inferred), Some(&attribution), false);

        assert_eq!(template.rows[0].slug, "near");
        let html = template.render().expect("render finder");
        assert!(html.contains("server location guess: San Francisco"));
        assert!(html.contains("search checks store name, URL slug, and address."));
        assert!(html.contains("data-distance=\"0.000000\""));
        assert!(html.contains("wait unknown"));
        assert!(!html.contains("tracked"));
        assert!(!html.contains("request tracking"));
        assert!(html.contains("IP location data by"));
        assert!(!html.contains("locations-data"));
    }

    fn test_location(
        slug: &str,
        name: &str,
        latitude: f64,
        longitude: f64,
        wait_minutes: i32,
    ) -> LocationResponse {
        let now = Utc::now();
        LocationResponse {
            slug: slug.to_string(),
            name: name.to_string(),
            address: format!("{name} address"),
            latitude: Some(latitude),
            longitude: Some(longitude),
            timezone: "America/Los_Angeles".to_string(),
            is_managed: true,
            is_enabled: Some(true),
            support_takeaway: Some(true),
            is_open: Some(true),
            pickup_wait_minutes: Some(wait_minutes),
            observed_at: Some(now),
            stale: false,
            stale_after: Some(now),
        }
    }
}

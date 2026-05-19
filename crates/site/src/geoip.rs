use axum::http::HeaderMap;
use maxminddb::geoip2;
use std::{collections::BTreeMap, env, net::IpAddr, net::SocketAddr, path::Path, sync::Arc};

const MMDB_ENV: &str = "HEYTEA_GEOIP_MMDB";
const ATTRIBUTION_NAME_ENV: &str = "HEYTEA_GEOIP_ATTRIBUTION_NAME";
const ATTRIBUTION_URL_ENV: &str = "HEYTEA_GEOIP_ATTRIBUTION_URL";

#[derive(Debug, Clone, Copy)]
pub(crate) struct Coordinates {
    pub(crate) latitude: f64,
    pub(crate) longitude: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct InferredPlace {
    pub(crate) coordinates: Coordinates,
    pub(crate) label: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GeoAttribution {
    pub(crate) name: String,
    pub(crate) url: String,
}

#[derive(Clone)]
pub(crate) struct GeoIp {
    reader: Option<Arc<maxminddb::Reader<Vec<u8>>>>,
    attribution: Option<GeoAttribution>,
}

impl GeoIp {
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let Some(path) = env::var_os(MMDB_ENV).filter(|value| !value.is_empty()) else {
            return Ok(Self::disabled());
        };
        Self::from_path(path)
    }

    pub(crate) fn disabled() -> Self {
        Self {
            reader: None,
            attribution: None,
        }
    }

    pub(crate) fn from_path(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let reader = maxminddb::Reader::open_readfile(path)?;
        Ok(Self {
            reader: Some(Arc::new(reader)),
            attribution: Some(default_attribution()),
        })
    }

    pub(crate) fn lookup_request(
        &self,
        headers: &HeaderMap,
        peer: Option<SocketAddr>,
    ) -> Option<InferredPlace> {
        self.lookup_ip(client_ip(headers, peer)?)
    }

    pub(crate) fn lookup_ip(&self, ip: IpAddr) -> Option<InferredPlace> {
        if !is_public_ip(ip) {
            return None;
        }

        let city = self.reader.as_ref()?.lookup::<geoip2::City<'_>>(ip).ok()?;
        let location = city.location.as_ref()?;
        let coordinates = Coordinates {
            latitude: location.latitude?,
            longitude: location.longitude?,
        };
        if !valid_coordinates(coordinates) {
            return None;
        }

        Some(InferredPlace {
            coordinates,
            label: place_label(&city).unwrap_or_else(|| "your network".to_string()),
        })
    }

    pub(crate) fn attribution(&self) -> Option<&GeoAttribution> {
        self.attribution.as_ref()
    }
}

pub(crate) fn miles_between(a: Coordinates, b: Coordinates) -> f64 {
    const EARTH_RADIUS_MILES: f64 = 3958.8;
    let d_lat = (b.latitude - a.latitude).to_radians();
    let d_lon = (b.longitude - a.longitude).to_radians();
    let a_lat = a.latitude.to_radians();
    let b_lat = b.latitude.to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + a_lat.cos() * b_lat.cos() * (d_lon / 2.0).sin().powi(2);
    let h = h.clamp(0.0, 1.0);
    2.0 * EARTH_RADIUS_MILES * h.sqrt().atan2((1.0 - h).sqrt())
}

fn default_attribution() -> GeoAttribution {
    GeoAttribution {
        name: env::var(ATTRIBUTION_NAME_ENV).unwrap_or_else(|_| "DB-IP".to_string()),
        url: env::var(ATTRIBUTION_URL_ENV).unwrap_or_else(|_| "https://db-ip.com".to_string()),
    }
}

fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<IpAddr> {
    let peer_ip = peer.map(|peer| peer.ip());
    if peer_ip.is_some_and(is_trusted_proxy) {
        if let Some(ip) = forwarded_for_ip(headers) {
            return Some(ip);
        }
    }

    peer_ip.filter(|ip| is_public_ip(*ip))
}

fn is_trusted_proxy(ip: IpAddr) -> bool {
    ip.is_loopback()
}

fn forwarded_for_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())?
        .split(',')
        .filter_map(parse_ip)
        .find(|ip| is_public_ip(*ip))
}

fn parse_ip(value: &str) -> Option<IpAddr> {
    let value = value.trim().trim_matches('"');
    if let Ok(ip) = value.parse() {
        return Some(ip);
    }

    if let Some(rest) = value.strip_prefix('[') {
        if let Some((host, _)) = rest.split_once(']') {
            return host.parse().ok();
        }
    }

    if value.matches(':').count() == 1 {
        return value
            .rsplit_once(':')
            .and_then(|(host, _)| host.parse().ok());
    }

    None
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || matches!(octets, [100, 64..=127, _, _])
                || matches!(octets, [192, 0, 0, _])
                || matches!(octets, [198, 18 | 19, _, _]))
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x2001 && segments[1] == 0x0db8))
        }
    }
}

fn valid_coordinates(coordinates: Coordinates) -> bool {
    coordinates.latitude.is_finite()
        && coordinates.longitude.is_finite()
        && (-90.0..=90.0).contains(&coordinates.latitude)
        && (-180.0..=180.0).contains(&coordinates.longitude)
}

fn place_label(city: &geoip2::City<'_>) -> Option<String> {
    let city_name = city
        .city
        .as_ref()
        .and_then(|city| city.names.as_ref())
        .and_then(name_value);
    let region_name = city
        .subdivisions
        .as_ref()
        .and_then(|subdivisions| subdivisions.first())
        .and_then(|subdivision| {
            subdivision
                .names
                .as_ref()
                .and_then(name_value)
                .or(subdivision.iso_code)
        });
    let country_name = city.country.as_ref().and_then(|country| {
        country
            .names
            .as_ref()
            .and_then(name_value)
            .or(country.iso_code)
    });

    let parts = [city_name, region_name, country_name]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn name_value<'a>(names: &'a BTreeMap<&str, &str>) -> Option<&'a str> {
    names
        .get("en")
        .copied()
        .or_else(|| names.values().next().copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use std::net::{Ipv4Addr, SocketAddr};

    #[test]
    fn distance_is_zero_for_same_point() {
        let here = Coordinates {
            latitude: 37.784,
            longitude: -122.403,
        };

        assert_eq!(miles_between(here, here), 0.0);
    }

    #[test]
    fn distance_handles_antipodes() {
        let miles = miles_between(
            Coordinates {
                latitude: 0.0,
                longitude: 0.0,
            },
            Coordinates {
                latitude: 0.0,
                longitude: 180.0,
            },
        );

        assert!((miles - 12_437.0).abs() < 1.0);
    }

    #[test]
    fn forwarded_ip_requires_trusted_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 8.8.8.8"),
        );
        let loopback_peer = SocketAddr::from((Ipv4Addr::LOCALHOST, 3100));
        let public_peer = SocketAddr::from((Ipv4Addr::new(8, 8, 4, 4), 443));

        assert_eq!(
            client_ip(&headers, Some(loopback_peer)),
            Some(IpAddr::from([8, 8, 8, 8]))
        );
        assert_eq!(
            client_ip(&headers, Some(public_peer)),
            Some(IpAddr::from([8, 8, 4, 4]))
        );
    }
}

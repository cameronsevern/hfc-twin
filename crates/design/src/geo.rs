//! Lengths are feet, and a map's area is small enough for a local flat projection.

use schema::common::LonLat;

const FT_PER_DEG_LAT: f64 = 364_000.0;

/// Feet east per degree of longitude at this latitude.
fn ft_per_deg_lon(lat_deg: f64) -> f64 {
    FT_PER_DEG_LAT * lat_deg.to_radians().cos()
}

/// Distance between two points, feet.
pub fn dist_ft(a: LonLat, b: LonLat) -> f64 {
    let dx = (a.lon_deg - b.lon_deg) * ft_per_deg_lon((a.lat_deg + b.lat_deg) / 2.0);
    let dy = (a.lat_deg - b.lat_deg) * FT_PER_DEG_LAT;
    (dx * dx + dy * dy).sqrt()
}

/// A point `east_ft` and `north_ft` from `origin`.
pub fn offset(origin: LonLat, east_ft: f64, north_ft: f64) -> LonLat {
    LonLat {
        lon_deg: origin.lon_deg + east_ft / ft_per_deg_lon(origin.lat_deg),
        lat_deg: origin.lat_deg + north_ft / FT_PER_DEG_LAT,
    }
}

/// `share` of the way from `a` to `b`.
pub fn lerp(a: LonLat, b: LonLat, share: f64) -> LonLat {
    LonLat {
        lon_deg: a.lon_deg + (b.lon_deg - a.lon_deg) * share,
        lat_deg: a.lat_deg + (b.lat_deg - a.lat_deg) * share,
    }
}

/// The share along `a`→`b` closest to `p`, clamped to the segment.
pub fn nearest_share(a: LonLat, b: LonLat, p: LonLat) -> f64 {
    let k = ft_per_deg_lon(a.lat_deg) / FT_PER_DEG_LAT;
    let (ax, ay) = (a.lon_deg * k, a.lat_deg);
    let (bx, by) = (b.lon_deg * k, b.lat_deg);
    let (px, py) = (p.lon_deg * k, p.lat_deg);
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return 0.0;
    }
    (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
}

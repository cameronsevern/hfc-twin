//! Generated geography by family, from a seed. All synthetic and labeled so; a generated plant is never
//! presented as an operator's (invariant 11).

use schema::common::LonLat;
use schema::plant::Installation;
use schema::region::{Home, LayoutFamily, MapSource, SCHEMA_VERSION, Street, StreetMap};
use sim::rng::Rng;

use crate::geo::{dist_ft, lerp, offset};

/// An arbitrary place to put generated maps: the Front Range, as in the one-street fixture.
const ORIGIN: LonLat = LonLat {
    lon_deg: -105.1,
    lat_deg: 39.9,
};

/// How close, feet, a street's end has to come to another street to join it.
const JOIN_FT: f64 = 3.0;

/// Adds a shared point wherever two streets meet, as real map data has at a junction. A street graph only connects at shared points, so
/// without one a crossing is two streets that never meet, and whatever's on the far one can't be reached.
fn join_streets(streets: &mut [Street]) {
    let mut cuts: Vec<Vec<(usize, f64)>> = vec![Vec::new(); streets.len()];
    for i in 0..streets.len() {
        for j in i + 1..streets.len() {
            for (si, a) in streets[i].points.windows(2).enumerate() {
                for (sj, b) in streets[j].points.windows(2).enumerate() {
                    let (ax, ay) = (a[1].lon_deg - a[0].lon_deg, a[1].lat_deg - a[0].lat_deg);
                    let (bx, by) = (b[1].lon_deg - b[0].lon_deg, b[1].lat_deg - b[0].lat_deg);
                    let cross = ax * by - ay * bx;
                    if cross.abs() < 1e-18 {
                        continue;
                    }
                    let (dx, dy) = (b[0].lon_deg - a[0].lon_deg, b[0].lat_deg - a[0].lat_deg);
                    let t = (dx * by - dy * bx) / cross;
                    let u = (dx * ay - dy * ax) / cross;
                    let slack_t = JOIN_FT / dist_ft(a[0], a[1]).max(1e-9);
                    let slack_u = JOIN_FT / dist_ft(b[0], b[1]).max(1e-9);
                    if t < -slack_t || t > 1.0 + slack_t || u < -slack_u || u > 1.0 + slack_u {
                        continue;
                    }
                    if t > slack_t && t < 1.0 - slack_t {
                        cuts[i].push((si, t));
                    }
                    if u > slack_u && u < 1.0 - slack_u {
                        cuts[j].push((sj, u));
                    }
                }
            }
        }
    }
    for (street, mut cut) in streets.iter_mut().zip(cuts) {
        // Back to front, so an insertion doesn't move the ones still to come.
        cut.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));
        let before = street.points.clone();
        for (segment, share) in cut {
            let at = lerp(before[segment], before[segment + 1], share);
            street.points.insert(segment + 1, at);
        }
    }
}

struct Builder {
    origin: LonLat,
    streets: Vec<Street>,
    homes: Vec<Home>,
    next_home: usize,
}

impl Builder {
    fn street(&mut self, points: Vec<(f64, f64)>, installation: Installation) -> usize {
        let id = self.streets.len();
        self.streets.push(Street {
            id: format!("street-{id}"),
            points: points
                .into_iter()
                .map(|(e, n)| offset(self.origin, e, n))
                .collect(),
            installation,
        });
        id
    }

    fn home(&mut self, east_ft: f64, north_ft: f64) {
        self.homes.push(Home {
            id: format!("home-{}", self.next_home),
            geo: offset(self.origin, east_ft, north_ft),
        });
        self.next_home += 1;
    }

    /// Homes down both sides of the straight run `a`→`b`, one per `lot_ft`, set back `setback_ft`.
    fn lots(
        &mut self,
        a: (f64, f64),
        b: (f64, f64),
        lot_ft: f64,
        setback_ft: f64,
        rng: &mut Rng,
        fill: f64,
    ) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            return;
        }
        let (ux, uy) = (dx / len, dy / len);
        let mut along = lot_ft / 2.0;
        while along < len {
            for side in [-1.0, 1.0] {
                if rng.uniform() < fill {
                    let jitter = rng.range(-0.1, 0.1) * lot_ft;
                    self.home(
                        a.0 + ux * (along + jitter) - uy * side * setback_ft,
                        a.1 + uy * (along + jitter) + ux * side * setback_ft,
                    );
                }
            }
            along += lot_ft;
        }
    }
}

/// A map of about `homes` homes in `family`.
pub fn generate(family: LayoutFamily, seed: u64, homes: usize) -> StreetMap {
    let mut rng = Rng::derive(seed, &[family as u64 + 1, homes as u64]);
    let mut b = Builder {
        origin: ORIGIN,
        streets: Vec::new(),
        homes: Vec::new(),
        next_home: 0,
    };
    match family {
        LayoutFamily::UrbanGrid => {
            // Blocks of 330–420 ft, 50 ft lots.
            let block = rng.range(330.0, 420.0);
            let lot = rng.range(45.0, 60.0);
            let per_block_street = 2.0 * block / lot;
            let streets_needed = (homes as f64 / per_block_street / 2.0).max(1.0);
            let n = (streets_needed.sqrt().ceil() as usize + 1).max(2);
            for i in 0..=n {
                let x = i as f64 * block;
                b.street(vec![(x, 0.0), (x, n as f64 * block)], Installation::Aerial);
                b.street(vec![(0.0, x), (n as f64 * block, x)], Installation::Aerial);
            }
            'grid: for i in 0..n {
                for j in 0..n {
                    let (x0, y0) = (i as f64 * block, j as f64 * block);
                    b.lots((x0, y0), (x0 + block, y0), lot, 40.0, &mut rng, 0.95);
                    b.lots((x0, y0), (x0, y0 + block), lot, 40.0, &mut rng, 0.95);
                    if b.homes.len() >= homes {
                        break 'grid;
                    }
                }
            }
        }
        LayoutFamily::Suburban => {
            // An arterial with cul-de-sacs off it. Underground, as most newer subdivisions are.
            let lot = rng.range(70.0, 100.0);
            let spacing = rng.range(450.0, 650.0);
            let court_len = rng.range(350.0, 700.0);
            let courts = ((homes as f64 * lot / 2.0 / court_len / 2.0).ceil() as usize).max(2);
            let run = courts as f64 * spacing;
            b.street(
                vec![(0.0, 0.0), (run * 0.5, 40.0), (run, 0.0)],
                Installation::Underground,
            );
            let mut placed = 0;
            for i in 0..courts {
                let x = (i as f64 + 0.5) * spacing;
                let y = if x < run * 0.5 {
                    40.0 * x / (run * 0.5)
                } else {
                    40.0 * (run - x) / (run * 0.5)
                };
                for side in [-1.0, 1.0] {
                    let len = court_len * rng.range(0.7, 1.2);
                    let bend = rng.range(-120.0, 120.0);
                    let end = (x + bend, y + side * len);
                    let mid = (x + bend * 0.3, y + side * len * 0.55);
                    b.street(vec![(x, y), mid, end], Installation::Underground);
                    let before = b.homes.len();
                    b.lots((x, y), mid, lot, 45.0, &mut rng, 0.9);
                    b.lots(mid, end, lot, 45.0, &mut rng, 0.9);
                    placed += b.homes.len() - before;
                }
                if placed >= homes {
                    break;
                }
            }
        }
        LayoutFamily::Rural => {
            // A long road with spurs: homes few and far between, aerial.
            let spur_gap = rng.range(2500.0, 4500.0);
            let per_mile = rng.range(8.0, 22.0);
            let lot = 5280.0 / per_mile * 2.0;
            let total_ft = homes as f64 / per_mile * 5280.0;
            let spurs = (total_ft / 2.0 / spur_gap).ceil() as usize;
            let trunk = (spurs.max(1) as f64 + 1.0) * spur_gap;
            b.street(vec![(0.0, 0.0), (trunk, 0.0)], Installation::Aerial);
            b.lots((0.0, 0.0), (trunk, 0.0), lot, 60.0, &mut rng, 0.8);
            for i in 0..spurs.max(1) {
                let x = (i as f64 + 1.0) * spur_gap;
                let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                let len = rng.range(1500.0, 4000.0);
                b.street(vec![(x, 0.0), (x, side * len)], Installation::Aerial);
                b.lots((x, 0.0), (x, side * len), lot, 60.0, &mut rng, 0.8);
            }
        }
    }
    join_streets(&mut b.streets);
    StreetMap {
        schema_version: SCHEMA_VERSION.to_string(),
        id: format!("generated-{family:?}-{seed}-{homes}"),
        source: MapSource::Generated { family, seed },
        headend: ORIGIN,
        streets: b.streets,
        homes: b.homes,
    }
}

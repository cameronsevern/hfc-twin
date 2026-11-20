//! Stage `fittings`: a connector at every cable end, a splice wherever hardline runs past a reel's length, strand or
//! conduit under every span, and each span's path along its street (`docs/physics.md` §14). The walk asks for a span's
//! fittings as it lays the span, so its levels count their loss.

use schema::common::{ComponentId, LonLat, StructureId};
use schema::plant::{Fitting, Installation, RoutePoint};

use super::kit::{Hardline, Kit};
use super::structures::Structures;

/// A hardline span's fittings: a pin at each end and a splice at each reel's end between them.
pub fn span_fittings(cable: &Hardline<'_>, len_ft: f64, reel_len_ft: f64) -> Vec<Fitting> {
    let mut out = vec![Fitting {
        pos_ft: 0.0,
        component_ref: cable.pin.id.clone(),
    }];
    if let Some(splice) = cable.splice
        && reel_len_ft > 0.0
    {
        let mut at = reel_len_ft;
        while at < len_ft - 1e-6 {
            out.push(Fitting {
                pos_ft: at,
                component_ref: splice.id.clone(),
            });
            at += reel_len_ft;
        }
    }
    out.push(Fitting {
        pos_ft: len_ft,
        component_ref: cable.pin.id.clone(),
    });
    out
}

/// A drop's or in-home cable's fittings: an F connector at each end.
pub fn drop_fittings(kit: &Kit<'_>, len_ft: f64) -> Vec<Fitting> {
    vec![
        Fitting {
            pos_ft: 0.0,
            component_ref: kit.drop_f.id.clone(),
        },
        Fitting {
            pos_ft: len_ft,
            component_ref: kit.drop_f.id.clone(),
        },
    ]
}

/// The strand or conduit a span hangs on or runs in.
pub fn support(kit: &Kit<'_>, installation: Installation) -> ComponentId {
    match installation {
        Installation::Aerial => kit.rules.structures.strand_ref.value.clone(),
        Installation::Underground => kit.rules.structures.conduit_ref.value.clone(),
    }
}

/// A span's path along runs, from the structure at its upstream end: the street's points and every structure it passes,
/// the two ends left out. With its length and the installation most of it has.
pub fn span_path(
    structures: &Structures,
    start: usize,
    runs: &[usize],
) -> (Vec<RoutePoint>, f64, Installation) {
    let mut path = Vec::new();
    let mut at = start;
    let (mut aerial, mut buried, mut len_ft) = (0.0, 0.0, 0.0);
    for (i, &r) in runs.iter().enumerate() {
        let run = &structures.runs[r];
        for p in run.path_from(at) {
            path.push(RoutePoint {
                geo: p,
                structure: None,
            });
        }
        at = run.other(at);
        if i + 1 < runs.len() {
            let site = &structures.sites[at];
            path.push(RoutePoint {
                geo: site.geo,
                structure: Some(site.id.clone()),
            });
        }
        len_ft += run.len_ft;
        match run.installation {
            Installation::Aerial => aerial += run.len_ft,
            Installation::Underground => buried += run.len_ft,
        }
    }
    let installation = if buried > aerial {
        Installation::Underground
    } else {
        Installation::Aerial
    };
    (path, len_ft, installation)
}

/// A path's length from one end through its points to the other, feet.
pub fn path_len_ft(from: LonLat, path: &[RoutePoint], to: LonLat) -> f64 {
    let mut len = 0.0;
    let mut last = from;
    for p in path.iter().map(|p| p.geo).chain(std::iter::once(to)) {
        len += crate::geo::dist_ft(last, p);
        last = p;
    }
    len
}

/// The structures a path passes.
pub fn path_structures(path: &[RoutePoint]) -> Vec<StructureId> {
    path.iter().filter_map(|p| p.structure.clone()).collect()
}

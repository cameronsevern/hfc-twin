//! One row per place a fault could be. A fault on a component affects what's below it, so the telling thing is how what's
//! below differs from what isn't. All relative, so the rows hold across profiles and plants (`docs/DESIGN.md`).

use std::collections::{BTreeMap, BTreeSet};

use features::{ModemFeatures, StepFeatures};
use plant_graph::PlantGraph;
use schema::common::ElementId;
use schema::fault::Location;
use schema::plant::{EdgeKind, ElementKind, PortRole};
use schema::rpd::{GcpStatus, PtpResult, RpdState};
use serde::{Deserialize, Serialize};

/// What kind of place a row is about.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    /// Nothing's wrong.
    Healthy,
    /// A hardline span.
    Span,
    /// An amp.
    Amp,
    /// A home's drop.
    Drop,
    /// A power supply.
    Supply,
    /// The node or RPD.
    Root,
    /// A part in a home, or its in-home cable (version 2 plant).
    Premises,
    /// A drop's cable, where it records its fittings (version 2 plant).
    DropCable,
}

/// A candidate, with its features.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ComponentRow {
    /// The plant's name for it. Empty for healthy.
    pub component: String,
    /// Its kind.
    pub kind: ComponentKind,
    /// In the order of `FEATURE_NAMES`. `None` where there's no value.
    pub x: Vec<Option<f64>>,
}

/// The per-modem measures rows summarize.
const METRICS: [&str; 12] = [
    "level",
    "level_min",
    "slope",
    "tx",
    "mer",
    "ripple",
    "nmter",
    "echo_rise",
    "log_uncorr",
    "down",
    "t3",
    "t4",
];

fn metric(modem: &ModemFeatures, index: usize) -> Option<f64> {
    match index {
        0 => modem.level_drift_db,
        1 => modem.level_drift_min_db,
        2 => modem.level_drift_slope_db_per_100mhz,
        3 => modem.us_tx_drift_db,
        4 => modem.mer_drift_db,
        5 => modem.chan_est_ripple_db,
        6 => modem.preeq_nmter_drift_db,
        7 => modem.preeq_echo_rise_db,
        8 => modem.uncorrectable_rate.map(|r| r.max(1e-9).log10()),
        9 => Some(if modem.operational { 0.0 } else { 1.0 }),
        10 => Some((modem.t3_delta as f64).ln_1p()),
        _ => Some((modem.t4_delta as f64).ln_1p()),
    }
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (mut sum, mut n) = (0.0, 0usize);
    for v in values {
        if v.is_finite() {
            sum += v;
            n += 1;
        }
    }
    (n > 0).then(|| sum / n as f64)
}

/// Global features, then kind, then topology, then each metric over below and outside, then the candidate's own.
const GLOBAL: [&str; 11] = [
    "n_modems_ln",
    "share_down",
    "us_snr_drift_min",
    "us_uncorr_log",
    "rpd_not_operational",
    "rpd_gcp_lost",
    "rpd_ptp_holdover",
    "rpd_no_rf",
    "rpd_no_power",
    "share_affected",
    "v_drift_min",
];
const KINDS: [&str; 6] = [
    "k_healthy",
    "k_span",
    "k_amp",
    "k_drop",
    "k_supply",
    "k_root",
];
const TOPOLOGY: [&str; 5] = ["below_share", "jaccard", "depth", "len_ln", "echo_vs_len"];
const OWN: [&str; 7] = [
    "out_drift",
    "v_drift",
    "silent",
    "sib_level",
    "sib_tx",
    "own_level",
    "own_tx",
];

/// Added by task 24, after everything a model before it reads, so its columns don't move: the new kinds, the
/// candidate's fittings, the link's light against design, a port against its set point, a device against the others in
/// its home, and channel-estimate ripple against design.
const AS_BUILT: [&str; 10] = [
    "k_premises",
    "k_drop_cable",
    "fittings_ln",
    "node_rx_drift",
    "hub_rx_drift",
    "port_level_dev",
    "home_dev",
    "ripple_drift_below",
    "ripple_drift_outside",
    "ripple_drift_delta",
];

/// The names of every feature, in order. A model records the ones it reads (task 24), and is refused any this build
/// doesn't compute.
pub static FEATURE_NAMES: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
    let mut names: Vec<String> = GLOBAL
        .iter()
        .chain(&KINDS)
        .chain(&TOPOLOGY)
        .map(|s| s.to_string())
        .collect();
    for m in METRICS {
        names.push(format!("{m}_below"));
        names.push(format!("{m}_outside"));
        names.push(format!("{m}_delta"));
    }
    names.extend(OWN.iter().map(|s| s.to_string()));
    names.extend(AS_BUILT.iter().map(|s| s.to_string()));
    names
});

/// What a candidate adds to the task 24 columns.
#[derive(Default)]
struct Extra {
    fittings: Option<f64>,
    port_dev: Option<f64>,
    home_dev: Option<f64>,
}

/// The mean of an active's ports against their set points, dB, where it reports them.
fn port_dev(active: Option<&features::extract::ActiveFeatures>) -> Option<f64> {
    let ports = &active?.port_level_drift_db;
    (!ports.is_empty()).then(|| ports.iter().map(|p| p.drift_db).sum::<f64>() / ports.len() as f64)
}

fn affected_like(modem: &ModemFeatures) -> bool {
    !modem.operational
        || modem.level_drift_db.is_some_and(|d| d.abs() > 1.5)
        || modem.us_tx_drift_db.is_some_and(|d| d.abs() > 1.5)
        || modem.mer_drift_db.is_some_and(|d| d < -1.5)
        || modem.preeq_echo_rise_db.is_some_and(|d| d > 6.0)
        || modem.uncorrectable_rate.is_some_and(|r| r > 1e-4)
}

/// The rows for one plant at one step, in canonical order: healthy first, then by kind and id.
pub fn component_rows(graph: &PlantGraph<'_>, features: &StepFeatures) -> Vec<ComponentRow> {
    let by_id: BTreeMap<&ElementId, &ModemFeatures> =
        features.modems.iter().map(|m| (&m.element_id, m)).collect();
    let n = features.modems.len().max(1) as f64;
    let affected: BTreeSet<&ElementId> = features
        .modems
        .iter()
        .filter(|m| affected_like(m))
        .map(|m| &m.element_id)
        .collect();
    let group = &features.service_group;
    let flag = |b: bool| Some(if b { 1.0 } else { 0.0 });
    let rpd = features.rpd.as_ref();
    let v_drift_min = features
        .actives
        .iter()
        .filter_map(|a| a.v_drift)
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.min(v)))
        });
    let global: Vec<Option<f64>> = vec![
        Some(n.ln()),
        Some(group.modems_down_share),
        group.us_snr_drift_min_db.filter(|v| v.is_finite()),
        Some(group.us_uncorrectable_rate.max(1e-9).log10()),
        rpd.map_or(Some(0.0), |r| {
            flag(r.top_level_state != RpdState::Operational)
        }),
        rpd.map_or(Some(0.0), |r| flag(r.gcp_status != GcpStatus::Connected)),
        rpd.map_or(Some(0.0), |r| flag(r.ptp_result != PtpResult::Synchronized)),
        rpd.map_or(Some(0.0), |r| flag(!r.rf_output_up)),
        rpd.map_or(Some(0.0), |r| flag(!r.drawing_power)),
        Some(affected.len() as f64 / n),
        v_drift_min,
    ];
    let echo_ft = {
        let mut v: Vec<f64> = features
            .modems
            .iter()
            .filter(|m| m.preeq_echo_rise_db.is_some_and(|r| r > 3.0))
            .filter_map(|m| m.preeq_echo_ft)
            .collect();
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied()
    };

    let fittings_of = |edge: &schema::common::EdgeId| {
        graph
            .fittings(edge)
            .ok()
            .map(|f| f.len() as f64)
            .filter(|n| *n > 0.0)
    };
    let mut rows = Vec::new();
    let ripple_drift = |m: &ModemFeatures| m.chan_est_ripple_drift_db;
    let mut push = |component: String,
                    kind: ComponentKind,
                    below: &BTreeSet<&ElementId>,
                    depth: f64,
                    len_ft: Option<f64>,
                    own: Vec<Option<f64>>,
                    extra: Extra| {
        let mut x = global.clone();
        for k in [
            ComponentKind::Healthy,
            ComponentKind::Span,
            ComponentKind::Amp,
            ComponentKind::Drop,
            ComponentKind::Supply,
            ComponentKind::Root,
        ] {
            x.push(flag(k == kind));
        }
        let both = below.intersection(&affected).count() as f64;
        let either = below.union(&affected).count() as f64;
        x.push(Some(below.len() as f64 / n));
        x.push(Some(if either > 0.0 { both / either } else { 0.0 }));
        x.push(Some(depth));
        x.push(len_ft.map(|l| l.max(1.0).ln()));
        x.push(match (echo_ft, len_ft) {
            (Some(e), Some(l)) if l > 0.0 => Some((e / l).min(10.0)),
            _ => None,
        });
        for index in 0..METRICS.len() {
            let inside = mean(
                below
                    .iter()
                    .filter_map(|id| by_id.get(*id).and_then(|m| metric(m, index))),
            );
            let outside = mean(
                features
                    .modems
                    .iter()
                    .filter(|m| !below.contains(&m.element_id))
                    .filter_map(|m| metric(m, index)),
            );
            x.push(inside);
            x.push(outside);
            x.push(inside.zip(outside).map(|(a, b)| a - b));
        }
        x.extend(own);
        x.push(flag(kind == ComponentKind::Premises));
        x.push(flag(kind == ComponentKind::DropCable));
        x.push(extra.fittings.map(f64::ln_1p));
        x.push(group.node_rx_drift_db);
        x.push(group.hub_rx_drift_db);
        x.push(extra.port_dev);
        x.push(extra.home_dev);
        let inside = mean(
            below
                .iter()
                .filter_map(|id| by_id.get(*id).and_then(|m| ripple_drift(m))),
        );
        let outside = mean(
            features
                .modems
                .iter()
                .filter(|m| !below.contains(&m.element_id))
                .filter_map(ripple_drift),
        );
        x.push(inside);
        x.push(outside);
        x.push(inside.zip(outside).map(|(a, b)| a - b));
        rows.push(ComponentRow { component, kind, x });
    };
    let no_own = || vec![None; OWN.len()];

    push(
        String::new(),
        ComponentKind::Healthy,
        &BTreeSet::new(),
        0.0,
        None,
        no_own(),
        Extra::default(),
    );

    let all: BTreeSet<&ElementId> = by_id.keys().copied().collect();
    let root = graph.root();
    let root_active = features.actives.iter().find(|a| a.element_id == root.id);
    let mut root_own = no_own();
    if let Some(a) = root_active {
        root_own[0] = a.out_level_drift_db;
        root_own[1] = a.v_drift;
        root_own[2] = flag(a.silent);
    }
    push(
        root.id.to_string(),
        ComponentKind::Root,
        &all,
        0.0,
        None,
        root_own,
        Extra {
            port_dev: port_dev(root_active),
            ..Extra::default()
        },
    );

    let ids_below = |location: &Location| -> BTreeSet<&ElementId> {
        graph
            .modems_below(location)
            .map(|list| {
                list.iter()
                    .filter_map(|m| by_id.get(&m.id).map(|f| &f.element_id))
                    .collect()
            })
            .unwrap_or_default()
    };

    let mut edges: Vec<_> = graph.plant().edges.iter().collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    for edge in edges {
        if let EdgeKind::Span { len_ft, .. } = edge.kind {
            let below = ids_below(&Location::Span {
                span_id: edge.id.clone(),
                pos_ft: 0.0,
            });
            let depth = graph.cascade(&edge.to.element_id).map_or(0, |c| c.len()) as f64;
            // The port feeding it, against its set point: a fault on the span doesn't move what the port puts out.
            let feeding = features
                .actives
                .iter()
                .find(|a| a.element_id == edge.from.element_id)
                .and_then(|a| {
                    a.port_level_drift_db
                        .iter()
                        .find(|p| p.port == edge.from.port)
                })
                .map(|p| p.drift_db);
            push(
                edge.id.to_string(),
                ComponentKind::Span,
                &below,
                depth,
                Some(len_ft),
                no_own(),
                Extra {
                    fittings: fittings_of(&edge.id),
                    port_dev: feeding,
                    ..Extra::default()
                },
            );
        }
    }
    for amp in graph.amps() {
        let Some(input) = amp.ports.iter().find(|p| p.role == PortRole::Input) else {
            continue;
        };
        let below = ids_below(&Location::Port {
            element_id: amp.id.clone(),
            port: input.id.clone(),
        });
        let mut own = no_own();
        let reported = features.actives.iter().find(|a| a.element_id == amp.id);
        if let Some(a) = reported {
            own[0] = a.out_level_drift_db;
            own[1] = a.v_drift;
            own[2] = flag(a.silent);
        }
        let depth = graph.cascade(&amp.id).map_or(0, |c| c.len()) as f64;
        push(
            amp.id.to_string(),
            ComponentKind::Amp,
            &below,
            depth,
            None,
            own,
            Extra {
                port_dev: port_dev(reported),
                ..Extra::default()
            },
        );
    }
    for modem in &features.modems {
        let mut own = no_own();
        own[3] = modem.sibling_level_dev_db;
        own[4] = modem.sibling_us_tx_dev_db;
        own[5] = modem.level_drift_db;
        own[6] = modem.us_tx_drift_db;
        let below: BTreeSet<&ElementId> = [&modem.element_id].into_iter().collect();
        let len_ft =
            graph
                .parent(&modem.element_id)
                .ok()
                .flatten()
                .and_then(|(_, edge)| match edge.kind {
                    EdgeKind::Drop { len_ft, .. } => Some(len_ft),
                    _ => None,
                });
        push(
            modem.element_id.to_string(),
            ComponentKind::Drop,
            &below,
            modem.cascade as f64,
            len_ft,
            own,
            Extra {
                home_dev: modem.home_level_dev_db,
                ..Extra::default()
            },
        );
    }
    for supply in &graph.plant().power_supplies {
        let mut own = no_own();
        own[1] = v_drift_min;
        push(
            supply.id.to_string(),
            ComponentKind::Supply,
            &all,
            0.0,
            None,
            own,
            Extra::default(),
        );
    }

    // Version 2 plant (task 24): each home's parts and in-home cable, and each drop that records its fittings. A plant
    // whose homes are a modem on a drop with no fittings has none of these, so its rows are as they were.
    let home_dev = |below: &BTreeSet<&ElementId>| {
        mean(
            below
                .iter()
                .filter_map(|id| by_id.get(*id).and_then(|m| m.home_level_dev_db)),
        )
    };
    let mut drops: Vec<_> = graph
        .plant()
        .edges
        .iter()
        .filter(|e| matches!(e.kind, EdgeKind::Drop { .. }))
        .collect();
    drops.sort_by(|a, b| a.id.cmp(&b.id));
    for drop in drops {
        let Some(home) = graph.premises_of(&drop.to.element_id).ok().flatten() else {
            continue;
        };
        let depth = graph.cascade(&drop.to.element_id).map_or(0, |c| c.len()) as f64;
        let fittings = fittings_of(&drop.id);
        if fittings.is_some_and(|n| n > 0.0) {
            let below = ids_below(&Location::Span {
                span_id: drop.id.clone(),
                pos_ft: 0.0,
            });
            push(
                drop.id.to_string(),
                ComponentKind::DropCable,
                &below,
                depth,
                drop.kind.cable().map(|(_, len_ft)| len_ft),
                no_own(),
                Extra {
                    fittings,
                    home_dev: home_dev(&below),
                    ..Extra::default()
                },
            );
        }
        let mut parts: Vec<_> = home
            .elements
            .iter()
            .filter(|e| !matches!(e.kind, ElementKind::Modem | ElementKind::Cpe))
            .collect();
        parts.sort_by(|a, b| a.id.cmp(&b.id));
        for part in parts {
            let Some(input) = part.ports.iter().find(|p| p.role == PortRole::Input) else {
                continue;
            };
            let below = ids_below(&Location::Port {
                element_id: part.id.clone(),
                port: input.id.clone(),
            });
            push(
                part.id.to_string(),
                ComponentKind::Premises,
                &below,
                depth,
                None,
                no_own(),
                Extra {
                    home_dev: home_dev(&below),
                    ..Extra::default()
                },
            );
        }
        let mut cables: Vec<_> = home.house_edges.clone();
        cables.sort_by(|a, b| a.id.cmp(&b.id));
        for cable in cables {
            let below = ids_below(&Location::Span {
                span_id: cable.id.clone(),
                pos_ft: 0.0,
            });
            push(
                cable.id.to_string(),
                ComponentKind::Premises,
                &below,
                depth,
                cable.kind.cable().map(|(_, len_ft)| len_ft),
                no_own(),
                Extra {
                    fittings: fittings_of(&cable.id),
                    home_dev: home_dev(&below),
                    ..Extra::default()
                },
            );
        }
    }
    rows
}

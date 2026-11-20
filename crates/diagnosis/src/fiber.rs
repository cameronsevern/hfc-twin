//! The region's fiber, read from every node's optics (task 24): the cable, closure or optic that best explains what the
//! nodes' links report, each candidate simulated through `sim::optics` and compared with every node's reading, or "not
//! fiber" where the affected nodes share nothing on the fiber or no one fault there fits.

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{ComponentId, FiberCableId, FiberElementId, ServiceGroupId, Timestamp};
use schema::component::{ComponentLibrary, ComponentSpec};
use schema::fault::Onset;
use schema::fiber::{FiberElementKind, FiberPlant, Hop};
use schema::fiber_fault::{FiberFault, FiberFaultKind, FiberLocation};
use schema::observation::OpticsObservation;
use schema::plant::Plant;
use schema::profile::ChannelPlan;
use schema::work_order::CauseClass;
use serde::{Deserialize, Serialize};
use sim::PhysicsError;
use sim::optics::{NodeLink, region_links};

/// A received power this far off its design value marks a node's link as affected, dB: the measurement noise of a
/// receiver's power monitor is a fraction of this.
const AFFECTED_DB: f64 = 1.0;
/// The spread a power reading is fitted with, dB.
const SIGMA_DB: f64 = 0.5;
/// A link up where it should be down, or down where it should be up, costs this much in the fit.
const STATE_MISS: f64 = 100.0;
/// The best candidate's fit, per affected node, over which no one fault on the fiber explains the readings.
const UNEXPLAINED_PER_NODE: f64 = 25.0;

/// One node's link as its devices report it: received power each way, and whether each way is up.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LinkReading {
    /// Into the node, dBm, or `None` with no light.
    pub forward_dbm: Option<f64>,
    /// Into the hub, dBm, or `None` with no light.
    pub return_dbm: Option<f64>,
    /// The forward way is up.
    pub forward_up: bool,
    /// The return way is up.
    pub return_up: bool,
}

impl LinkReading {
    /// What a node's link would report, as the sim computes it.
    pub fn of(link: &NodeLink) -> LinkReading {
        let way = |l: Option<&sim::optics::LinkResult>| {
            (l.and_then(|x| x.received_dbm), l.is_some_and(|x| x.up))
        };
        let (forward_dbm, forward_up) = way(link.forward.as_ref());
        let (return_dbm, return_up) = way(link.ret.as_ref());
        LinkReading {
            forward_dbm,
            return_dbm,
            forward_up,
            return_up,
        }
    }

    /// What a node's optics observation says (§16): the RPD's optic where it reports one, else the two receivers.
    pub fn from_observation(o: &OpticsObservation) -> LinkReading {
        match &o.rpd_optic {
            Some(optic) => LinkReading {
                forward_dbm: optic.rx_dbm,
                return_dbm: o.hub_rx_dbm,
                forward_up: optic.link_up,
                return_up: optic.link_up,
            },
            None => LinkReading {
                forward_dbm: o.node_rx_dbm,
                return_dbm: o.hub_rx_dbm,
                forward_up: o.node_rx_dbm.is_some(),
                return_up: o.hub_rx_dbm.is_some(),
            },
        }
    }
}

/// What the fiber reading concludes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum FiberVerdict {
    /// Every node's link reads as designed.
    Healthy,
    /// Some nodes' links read wrong, and they share nothing outside the hub, or no one fault on the fiber explains them.
    NotFiber {
        /// The nodes whose links read wrong.
        affected: Vec<ServiceGroupId>,
    },
    /// One fault on the fiber explains them.
    Fiber {
        /// The fault, as simulated.
        fault: FiberFault,
        /// Its cause class.
        class: CauseClass,
        /// The cable or element it's on.
        place: String,
        /// The nodes whose links read wrong.
        affected: Vec<ServiceGroupId>,
    },
}

/// The verdict, and every candidate tried, best first, with its fit (lower is better).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FiberAnswer {
    /// The conclusion.
    pub verdict: FiberVerdict,
    /// Every candidate's place, class and fit, best first.
    pub ranked: Vec<(String, CauseClass, f64)>,
}

/// The cables and the elements outside the hub a node's links pass through, and its transmitters.
fn path_of(
    fiber: &FiberPlant,
    sg: &ServiceGroupId,
) -> (BTreeSet<FiberCableId>, BTreeSet<FiberElementId>) {
    let mut cables = BTreeSet::new();
    let mut elements = BTreeSet::new();
    for link in fiber.links.iter().filter(|l| &l.service_group == sg) {
        for hop in &link.hops {
            let at = match hop {
                Hop::Fiber { cable, .. } => {
                    cables.insert(cable.clone());
                    continue;
                }
                Hop::Splice { at, .. } | Hop::Connector { at, .. } => at,
                Hop::Through { element, .. } => element,
            };
            elements.insert(at.clone());
        }
    }
    (cables, elements)
}

/// Whether two readings differ enough to call the second off the first.
fn off(design: &LinkReading, seen: &LinkReading) -> bool {
    let power = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() > AFFECTED_DB,
        (None, None) => false,
        _ => true,
    };
    design.forward_up != seen.forward_up
        || design.return_up != seen.return_up
        || power(design.forward_dbm, seen.forward_dbm)
        || power(design.return_dbm, seen.return_dbm)
}

/// How far a predicted reading is from a seen one.
fn cost(predicted: &LinkReading, seen: &LinkReading) -> f64 {
    let mut c = 0.0;
    for (p_up, s_up, p, s) in [
        (
            predicted.forward_up,
            seen.forward_up,
            predicted.forward_dbm,
            seen.forward_dbm,
        ),
        (
            predicted.return_up,
            seen.return_up,
            predicted.return_dbm,
            seen.return_dbm,
        ),
    ] {
        if p_up != s_up {
            c += STATE_MISS;
        }
        match (p, s) {
            (Some(p), Some(s)) => c += ((p - s) / SIGMA_DB).powi(2),
            (None, None) => {}
            _ => c += STATE_MISS,
        }
    }
    c
}

/// The fiber reading for a region: `observed` per node, against what its links should read.
pub fn diagnose_fiber(
    fiber: &FiberPlant,
    nodes: &[(&Plant, &ChannelPlan)],
    library: &ComponentLibrary,
    observed: &BTreeMap<ServiceGroupId, LinkReading>,
) -> Result<FiberAnswer, PhysicsError> {
    let t = Timestamp(0);
    let readings =
        |faults: &[FiberFault]| -> Result<BTreeMap<ServiceGroupId, LinkReading>, PhysicsError> {
            Ok(region_links(fiber, nodes, library, faults, t)?
                .iter()
                .map(|(sg, link)| (sg.clone(), LinkReading::of(link)))
                .collect())
        };
    let design = readings(&[])?;
    let affected: Vec<ServiceGroupId> = observed
        .iter()
        .filter(|(sg, seen)| design.get(*sg).is_none_or(|d| off(d, seen)))
        .map(|(sg, _)| sg.clone())
        .collect();
    if affected.is_empty() {
        return Ok(FiberAnswer {
            verdict: FiberVerdict::Healthy,
            ranked: Vec::new(),
        });
    }
    // What the affected nodes share outside the hub: a fault on the fiber that takes them all passes there.
    let hub_side = |id: &FiberElementId| {
        fiber
            .elements
            .iter()
            .any(|e| &e.id == id && e.hub.is_some())
    };
    let paths: Vec<(BTreeSet<FiberCableId>, BTreeSet<FiberElementId>)> =
        affected.iter().map(|sg| path_of(fiber, sg)).collect();
    let shared_cables: BTreeSet<FiberCableId> = paths
        .iter()
        .map(|p| p.0.clone())
        .reduce(|a, b| a.intersection(&b).cloned().collect())
        .unwrap_or_default();
    let shared_elements: BTreeSet<FiberElementId> = paths
        .iter()
        .map(|p| {
            p.1.iter()
                .filter(|e| !hub_side(e))
                .cloned()
                .collect::<BTreeSet<_>>()
        })
        .reduce(|a, b| a.intersection(&b).cloned().collect())
        .unwrap_or_default();
    if affected.len() > 1 && shared_cables.is_empty() && shared_elements.is_empty() {
        return Ok(FiberAnswer {
            verdict: FiberVerdict::NotFiber { affected },
            ranked: Vec::new(),
        });
    }
    // How much light went missing, as a start for a loss or a drift: the median shortfall over the affected nodes' lit ways.
    let mut short: Vec<f64> = Vec::new();
    for sg in &affected {
        if let (Some(d), Some(s)) = (design.get(sg), observed.get(sg)) {
            for (a, b) in [(d.forward_dbm, s.forward_dbm), (d.return_dbm, s.return_dbm)] {
                if let (Some(a), Some(b)) = (a, b) {
                    short.push(a - b);
                }
            }
        }
    }
    short.sort_by(f64::total_cmp);
    let shortfall = short.get(short.len() / 2).copied().unwrap_or(3.0).max(0.5);

    // Candidates: on every cable and element the affected nodes' links pass, outside the hub but for its transmitters.
    let instant = |kind: FiberFaultKind| FiberFault {
        onset: Onset::Instant,
        kind,
    };
    let mut candidates: Vec<(FiberFault, String)> = Vec::new();
    let cables: BTreeSet<&FiberCableId> = paths.iter().flat_map(|p| p.0.iter()).collect();
    for cable in cables {
        let len = fiber
            .cables
            .iter()
            .find(|c| &c.id == cable)
            .map_or(0.0, |c| c.len_ft);
        candidates.push((
            instant(FiberFaultKind::FiberCut {
                cable: cable.clone(),
                pos_ft: len * 0.5,
                repair_s: None,
            }),
            cable.to_string(),
        ));
        candidates.push((
            instant(FiberFaultKind::EventLoss {
                location: FiberLocation::Cable {
                    cable: cable.clone(),
                    pos_ft: len * 0.5,
                },
                loss_db: shortfall,
            }),
            cable.to_string(),
        ));
    }
    let elements: BTreeSet<&FiberElementId> = paths.iter().flat_map(|p| p.1.iter()).collect();
    for id in elements {
        let Some(element) = fiber.elements.iter().find(|e| &e.id == id) else {
            continue;
        };
        match element.kind {
            FiberElementKind::SpliceClosure
            | FiberElementKind::PatchPanel
            | FiberElementKind::OpticalPassive
                if element.hub.is_none() || element.kind == FiberElementKind::PatchPanel =>
            {
                candidates.push((
                    instant(FiberFaultKind::EventLoss {
                        location: FiberLocation::Element {
                            element: id.clone(),
                        },
                        loss_db: shortfall,
                    }),
                    id.to_string(),
                ));
            }
            FiberElementKind::OpticalTx | FiberElementKind::Transceiver
                if emits(library, &element.component_ref) =>
            {
                candidates.push((
                    instant(FiberFaultKind::TransmitterDrift {
                        element: id.clone(),
                        power_delta_db: -shortfall,
                        rin_delta_db: 0.0,
                    }),
                    id.to_string(),
                ));
            }
            _ => {}
        }
    }
    let mut ranked: Vec<(FiberFault, String, f64)> = Vec::new();
    for (fault, place) in candidates {
        let predicted = readings(std::slice::from_ref(&fault))?;
        let fit: f64 = observed
            .iter()
            .map(|(sg, seen)| predicted.get(sg).map_or(STATE_MISS, |p| cost(p, seen)))
            .sum();
        ranked.push((fault, place, fit));
    }
    ranked.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.1.cmp(&b.1)));
    let listing: Vec<(String, CauseClass, f64)> = ranked
        .iter()
        .map(|(f, p, c)| (p.clone(), CauseClass::from(&f.kind), *c))
        .collect();
    let verdict = match ranked.first() {
        Some((fault, place, fit)) if *fit <= UNEXPLAINED_PER_NODE * affected.len() as f64 => {
            FiberVerdict::Fiber {
                fault: fault.clone(),
                class: CauseClass::from(&fault.kind),
                place: place.clone(),
                affected,
            }
        }
        _ => FiberVerdict::NotFiber { affected },
    };
    Ok(FiberAnswer {
        verdict,
        ranked: listing,
    })
}

/// Whether a part sends light: a transmitter, or a pluggable optic.
fn emits(library: &ComponentLibrary, part: &ComponentId) -> bool {
    library.get(part).is_some_and(|c| {
        matches!(
            c.spec,
            ComponentSpec::OpticalTx(_) | ComponentSpec::Transceiver(_)
        )
    })
}

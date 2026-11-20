//! The version 2 designer (`docs/tasks/23-as-built-designer.md`): streets, homes, a hub site and a design-rules file in, a
//! region a field designer would recognize out. Each stage is its own module and a pure function of what's above it, and
//! every rule's value comes from `DesignRules`. Classical and deterministic, with no AI. Everything it emits is `Designed`
//! and `Synthetic`.
//!
//! Version 1 (`crate::optimize`) is untouched and stays the path when no rules are given (`docs/as-built-plan.md`,
//! rule 7).

pub mod structures;
pub mod upgrade;

pub mod assemble;
pub mod backbone;
pub mod bars;
pub mod cost;
pub mod drops;
pub mod feeder;
pub mod fiber;
pub mod fittings;
pub mod kit;
pub mod levels;
pub mod nodes;
pub mod pipeline;
pub mod power;

use thiserror::Error;

/// What can stop a version 2 design.
#[derive(Error, Debug)]
pub enum DesignError {
    /// A part the rules name isn't in the library, or doesn't qualify where it's needed.
    #[error("no usable part: {0}")]
    MissingPart(String),
    /// The rules don't validate, or lack what the designer reads.
    #[error("design rules: {0}")]
    Rules(String),
    /// A plant it built doesn't validate, which is a bug here.
    #[error("built an invalid plant: {0}")]
    InvalidPlant(String),
    /// The sim refused it.
    #[error(transparent)]
    Physics(#[from] sim::PhysicsError),
    /// The map has no streets to route on, or none reaches the hub.
    #[error("the map has no streets the hub reaches")]
    NoStreets,
}

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{ComponentId, Origin, ServiceGroupId};
use schema::component::ComponentLibrary;
use schema::design_rules::DesignRules;
use schema::fiber::FiberPlant;
use schema::plant::{Plant, Structure};
use schema::profile::Profile;
use schema::region::{DesignControls, Region, RegionNode, StreetMap};
use serde::{Deserialize, Serialize};

use cost::CostReport;
use kit::Kit;
use pipeline::{NodePlan, place_structures, plan_nodes};
use structures::Structures;

/// One node of a version 2 design.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeV2 {
    /// The plant, every element `Designed`.
    pub plant: Plant,
    /// Homes it passes.
    pub homes_passed: u32,
    /// What "No issues" found on it.
    pub issues: Vec<String>,
    /// Where a stage couldn't hold a rule, in words.
    pub notes: Vec<String>,
    /// The premises template each home with service got, by home id.
    pub templates: Vec<(String, String)>,
    /// The runs its coax follows, by index in the map's structure layout.
    pub runs: Vec<usize>,
}

/// One candidate's result, reported so the choice can be checked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    /// Its express cable.
    pub express_cable: ComponentId,
    /// Its step from the node count the target gives.
    pub step: i32,
    /// Nodes built.
    pub nodes: usize,
    /// Its cost, US dollars, nodes over the ceiling left out.
    pub usd: f64,
    /// Per home passed.
    pub usd_per_home: f64,
    /// Issues on the nodes it builds.
    pub issues: usize,
    /// Whether it's the one kept.
    pub kept: bool,
}

/// Homes a coax design would cost too much for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverCeiling {
    /// The homes, by id.
    pub homes: Vec<String>,
    /// What coax would cost per home passed, US dollars.
    pub usd_per_home: f64,
}

/// A version 2 design.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DesignV2 {
    /// The region, with its fiber plant named.
    pub region: Region,
    /// Each node built.
    pub nodes: Vec<NodeV2>,
    /// The fiber plant.
    pub fiber: FiberPlant,
    /// Every structure any plant or the fiber uses, once.
    pub structures: Vec<Structure>,
    /// Homes no street within a drop reaches, or no street joins to the hub, by id.
    pub unserved: Vec<String>,
    /// Homes coax isn't the answer for, by node area.
    pub over_ceiling: Vec<OverCeiling>,
    /// The kept design's cost.
    pub cost: CostReport,
    /// Every candidate's cost and issues.
    pub candidates: Vec<Candidate>,
    /// The rules it was designed by.
    pub rules_id: String,
    /// In the second mode, what the upgrade from the legacy design did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<upgrade::UpgradeReport>,
}

/// One candidate, built and judged.
struct Built {
    plans: Vec<NodePlan>,
    fiber: fiber::FiberDesign,
    cost: CostReport,
    region: Region,
    issues: BTreeMap<String, Vec<String>>,
    unserved: Vec<usize>,
    over: Vec<OverCeiling>,
    /// Tap ports outside the rules' tolerance of their target, at either end of the temperature range ("Tap-port level").
    port_misses: usize,
}

fn build_candidate(
    kit: &Kit<'_>,
    s: &Structures,
    map: &StreetMap,
    controls: &DesignControls,
    step: i32,
) -> Result<Built, DesignError> {
    let (mut plans, unserved) = plan_nodes(kit, s, map, controls, step)?;
    let region_id = format!("region-{}", map.id);
    let ceiling = kit
        .rules
        .costs
        .ceiling_usd_per_home
        .as_ref()
        .map_or(f64::INFINITY, |c| c.value);
    let mut over = Vec::new();
    loop {
        let pairs: Vec<(Plant, usize)> = plans
            .iter()
            .map(|p| (p.plant.clone(), p.area.site))
            .collect();
        let fd = fiber::fiber(kit, s, map.headend, &region_id, &pairs)?;
        let homes: Vec<u32> = plans.iter().map(|p| p.area.homes.len() as u32).collect();
        let runs: Vec<BTreeSet<usize>> = plans.iter().map(|p| p.runs.clone()).collect();
        let rows: Vec<(&Plant, u32, &BTreeSet<usize>)> = fd
            .plants
            .iter()
            .zip(&homes)
            .zip(&runs)
            .map(|((p, h), r)| (p, *h, r))
            .collect();
        let report = cost::cost(kit, s, &rows, &fd.fiber, &fd.runs);
        // §DESIGN, "When coax isn't the answer": nodes over the ceiling aren't built; their homes are said so.
        let costly: BTreeSet<ServiceGroupId> = report
            .nodes
            .iter()
            .filter(|n| n.usd_per_home > ceiling)
            .map(|n| n.service_group.clone())
            .collect();
        if !costly.is_empty() && controls.build_over_ceiling {
            // Built all the same, as the controls ask, and still said so for them and their homes.
            for plan in &plans {
                if let Some(n) = report.nodes.iter().find(|n| {
                    n.service_group == plan.plant.service_group && costly.contains(&n.service_group)
                }) {
                    over.push(OverCeiling {
                        homes: plan
                            .area
                            .homes
                            .iter()
                            .map(|h| map.homes[*h].id.clone())
                            .collect(),
                        usd_per_home: n.usd_per_home,
                    });
                }
            }
        } else if !costly.is_empty() {
            let mut keep = Vec::new();
            for plan in plans {
                if costly.contains(&plan.plant.service_group) {
                    let usd = report
                        .nodes
                        .iter()
                        .find(|n| n.service_group == plan.plant.service_group)
                        .map_or(0.0, |n| n.usd_per_home);
                    over.push(OverCeiling {
                        homes: plan
                            .area
                            .homes
                            .iter()
                            .map(|h| map.homes[*h].id.clone())
                            .collect(),
                        usd_per_home: usd,
                    });
                } else {
                    keep.push(plan);
                }
            }
            plans = keep;
            continue;
        }
        let region = Region {
            schema_version: schema::region::SCHEMA_VERSION.to_string(),
            id: region_id.clone(),
            map_id: map.id.clone(),
            origin: Origin::Synthetic,
            nodes: fd
                .plants
                .iter()
                .zip(&plans)
                .map(|(p, plan)| RegionNode {
                    service_group: p.service_group.clone(),
                    profile_ref: p.profile_ref.clone(),
                    homes_passed: plan.area.homes.len() as u32,
                })
                .collect(),
            fiber_ref: Some(fd.fiber.id.clone()),
        };
        let design = bars::Design {
            kit,
            region: &region,
            plants: &fd.plants,
            fiber: &fd.fiber,
        };
        let mut issues = bars::issues(&design);
        // A cascade whose response is outside N/10 + 3 dB [SCTE 183 §6.2] is a defect of the design too, on its node,
        // so a candidate with one isn't kept while another passes: the walk judges each amp on its own input, and the
        // sim reads what the cascade adds up to.
        for failure in bars::alignment_failures(&design) {
            let sg: String = failure.split('-').take(2).collect::<Vec<_>>().join("-");
            issues
                .entry(sg)
                .or_default()
                .push(format!("Alignment: {failure}"));
        }
        let port_misses = bars::tap_port_level(&design).failed;
        return Ok(Built {
            plans,
            fiber: fd,
            cost: report,
            region,
            issues,
            unserved,
            over,
            port_misses,
        });
    }
}

/// Designs a map under rules (`docs/tasks/23-as-built-designer.md`): every candidate in the rules' fixed set, any with
/// an issue dropped, the cheapest kept. Every element is `Designed` and the region `Synthetic`.
pub fn design(
    map: &StreetMap,
    profile: &Profile,
    library: &ComponentLibrary,
    rules: &DesignRules,
    controls: &DesignControls,
) -> Result<DesignV2, DesignError> {
    if map.streets.is_empty() {
        return Err(DesignError::NoStreets);
    }
    let base = Kit::new(rules, library, profile, None)?;
    let s = place_structures(&base, map, controls)?;
    let mut cables: Vec<ComponentId> = vec![base.express.part.id.clone()];
    for c in &base.complete.candidates.express_cables.value {
        if !cables.contains(c) {
            cables.push(c.clone());
        }
    }
    let steps = base.complete.candidates.node_count_steps.value.clone();
    let mut tried: Vec<(Candidate, Built)> = Vec::new();
    for cable in &cables {
        let kit = Kit::new(rules, library, profile, Some(cable))?;
        for &step in &steps {
            let built = build_candidate(&kit, &s, map, controls, step)?;
            let issues: usize = built.issues.values().map(Vec::len).sum();
            tried.push((
                Candidate {
                    express_cable: cable.clone(),
                    step,
                    nodes: built.plans.len(),
                    usd: built.cost.total_usd,
                    usd_per_home: built.cost.usd_per_home,
                    issues,
                    kept: false,
                },
                built,
            ));
        }
    }
    // The fewest issues, then the fewest homes left unbuilt, over the ceiling or unserved, then the fewest tap ports off
    // their target, then the cheapest (task 25): a candidate that leaves homes out is cheaper for it, and isn't better.
    let left_out = |b: &Built| -> usize {
        b.unserved.len()
            + if controls.build_over_ceiling {
                0
            } else {
                b.over.iter().map(|o| o.homes.len()).sum::<usize>()
            }
    };
    let best = tried
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            a.0.issues
                .cmp(&b.0.issues)
                .then(left_out(&a.1).cmp(&left_out(&b.1)))
                .then(a.1.port_misses.cmp(&b.1.port_misses))
                .then(a.0.usd.total_cmp(&b.0.usd))
        })
        .map(|(i, _)| i)
        .ok_or(DesignError::NoStreets)?;
    tried[best].0.kept = true;
    let candidates: Vec<Candidate> = tried.iter().map(|(c, _)| c.clone()).collect();
    let (_, built) = tried.swap_remove(best);
    let nodes: Vec<NodeV2> = built
        .fiber
        .plants
        .iter()
        .zip(&built.plans)
        .map(|(plant, plan)| NodeV2 {
            plant: plant.clone(),
            homes_passed: plan.area.homes.len() as u32,
            issues: built
                .issues
                .get(plant.service_group.as_str())
                .cloned()
                .unwrap_or_default(),
            notes: plan
                .notes
                .iter()
                .cloned()
                .chain(
                    built
                        .fiber
                        .choices
                        .get(&plant.service_group)
                        .and_then(|c| c.as_ref().err())
                        .map(|why| format!("no optics fit: {why}")),
                )
                .collect(),
            templates: plan.templates.clone(),
            runs: plan.runs.iter().copied().collect(),
        })
        .collect();
    let mut used: BTreeSet<usize> = built.fiber.sites.clone();
    for plan in &built.plans {
        used.extend(plan.sites.iter().copied());
    }
    Ok(DesignV2 {
        region: built.region,
        nodes,
        fiber: built.fiber.fiber,
        structures: s.records(used),
        unserved: built
            .unserved
            .iter()
            .map(|h| map.homes[*h].id.clone())
            .collect(),
        over_ceiling: built.over,
        cost: built.cost,
        candidates,
        rules_id: rules.id.clone(),
        upgrade: None,
    })
}

/// The second mode: designs the map under the legacy rules and profile, then upgrades every node in place to the target
/// profile under the target rules' moves (`upgrade`), with the fiber's equipment and links chosen again over the same
/// route. What still fails stays in each node's issues.
pub fn design_upgraded(
    map: &StreetMap,
    profile: &Profile,
    library: &ComponentLibrary,
    rules: &DesignRules,
    legacy_profile: &Profile,
    legacy_rules: &DesignRules,
    controls: &DesignControls,
) -> Result<DesignV2, DesignError> {
    let legacy = design(map, legacy_profile, library, legacy_rules, controls)?;
    let legacy_kit = Kit::new(legacy_rules, library, legacy_profile, None)?;
    let s = place_structures(&legacy_kit, map, controls)?;
    let kit = Kit::new(rules, library, profile, None)?;
    let allowed: Vec<schema::design_rules::UpgradeMove> = rules
        .upgrade
        .iter()
        .flat_map(|u| u.moves.iter().map(|m| m.value))
        .collect();
    let mut report = upgrade::UpgradeReport {
        legacy_rules_id: legacy_rules.id.clone(),
        legacy_profile: legacy_profile.id.to_string(),
        moves: allowed.clone(),
        ..Default::default()
    };
    let mut upgraded = Vec::new();
    for node in &legacy.nodes {
        let (plant, changes, stuck) = upgrade::upgrade_plant(&kit, &node.plant, &allowed)?;
        report.changes.insert(plant.service_group.clone(), changes);
        if !stuck.is_empty() {
            report.stuck.insert(plant.service_group.clone(), stuck);
        }
        let site = plant
            .root()
            .and_then(|r| r.structure.clone())
            .and_then(|id| s.sites.iter().position(|x| x.id == id))
            .ok_or_else(|| {
                DesignError::InvalidPlant(format!(
                    "{}: its root has no structure",
                    plant.service_group
                ))
            })?;
        upgraded.push((plant, site));
    }
    let fd = fiber::fiber(&kit, &s, map.headend, &legacy.region.id, &upgraded)?;
    let mut region = legacy.region.clone();
    for n in &mut region.nodes {
        n.profile_ref = profile.id.clone();
    }
    region.fiber_ref = Some(fd.fiber.id.clone());
    let issues = bars::issues(&bars::Design {
        kit: &kit,
        region: &region,
        plants: &fd.plants,
        fiber: &fd.fiber,
    });
    let runs: Vec<BTreeSet<usize>> = legacy
        .nodes
        .iter()
        .map(|n| n.runs.iter().copied().collect())
        .collect();
    let rows: Vec<(&Plant, u32, &BTreeSet<usize>)> = fd
        .plants
        .iter()
        .zip(&legacy.nodes)
        .zip(&runs)
        .map(|((p, n), r)| (p, n.homes_passed, r))
        .collect();
    let report_cost = cost::cost(&kit, &s, &rows, &fd.fiber, &fd.runs);
    let nodes: Vec<NodeV2> = fd
        .plants
        .iter()
        .zip(&legacy.nodes)
        .map(|(plant, old)| NodeV2 {
            plant: plant.clone(),
            homes_passed: old.homes_passed,
            issues: issues
                .get(plant.service_group.as_str())
                .cloned()
                .unwrap_or_default(),
            notes: old.notes.clone(),
            templates: old.templates.clone(),
            runs: old.runs.clone(),
        })
        .collect();
    let mut used: BTreeSet<usize> = fd.sites.clone();
    for n in &nodes {
        for st in &n.plant.structures {
            if let Some(i) = s.sites.iter().position(|x| x.id == st.id) {
                used.insert(i);
            }
        }
    }
    Ok(DesignV2 {
        region,
        nodes,
        fiber: fd.fiber,
        structures: s.records(used),
        unserved: legacy.unserved.clone(),
        over_ceiling: legacy.over_ceiling.clone(),
        cost: report_cost,
        candidates: legacy.candidates.clone(),
        rules_id: rules.id.clone(),
        upgrade: Some(report),
    })
}

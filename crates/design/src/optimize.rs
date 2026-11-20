//! The whole design: partition, route, place, check, repair. Deterministic. The objective is the fewest actives and
//! the least cable with every constraint met, and this greedy pass makes no claim to be optimal.

use std::collections::BTreeMap;

use plant_graph::PlantGraph;
use schema::common::{ComponentId, ElementId, LonLat, Origin, ServiceGroupId, Timestamp};
use schema::component::{Component, ComponentClass, ComponentLibrary, ComponentSpec};
use schema::environment::EnvironmentSample;
use schema::plant::{EdgeKind, ElementKind, Plant, PowerSupply};
use schema::profile::{NodeKind, Profile};
use schema::region::{DesignControls, Region, RegionNode, StreetMap};
use serde::{Deserialize, Serialize};
use sim::Overlay;
use sim::ac::solve_ac;
use sim::budget::{PhysicsIssue, check_physics};
use thiserror::Error;

use crate::partition::{Group, bisect, partition, partition_with_pins};
use crate::place::{Parts, PlaceError, place};
use crate::route::{StreetGraph, steiner_tree};

/// Which parts a design uses, by id. Defaults pick the generic library's; step 9's real parts swap in here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DesignParts {
    /// Hardline.
    pub cable: ComponentId,
    /// Drop.
    pub drop: ComponentId,
    /// Amp.
    pub amp: ComponentId,
    /// Splitter.
    pub splitter: ComponentId,
    /// Terminator.
    pub terminator: ComponentId,
    /// Supply.
    pub supply: ComponentId,
    /// D3.0 modem.
    pub modem_d30: ComponentId,
    /// D3.1 modem.
    pub modem_d31: ComponentId,
}

impl Default for DesignParts {
    fn default() -> Self {
        DesignParts {
            cable: ComponentId::new("cable-500-p3"),
            drop: ComponentId::new("drop-rg6"),
            amp: ComponentId::new("amp-generic-1218"),
            splitter: ComponentId::new("splitter-generic-2way"),
            terminator: ComponentId::new("term-generic"),
            supply: ComponentId::new("psu-generic-90v"),
            modem_d30: ComponentId::new("modem-generic-d30"),
            modem_d31: ComponentId::new("modem-generic-d31"),
        }
    }
}

/// What can stop a design.
#[derive(Error, Debug)]
pub enum DesignError {
    /// A part it needs isn't in the library, or doesn't qualify.
    #[error("no usable part: {0}")]
    MissingPart(String),
    /// A plant it built doesn't validate, which is a bug here.
    #[error("built an invalid plant: {0}")]
    InvalidPlant(String),
    /// The sim refused it.
    #[error(transparent)]
    Physics(#[from] sim::PhysicsError),
    /// The map has no streets to route on.
    #[error("the map has no streets")]
    NoStreets,
}

/// One node's result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeDesign {
    /// The plant, with every element `Designed`.
    pub plant: Plant,
    /// Homes it passes.
    pub homes_passed: u32,
    /// What `check_design` found.
    pub topology_issues: Vec<String>,
    /// What `check_physics` found.
    pub physics_issues: Vec<String>,
    /// Amps.
    pub amps: u32,
    /// Hardline, feet.
    pub hardline_ft: f64,
    /// Deepest cascade.
    pub max_cascade: u32,
}

/// A whole region.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegionDesign {
    /// The region's record.
    pub region: Region,
    /// Each node.
    pub nodes: Vec<NodeDesign>,
    /// Homes no street reaches, by id.
    pub unserved: Vec<String>,
}

/// This designer is version 1's, frozen so the layouts the shipped models trained on don't move
/// (`docs/as-built-plan.md`, rule 7). It places 4-port through taps, which were all the library held when it was
/// written, so a tap of another size, a terminating tap or one with a version 2 field is never its choice.
pub fn version_1_tap(component: &Component) -> bool {
    matches!(&component.spec, ComponentSpec::Tap(t) if t.ports == 4 && !t.terminating)
        && component.version_2_fields().is_empty()
}

fn part<'a>(library: &'a ComponentLibrary, id: &ComponentId) -> Result<&'a Component, DesignError> {
    library
        .get(id)
        .ok_or_else(|| DesignError::MissingPart(id.to_string()))
}

fn resolve<'a>(
    library: &'a ComponentLibrary,
    profile: &Profile,
    chosen: &DesignParts,
) -> Result<Parts<'a>, DesignError> {
    let root_class = match profile.node_kind {
        NodeKind::Analog => ComponentClass::Node,
        NodeKind::Rpd => ComponentClass::Rpd,
    };
    let root = library
        .components
        .iter()
        .find(|c| c.class() == root_class && schema::component::qualifies(c, profile))
        .ok_or_else(|| DesignError::MissingPart(format!("{root_class:?}")))?;
    let mut taps: Vec<&Component> = library
        .components
        .iter()
        .filter(|c| {
            // An outdoor plant's power runs through its taps (§6.2), so one that blocks it can't go in line.
            let passes = matches!(&c.spec, ComponentSpec::Tap(t) if t.power_passing);
            passes && version_1_tap(c) && schema::component::qualifies(c, profile)
        })
        .collect();
    let value = |c: &&Component| match &c.spec {
        ComponentSpec::Tap(t) => t.tap_value_db,
        _ => 0.0,
    };
    taps.sort_by(|a, b| value(b).total_cmp(&value(a)).then(a.id.cmp(&b.id)));
    if taps.is_empty() {
        return Err(DesignError::MissingPart("Tap".into()));
    }
    Ok(Parts {
        root,
        amp: part(library, &chosen.amp)?,
        taps,
        splitter: part(library, &chosen.splitter)?,
        terminator: part(library, &chosen.terminator)?,
        cable: part(library, &chosen.cable)?,
        drop: part(library, &chosen.drop)?,
        modem_d30: part(library, &chosen.modem_d30)?,
        modem_d31: part(library, &chosen.modem_d31)?,
        supply: part(library, &chosen.supply)?,
    })
}

/// §6.3: a supply where voltage falls short. A span into the worst amp stops passing power, and a new supply feeds it.
fn repair_power(
    plant: &mut Plant,
    library: &ComponentLibrary,
    profile: &Profile,
    supply: &ComponentId,
) -> Result<(), DesignError> {
    for round in 0..16 {
        let graph = PlantGraph::new(plant, library, profile)
            .map_err(|e| DesignError::InvalidPlant(format!("{e:?}")))?;
        let solved = solve_ac(&graph, &Overlay::default())?;
        let overloaded: Vec<ElementId> = solved
            .supplies
            .iter()
            .filter(|(_, s)| s.i_total > 0.8 * s.i_rated)
            .map(|(id, _)| id.clone())
            .collect();
        // Margin for design: below 5 V above minimum, it won't ride a temperature swing.
        let low: Vec<(ElementId, f64)> = solved
            .actives
            .iter()
            .filter(|(_, a)| a.v_in < a.v_min + 5.0 || overloaded.contains(&a.supply))
            .map(|(id, a)| (id.clone(), a.v_in))
            .collect();
        let Some((worst, _)) = low
            .iter()
            .filter(|(id, _)| graph.parent(id).ok().flatten().is_some())
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .cloned()
        else {
            return Ok(());
        };
        // Pick an amp halfway along when one supply's overloaded, or the worst one otherwise.
        let edge_id = graph
            .parent(&worst)
            .ok()
            .flatten()
            .map(|(_, edge)| edge.id.clone());
        let geo = graph.element(&worst).ok().and_then(|e| e.geo);
        drop(graph);
        let Some(edge_id) = edge_id else {
            return Ok(());
        };
        for edge in &mut plant.edges {
            if edge.id == edge_id
                && let EdgeKind::Span { power_passing, .. } = &mut edge.kind
            {
                *power_passing = false;
            }
        }
        plant.power_supplies.push(PowerSupply {
            id: ElementId::new(format!("{}-psu-{}", plant.service_group, round + 2)),
            component_ref: supply.clone(),
            feeds: worst,
            geo,
            batteries: None,
            record_status: None,
            structure: None,
        });
    }
    Ok(())
}

fn at(temp_c: f64) -> EnvironmentSample {
    EnvironmentSample {
        t: Timestamp(0),
        temp_c,
        ground_temp_c: temp_c,
        rain_mm_per_hr: 0.0,
        solar_w_per_m2: None,
        wind_m_per_s: None,
    }
}

fn describe(
    plant: Plant,
    homes_passed: u32,
    library: &ComponentLibrary,
    profile: &Profile,
    max_drop_ft: f64,
) -> Result<NodeDesign, DesignError> {
    let graph = PlantGraph::new(&plant, library, profile)
        .map_err(|e| DesignError::InvalidPlant(format!("{e:?}")))?;
    let mut topology_issues: Vec<String> = graph
        .check_design()
        .iter()
        .map(|i| format!("{i:?}"))
        .collect();
    for edge in &plant.edges {
        if let EdgeKind::Drop { len_ft, .. } = edge.kind
            && len_ft > max_drop_ft
        {
            topology_issues.push(format!(
                "DropTooLong {{ edge: {}, len_ft: {len_ft:.0} }}",
                edge.id
            ));
        }
    }
    let issues: Vec<PhysicsIssue> =
        check_physics(&graph, &[at(-20.0), at(45.0)], &Overlay::default())?;
    let physics_issues = issues.iter().map(|i| format!("{i:?}")).collect();
    let amps = graph.amps().len() as u32;
    let mut max_cascade = 0;
    for modem in graph.modems() {
        max_cascade = max_cascade.max(graph.cascade(&modem.id).map_or(0, |c| c.len()) as u32);
    }
    let hardline_ft = plant
        .edges
        .iter()
        .map(|e| match e.kind {
            EdgeKind::Span { len_ft, .. } => len_ft,
            _ => 0.0,
        })
        .sum();
    drop(graph);
    Ok(NodeDesign {
        plant,
        homes_passed,
        topology_issues,
        physics_issues,
        amps,
        hardline_ft,
        max_cascade,
    })
}

fn centroid(map: &StreetMap, group: &Group) -> LonLat {
    let n = group.len().max(1) as f64;
    LonLat {
        lon_deg: group.iter().map(|&h| map.homes[h].geo.lon_deg).sum::<f64>() / n,
        lat_deg: group.iter().map(|&h| map.homes[h].geo.lat_deg).sum::<f64>() / n,
    }
}

/// Designs a whole map. Every element is `Designed`, the whole region is `Synthetic`, and whatever a node couldn't
/// meet is in its issues rather than hidden.
pub fn design_region(
    map: &StreetMap,
    profile: &Profile,
    library: &ComponentLibrary,
    controls: &DesignControls,
    parts: &DesignParts,
) -> Result<RegionDesign, DesignError> {
    if map.streets.is_empty() {
        return Err(DesignError::NoStreets);
    }
    let resolved = resolve(library, profile, parts)?;
    let target = (controls.homes_per_node.min(profile.max_homes_passed) as usize).max(1);
    let graph = StreetGraph::build(&map.streets, &map.homes, controls.max_drop_ft);
    let unserved: Vec<String> = map
        .homes
        .iter()
        .enumerate()
        .filter(|(h, _)| graph.home_vertex[*h].is_none())
        .map(|(_, home)| home.id.clone())
        .collect();

    let pins: Vec<LonLat> = controls.pinned_nodes.iter().map(|p| p.geo).collect();
    let (pinned_groups, rest) = partition_with_pins(&map.homes, &pins, target);
    let rest_homes: Vec<schema::region::Home> =
        rest.iter().map(|&h| map.homes[h].clone()).collect();
    let mut work: Vec<(Group, Option<LonLat>)> = pinned_groups
        .into_iter()
        .zip(pins.iter().copied())
        .map(|(g, pin)| (g, Some(pin)))
        .collect();
    for group in partition(&rest_homes, target) {
        work.push((group.into_iter().map(|i| rest[i]).collect(), None));
    }
    // Numbered in a stable order: pins first, then by each group's first home.
    work.sort_by(|a, b| {
        b.1.is_some()
            .cmp(&a.1.is_some())
            .then_with(|| a.0.iter().min().cmp(&b.0.iter().min()))
    });
    work.reverse();

    let mut nodes = Vec::new();
    let mut number = 0;
    while let Some((group, pin)) = work.pop() {
        let served: Vec<usize> = group
            .iter()
            .copied()
            .filter(|h| graph.home_vertex[*h].is_some())
            .collect();
        if served.is_empty() {
            continue;
        }
        let site = pin.unwrap_or_else(|| centroid(map, &served));
        let Some(mut root_vertex) = graph.nearest_vertex(site) else {
            continue;
        };
        let terminals: Vec<usize> = served
            .iter()
            .filter_map(|h| graph.home_vertex[*h])
            .collect();
        let mut tree = steiner_tree(&graph, root_vertex, &terminals);
        // Cable follows streets, so a home whose street doesn't connect to the node's can't be on this node. Those go to a node of
        // their own, with the site moved if none of the group connects to where it was.
        if terminals.iter().all(|t| tree.unreached.contains(t)) {
            root_vertex = terminals[0];
            tree = steiner_tree(&graph, root_vertex, &terminals);
        }
        let (served, apart): (Vec<usize>, Vec<usize>) = served
            .into_iter()
            .partition(|h| graph.home_vertex[*h].is_some_and(|v| !tree.unreached.contains(&v)));
        if !apart.is_empty() {
            work.push((apart, None));
        }
        let id = ServiceGroupId::new(format!("sg-{number}"));
        match place(
            &id,
            profile,
            library,
            &resolved,
            &graph,
            &tree,
            root_vertex,
            &map.homes,
            &served,
            controls,
        ) {
            Ok(mut plant) => {
                repair_power(&mut plant, library, profile, &resolved.supply.id)?;
                nodes.push(describe(
                    plant,
                    served.len() as u32,
                    library,
                    profile,
                    controls.max_drop_ft,
                )?);
                number += 1;
            }
            Err(PlaceError::CascadeExceeded) if served.len() > 1 && pin.is_none() => {
                // §DESIGN: a node splits at its cascade limit.
                let (a, b) = bisect(&map.homes, &served);
                work.push((b, None));
                work.push((a, None));
            }
            Err(PlaceError::CascadeExceeded) => {
                return Err(DesignError::MissingPart(
                    "a route that fits the cascade limit".into(),
                ));
            }
            Err(PlaceError::BadPart(part)) => {
                return Err(DesignError::MissingPart(part.to_string()));
            }
        }
    }

    let region = Region {
        schema_version: schema::region::SCHEMA_VERSION.to_string(),
        id: format!("region-{}", map.id),
        map_id: map.id.clone(),
        origin: Origin::Synthetic,
        nodes: nodes
            .iter()
            .map(|n: &NodeDesign| RegionNode {
                service_group: n.plant.service_group.clone(),
                profile_ref: n.plant.profile_ref.clone(),
                homes_passed: n.homes_passed,
            })
            .collect(),
        fiber_ref: None,
    };
    Ok(RegionDesign {
        region,
        nodes,
        unserved,
    })
}

/// What's been built so far: flips every element to `Built`.
pub fn mark_all_built(plant: &mut Plant) {
    for element in &mut plant.elements {
        element.record_status = schema::plant::RecordStatus::Built;
    }
}

/// How many of each record status a plant has.
pub fn record_counts(plant: &Plant) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    for element in &plant.elements {
        if !matches!(element.kind, ElementKind::Modem) {
            *out.entry(format!("{:?}", element.record_status))
                .or_insert(0) += 1;
        }
    }
    out
}

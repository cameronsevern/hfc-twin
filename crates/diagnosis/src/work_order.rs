//! How to fix it: the operator's policy table, never a model (`docs/DESIGN.md`). A work order comes from
//! a template, so a table edit changes it with no code or retrain.

use plant_graph::PlantGraph;
use schema::common::{ElementId, LonLat, Timestamp};
use schema::component::ComponentClass;
use schema::fault::{Fault, FaultKind, Location};
use schema::plant::Element;
use schema::work_order::{CauseClass, PolicyEntry, PolicyTable, WorkOrder, WorkOrderStatus};

use crate::engine::Hypothesis;

fn interpolate(a: Option<LonLat>, b: Option<LonLat>, share: f64) -> Option<LonLat> {
    let (a, b) = (a?, b?);
    Some(LonLat {
        lon_deg: a.lon_deg + (b.lon_deg - a.lon_deg) * share,
        lat_deg: a.lat_deg + (b.lat_deg - a.lat_deg) * share,
    })
}

/// Where a location is: the nearest element and a pin on the map.
pub fn locate(graph: &PlantGraph<'_>, location: &Location) -> Option<(ElementId, Option<LonLat>)> {
    match location {
        Location::Port { element_id, .. } => {
            let element = graph.element(element_id).ok()?;
            Some((element.id.clone(), element.geo))
        }
        Location::Span { span_id, pos_ft } => {
            let edge = graph.edge(span_id).ok()?;
            let from = graph.element(&edge.from.element_id).ok()?;
            let to = graph.element(&edge.to.element_id).ok()?;
            let len_ft = edge.kind.cable().map_or(1.0, |(_, len_ft)| len_ft);
            let share = (pos_ft / len_ft.max(1.0)).clamp(0.0, 1.0);
            let nearest = if share < 0.5 { from } else { to };
            Some((nearest.id.clone(), interpolate(from.geo, to.geo, share)))
        }
    }
}

fn fault_place(graph: &PlantGraph<'_>, kind: &FaultKind) -> Option<(ElementId, Option<LonLat>)> {
    match kind {
        FaultKind::WaterIngress {
            span_id,
            pos_ft,
            affected_len_ft,
            ..
        } => locate(
            graph,
            &Location::Span {
                span_id: span_id.clone(),
                pos_ft: pos_ft + affected_len_ft / 2.0,
            },
        ),
        FaultKind::ConnectorReflection { location, .. }
        | FaultKind::Suckout { location, .. }
        | FaultKind::FrequencySelectiveLoss { location, .. }
        | FaultKind::CommonPathDistortion { location, .. } => locate(graph, location),
        FaultKind::UpstreamIngress { entry_location, .. }
        | FaultKind::DownstreamIngress { entry_location, .. } => locate(graph, entry_location),
        FaultKind::AmpDrift { amp_id, .. }
        | FaultKind::ResonantPeak { amp_id, .. }
        | FaultKind::Misalignment {
            active_id: amp_id, ..
        } => {
            let element: &Element = graph.element(amp_id).ok()?;
            Some((element.id.clone(), element.geo))
        }
        FaultKind::BadDrop { modem_id, .. } => {
            let element: &Element = graph.element(modem_id).ok()?;
            Some((element.id.clone(), element.geo))
        }
        FaultKind::AdjacentLevelOffset { .. } | FaultKind::ForwardOptics { .. } => {
            let element = graph.root();
            Some((element.id.clone(), element.geo))
        }
        FaultKind::RpdFailure { rpd_id, .. }
        | FaultKind::CinLinkCut { rpd_id, .. }
        | FaultKind::GcpLoss { rpd_id, .. }
        | FaultKind::TimingLoss { rpd_id, .. }
        | FaultKind::RfmMisalignment { rpd_id, .. } => {
            let element: &Element = graph.element(rpd_id).ok()?;
            Some((element.id.clone(), element.geo))
        }
        FaultKind::SpanCut {
            span_id, pos_ft, ..
        }
        | FaultKind::PowerShort {
            span_id, pos_ft, ..
        } => locate(
            graph,
            &Location::Span {
                span_id: span_id.clone(),
                pos_ft: *pos_ft,
            },
        ),
        FaultKind::AnalogNodeFailure {
            node_id: element_id,
            ..
        }
        | FaultKind::WaterInDevice { element_id, .. }
        | FaultKind::DamagedDevice { element_id, .. } => {
            let element: &Element = graph.element(element_id).ok()?;
            Some((element.id.clone(), element.geo))
        }
        // The utility's, not the plant's: there's nowhere on the plant to send anyone.
        FaultKind::UtilityOutage { .. } => None,
        FaultKind::PowerSupplySag { psu_id, .. } => {
            let supply = graph
                .plant()
                .power_supplies
                .iter()
                .find(|p| &p.id == psu_id)?;
            Some((supply.id.clone(), supply.geo))
        }
    }
}

fn component_class(kind: &FaultKind) -> Option<ComponentClass> {
    Some(match kind {
        FaultKind::WaterIngress { .. } => ComponentClass::Cable,
        FaultKind::AmpDrift { .. } | FaultKind::ResonantPeak { .. } => ComponentClass::Amp,
        FaultKind::PowerSupplySag { .. } => ComponentClass::PowerSupply,
        FaultKind::BadDrop { .. } => ComponentClass::Drop,
        FaultKind::RpdFailure { .. }
        | FaultKind::CinLinkCut { .. }
        | FaultKind::GcpLoss { .. }
        | FaultKind::TimingLoss { .. }
        | FaultKind::RfmMisalignment { .. } => ComponentClass::Rpd,
        _ => return None,
    })
}

/// The first entry matching the cause and component class, then the cause alone, then `Unknown`.
pub fn policy_entry(
    policy: &PolicyTable,
    cause: CauseClass,
    class: Option<ComponentClass>,
) -> Option<&PolicyEntry> {
    policy
        .entries
        .iter()
        .find(|e| e.cause == cause && e.component_class == class)
        .or_else(|| policy.entries.iter().find(|e| e.cause == cause))
        .or_else(|| {
            policy
                .entries
                .iter()
                .find(|e| e.cause == CauseClass::Unknown)
        })
}

/// One proposed work order per fault in a hypothesis. Healthy gets none.
pub fn work_order(
    graph: &PlantGraph<'_>,
    hypothesis: &Hypothesis,
    observed_affected: u32,
    policy: &PolicyTable,
    t: Timestamp,
) -> Vec<WorkOrder> {
    let mut out = Vec::new();
    for fault in &hypothesis.faults {
        out.extend(one_order(
            graph,
            hypothesis,
            fault,
            observed_affected,
            policy,
            t,
        ));
    }
    out
}

fn one_order(
    graph: &PlantGraph<'_>,
    hypothesis: &Hypothesis,
    fault: &Fault,
    observed_affected: u32,
    policy: &PolicyTable,
    t: Timestamp,
) -> Option<WorkOrder> {
    let cause = features::sampler::class_in(graph, &fault.kind);
    let (element_id, geo) = fault_place(graph, &fault.kind)?;
    let entry = policy_entry(policy, cause, component_class(&fault.kind))?;
    let service_group = graph.plant().service_group.clone();
    let explanation = format!(
        "{} on {}: {} modems affected. This is simulated telemetry under that fault, at χ²/dof {:.2}, \
         beside what was seen; posterior {:.0} %.",
        cause.name(),
        features::sampler::fault_element(&fault.kind).unwrap_or_default(),
        observed_affected,
        hypothesis.chi2 / hypothesis.dof as f64,
        hypothesis.posterior * 100.0,
    );
    Some(WorkOrder {
        schema_version: schema::work_order::SCHEMA_VERSION.to_string(),
        id: WorkOrder::derive_id(&service_group, t, &element_id, cause),
        service_group,
        t,
        element_id,
        geo,
        cause,
        parameters: fault.clone(),
        confidence: hypothesis.posterior as f32,
        affected_modems: observed_affected,
        action: entry.action.clone(),
        parts: entry.parts.clone(),
        explanation,
        status: WorkOrderStatus::Proposed,
    })
}

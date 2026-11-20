//! §11: loss between any two points, in either direction, over the same device and cable functions
//! the walk uses. Passive only: an active's gain depends on what's above it, which only the walk
//! knows. So the path's actives are reported, not priced.

use plant_graph::{PlantGraph, Segment};
use schema::common::ElementId;
use schema::component::ComponentSpec;
use schema::environment::EnvironmentSample;
use schema::fault::Location;
use schema::plant::{Edge, EdgeKind};

use crate::cable::{Exposure, cable_loss_db, delay_us, exposure_temp_c};
use crate::error::PhysicsError;
use crate::passive::{check_rated, device_loss_db};

/// What a path costs at one frequency.
#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    /// Cable and passive loss along the path, dB.
    pub loss_db: f64,
    /// One-way cable delay, µs.
    pub delay_us: f64,
    /// Actives crossed, in path order.
    pub actives: Vec<ElementId>,
}

/// The cable on an edge, and what sets its temperature (§1.2, §13).
pub(crate) fn edge_cable<'a>(
    graph: &PlantGraph<'a>,
    edge: &'a Edge,
) -> Result<Option<(&'a schema::component::Component, Exposure)>, PhysicsError> {
    let (cable_ref, exposure) = match &edge.kind {
        EdgeKind::Span {
            cable_ref,
            installation,
            ..
        } => (cable_ref, Exposure::of(Some(*installation))),
        EdgeKind::Drop {
            cable_ref,
            installation,
            ..
        } => (cable_ref, Exposure::of(*installation)),
        EdgeKind::House { cable_ref, .. } => (cable_ref, Exposure::Indoor),
        EdgeKind::Direct => return Ok(None),
    };
    let component = graph
        .component(cable_ref)
        .ok_or_else(|| PhysicsError::BadComponent(edge.from.element_id.clone()))?;
    Ok(Some((component, exposure)))
}

/// `transfer(a, b, f_mhz)` at one environment sample (§11).
pub fn transfer(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    a: &Location,
    b: &Location,
    f_mhz: f64,
) -> Result<Transfer, PhysicsError> {
    crate::support::check_supported(graph)?;
    let path = graph.path_between(a, b)?;
    let mut out = Transfer {
        loss_db: 0.0,
        delay_us: 0.0,
        actives: Vec::new(),
    };
    for segment in &path.segments {
        match segment {
            Segment::Cable { edge, len_ft, .. } => {
                if let Some((component, exposure)) = edge_cable(graph, edge)? {
                    check_rated(component, f_mhz)?;
                    let spec = match &component.spec {
                        ComponentSpec::Cable(spec) | ComponentSpec::Drop(spec) => spec,
                        _ => return Err(PhysicsError::BadComponent(edge.from.element_id.clone())),
                    };
                    let temp_c = exposure_temp_c(sample, exposure);
                    out.loss_db += cable_loss_db(spec, f_mhz, *len_ft, temp_c);
                    out.delay_us += delay_us(spec, *len_ft);
                    // §14: a path over a whole edge crosses its fittings. Over part of one, which fittings it crosses
                    // depends on where it starts, which a segment doesn't say, so it takes none (approx).
                    let whole = edge
                        .kind
                        .cable()
                        .is_some_and(|(_, l)| (l - len_ft).abs() < 1e-9);
                    if whole {
                        for fitting in edge.kind.fittings() {
                            if let Some(ComponentSpec::Connector(c)) =
                                graph.component(&fitting.component_ref).map(|c| &c.spec)
                            {
                                out.loss_db += c.insertion_loss_db;
                            }
                        }
                    }
                }
            }
            Segment::Direct { .. } => {}
            Segment::Device {
                element,
                in_port,
                out_port,
            } => {
                let component = graph
                    .component(&element.component_ref)
                    .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?;
                if component.spec.active().is_some()
                    || matches!(component.spec, ComponentSpec::DropAmp(_))
                {
                    out.actives.push(element.id.clone());
                } else {
                    out.loss_db += device_loss_db(component, element, in_port, out_port, f_mhz)?;
                    // §15: a tap's faceplate conditioner, on the path in or out of a tap port.
                    let tap_port = in_port.role == schema::plant::PortRole::TapPort
                        || out_port.role == schema::plant::PortRole::TapPort;
                    if tap_port
                        && let Some(schema::plant::Fitted::Passive(passive)) = &element.fitted
                        && let Some(ComponentSpec::PlugIn(spec)) = passive
                            .conditioner
                            .as_ref()
                            .and_then(|id| graph.component(id))
                            .map(|c| &c.spec)
                    {
                        out.loss_db += crate::passive::plugin_loss_db(spec, f_mhz);
                    }
                }
            }
        }
    }
    Ok(out)
}

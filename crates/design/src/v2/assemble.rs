//! One area's plant from the stages: the walk's RF plant, the drops and homes, the port states, and the structures
//! everything stands at or passes.

use schema::common::{Origin, ServiceGroupId};
use schema::plant::{Fitted, PassiveFitted, Plant};
use schema::region::{DesignControls, StreetMap};

use super::backbone::{Overflow, Rf, build};
use super::drops::{Premises, drops};
use super::kit::Kit;
use super::nodes::Area;
use super::structures::Structures;

/// An area's plant before powering, alignment and optics, with what the stages noted.
#[derive(Debug, Clone)]
pub struct Built {
    /// The plant.
    pub plant: Plant,
    /// The walk's own record.
    pub rf: Rf,
    /// The homes.
    pub premises: Premises,
}

/// Builds an area's plant: RF, then drops and homes, with every tap port given a state.
pub fn node_plant(
    kit: &Kit<'_>,
    structures: &Structures,
    map: &StreetMap,
    area: &Area,
    controls: &DesignControls,
    service_group: &ServiceGroupId,
) -> Result<Built, Overflow> {
    let rf = build(kit, structures, area, service_group)?;
    let premises = drops(
        kit,
        structures,
        &map.homes,
        &rf.taps,
        controls,
        service_group.as_str(),
    );
    let mut elements = rf.elements.clone();
    for element in &mut elements {
        if let Some(states) = premises.port_states.get(&element.id) {
            match &mut element.fitted {
                Some(Fitted::Passive(p)) => p.ports.extend(states.iter().cloned()),
                _ => {
                    element.fitted = Some(Fitted::Passive(PassiveFitted {
                        conditioner: None,
                        ports: states.clone(),
                    }))
                }
            }
            if let Some(Fitted::Passive(p)) = &mut element.fitted {
                p.ports.sort_by(|a, b| {
                    let n = |s: &str| s.trim_start_matches("tap-").parse::<u32>().unwrap_or(0);
                    n(a.port.as_str()).cmp(&n(b.port.as_str()))
                });
            }
        }
    }
    elements.extend(premises.elements.iter().cloned());
    let mut edges = rf.edges.clone();
    edges.extend(premises.edges.iter().cloned());
    let plant = Plant {
        schema_version: schema::plant::SCHEMA_VERSION.to_string(),
        service_group: service_group.clone(),
        profile_ref: kit.profile.id.clone(),
        origin: Origin::Synthetic,
        elements,
        edges,
        power_supplies: Vec::new(),
        modem_bindings: Vec::new(),
        structures: structures.records(rf.sites.iter().copied()),
    };
    Ok(Built {
        plant,
        rf,
        premises,
    })
}

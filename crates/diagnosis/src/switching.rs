//! §7.3: modem data doesn't localize upstream ingress. The method is to attenuate legs one at a time
//! and watch the node's noise. This plans that and reads the answers, so a technician or the emulator
//! can supply the measurement.

use plant_graph::PlantGraph;
use schema::common::ElementId;
use schema::fault::Location;
use schema::plant::{Plant, PortRole};

/// Attenuation a technician's switch applies, dB.
pub const SWITCH_ATTEN_DB: f64 = 20.0;
/// How far the noise must fall to say the ingress is below the switch, dB.
pub const NOISE_FALL_DB: f64 = 3.0;

/// The plant with one amp's return path attenuated: the input the switch test changes.
pub fn with_return_atten(plant: &Plant, amp: &ElementId) -> Plant {
    let mut out = plant.clone();
    for element in &mut out.elements {
        if &element.id == amp
            && let schema::plant::ElementKind::Amp { return_atten_db } = &mut element.kind
        {
            *return_atten_db += SWITCH_ATTEN_DB;
        }
    }
    out
}

/// Walks down from the root: tests each amp, deepest first among those the noise follows, and returns
/// the lowest amp whose attenuation lowers the noise, or `None` if it's above every amp. `measure`
/// returns the node's noise in dB with that amp's return path attenuated, or with none for `None`.
pub fn localize_by_switching(
    graph: &PlantGraph<'_>,
    mut measure: impl FnMut(Option<&ElementId>) -> Option<f64>,
) -> Option<ElementId> {
    let base = measure(None)?;
    let mut below: Option<ElementId> = None;
    let mut depth = 0;
    for amp in graph.amps() {
        let cascade = graph.cascade(&amp.id).ok()?.len();
        let reading = measure(Some(&amp.id))?;
        if base - reading > NOISE_FALL_DB && (below.is_none() || cascade >= depth) {
            below = Some(amp.id.clone());
            depth = cascade;
        }
    }
    below
}

/// The location that says "somewhere below this amp", for a work order's pin.
pub fn leg_location(graph: &PlantGraph<'_>, amp: &ElementId) -> Option<Location> {
    let element = graph.element(amp).ok()?;
    let port = element.ports.iter().find(|p| p.role == PortRole::Output)?;
    Some(Location::Port {
        element_id: element.id.clone(),
        port: port.id.clone(),
    })
}

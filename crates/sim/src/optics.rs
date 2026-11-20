//! §16: a node's optical links. From the fiber plant, the optics fitted on the plant's root, the library and the profile's
//! loading, each link's budget, its CNR or margin, whether it's up, and its latency. Pure: the same inputs give the same
//! link state, and nothing here reads time or draws.

use std::collections::BTreeMap;

use schema::common::{LinkId, ServiceGroupId, Timestamp};
use schema::component::{
    ComponentLibrary, ComponentSpec, FiberCableSpec, MerPoint, OpticalDirection, OpticalRxSpec,
    OpticalTxSpec, TransceiverSpec,
};
use schema::fault::Onset;
use schema::fiber::{FiberElementKind, FiberPlant, Hop, OpticalLink};
use schema::fiber_fault::{FiberFault, FiberFaultKind, FiberLocation, StrandRef};
use schema::plant::{Fitted, Plant};
use schema::profile::ChannelPlan;
use serde::{Deserialize, Serialize};

use crate::error::PhysicsError;
use crate::faults::onset_share;
use crate::rng::{Rng, hash_str};
use crate::units::{db, lin};

/// Fiber delay, µs per 1,000 ft: 500 µs per 100 km [TR-DCA §11.9.3], our conversion.
pub const FIBER_DELAY_US_PER_KFT: f64 = 1.524;
/// The bandwidth a CNR is stated in, Hz: 6 MHz, the channel a modulation index is stated per (§16).
pub const CNR_BANDWIDTH_HZ: f64 = 6.0e6;
/// The width of one channel in a loading count, MHz.
const CHANNEL_MHZ: f64 = 6.0;
/// The bandwidth SCTE 270's EDFA term is stated in, Hz [SCTE 270 §25.7.2].
const EDFA_REF_BANDWIDTH_HZ: f64 = 4.0e6;
/// Simpson panels for the clipping integral. Even.
const CLIP_PANELS: usize = 2000;

/// What one hop costs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct HopLoss {
    /// The hop, as `fiber cable/strand`, `splice at`, `connector at` or `through element`.
    pub hop: String,
    /// Its loss, dB, a fault's included: `None` where a cut stops all light. An optical amplifier's is negative: its gain.
    pub loss_db: Option<f64>,
    /// Power after it, dBm, or `None` with no light.
    pub power_dbm: Option<f64>,
}

/// §16's four noise terms and clipping, each in 6 MHz, dB.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AnalogCnr {
    /// The transmitter's relative intensity noise.
    pub rin_db: f64,
    /// Shot noise at the receiver.
    pub shot_db: f64,
    /// The receiver's thermal noise.
    pub thermal_db: f64,
    /// Each optical amplifier on the path, in hop order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edfa_db: Vec<f64>,
    /// Clipping, from the loaded channel plan. `None` where it's too small to represent.
    pub clip_db: Option<f64>,
    /// RMS modulation index of the loaded composite.
    pub rms_index: f64,
    /// All of them in linear.
    pub cnr_db: f64,
}

/// One link, one way.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LinkResult {
    /// The link.
    pub link: LinkId,
    /// Which way it's read here: forward to the node, or return from it.
    pub direction: OpticalDirection,
    /// Wavelength, nm.
    pub wavelength_nm: f64,
    /// Analog, or a digital optic at both ends.
    pub analog: bool,
    /// The transmitting part.
    pub transmitter: schema::common::ComponentId,
    /// The receiving part.
    pub receiver: schema::common::ComponentId,
    /// Launch power, dBm.
    pub launch_dbm: f64,
    /// Every hop, in the direction light travels.
    pub hops: Vec<HopLoss>,
    /// Launch less received, dB, or `None` with no light.
    pub loss_db: Option<f64>,
    /// Received power, dBm, or `None` with no light.
    pub received_dbm: Option<f64>,
    /// Fiber along the link, feet.
    pub fiber_ft: f64,
    /// One-way delay in the fiber, µs.
    pub latency_us: f64,
    /// An analog link's CNR, or a return link's SNR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cnr: Option<AnalogCnr>,
    /// A digital link's margin over the receiver's sensitivity, dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_db: Option<f64>,
    /// A digital link's margin below the receiver's overload, dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overload_margin_db: Option<f64>,
    /// Whether it carries signal.
    pub up: bool,
}

/// A node's links: the one in service each way. A second link the same way (path redundancy) isn't switched to.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct NodeLink {
    /// The node's service group.
    pub service_group: ServiceGroupId,
    /// Hub to node.
    pub forward: Option<LinkResult>,
    /// Node to hub.
    pub ret: Option<LinkResult>,
    /// §17: the source MER table of the CMTS the fiber plant binds the service group to. Read behind an analog node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cmts_source_mer: Vec<MerPoint>,
    /// §17: that CMTS's upstream receiver ceiling, dB. Read behind an analog node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmts_us_rx_ceiling_db: Option<f64>,
}

impl NodeLink {
    /// The analog forward link's CNR, if it has one and it's up.
    pub fn forward_cnr_db(&self) -> Option<f64> {
        self.forward
            .as_ref()
            .filter(|l| l.analog && l.up)
            .and_then(|l| l.cnr.as_ref())
            .map(|c| c.cnr_db)
    }

    /// The analog return link's SNR: minus infinity if it's down, `None` if there's no analog return.
    pub fn return_snr_db(&self) -> Option<f64> {
        let link = self.ret.as_ref().filter(|l| l.analog)?;
        if !link.up {
            return Some(f64::NEG_INFINITY);
        }
        link.cnr.as_ref().map(|c| c.cnr_db)
    }

    /// Whether an analog forward link is down, so the node has no RF.
    pub fn forward_dark(&self) -> bool {
        self.forward.as_ref().is_some_and(|l| l.analog && !l.up)
    }

    /// Whether a digital link either way is down, which cuts an RPD off as a `CinLinkCut` does.
    pub fn digital_down(&self) -> bool {
        [&self.forward, &self.ret]
            .into_iter()
            .flatten()
            .any(|l| !l.analog && !l.up)
    }
}

/// `ρ`: the power clipped off a Gaussian composite of RMS index `µ` at zero light, relative to the signal's (§16):
/// `φ(a)·∫₀^∞ t²·e^(−a·t − t²/2) dt` with `a = 1/µ`, by Simpson's rule over where the integrand is above e^−50.
pub fn clip_ratio(rms_index: f64) -> f64 {
    if rms_index <= 0.0 {
        return 0.0;
    }
    let a = 1.0 / rms_index;
    let phi = (-a * a / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
    if phi == 0.0 {
        return 0.0;
    }
    let top = -a + (a * a + 100.0).sqrt();
    let h = top / CLIP_PANELS as f64;
    let mut sum = 0.0;
    for k in 0..=CLIP_PANELS {
        let t = k as f64 * h;
        let weight = if k == 0 || k == CLIP_PANELS {
            1.0
        } else if k % 2 == 1 {
            4.0
        } else {
            2.0
        };
        sum += weight * t * t * (-a * t - t * t / 2.0).exp();
    }
    phi * sum * h / 3.0
}

/// §16's four terms [SCTE 270 §25.7.1-§25.7.4] for per-channel index `m` at received power `received_dbm`, in noise
/// bandwidth `bandwidth_hz`: (RIN, shot, thermal), dB.
pub fn noise_terms_db(
    m: f64,
    received_dbm: f64,
    rin_db_per_hz: f64,
    responsivity_a_per_w: f64,
    ein_pa_per_rthz: f64,
    bandwidth_hz: f64,
) -> (f64, f64, f64) {
    let m_rms_db = 20.0 * (m / 2f64.sqrt()).log10();
    let rin = 20.0 * m.log10() - db(2.0 * bandwidth_hz) - rin_db_per_hz;
    let shot = received_dbm + m_rms_db + db(responsivity_a_per_w) - db(bandwidth_hz) + 154.94;
    let thermal = 2.0 * received_dbm + m_rms_db + 20.0 * responsivity_a_per_w.log10()
        - db(bandwidth_hz)
        - 20.0 * ein_pa_per_rthz.log10()
        + 180.0;
    (rin, shot, thermal)
}

/// An optical amplifier's term [SCTE 270 §25.7.2], at input `in_dbm`, moved from 4 MHz to `bandwidth_hz`, dB.
pub fn edfa_term_db(m: f64, in_dbm: f64, nf_db: f64, bandwidth_hz: f64) -> f64 {
    86.2 + in_dbm + 20.0 * m.log10() - nf_db - db(bandwidth_hz / EDFA_REF_BANDWIDTH_HZ)
}

/// The whole analog link at per-channel index `m` with `channels` 6 MHz channels loaded.
pub fn analog_cnr(
    m: f64,
    channels: f64,
    received_dbm: f64,
    tx: &OpticalTxSpec,
    rx: &OpticalRxSpec,
    edfa_db: Vec<f64>,
) -> AnalogCnr {
    let (rin_db, shot_db, thermal_db) = noise_terms_db(
        m,
        received_dbm,
        tx.rin_db_per_hz,
        rx.responsivity_a_per_w,
        rx.ein_pa_per_rthz,
        CNR_BANDWIDTH_HZ,
    );
    let rms_index = m * (channels / 2.0).sqrt();
    let rho = clip_ratio(rms_index);
    let clip_db = (rho > 0.0).then(|| -db(rho));
    let inv = lin(-rin_db)
        + lin(-shot_db)
        + lin(-thermal_db)
        + edfa_db.iter().map(|e| lin(-e)).sum::<f64>()
        + rho;
    AnalogCnr {
        rin_db,
        shot_db,
        thermal_db,
        edfa_db,
        clip_db,
        rms_index,
        cnr_db: -db(inv),
    }
}

/// Loaded downstream bandwidth in 6 MHz channels: SC-QAM widths, and each OFDM channel's active span plus one subcarrier.
pub fn forward_channels(plan: &ChannelPlan) -> f64 {
    let sc: f64 = plan.ds_sc_qam.iter().map(|c| c.width_mhz).sum();
    let ofdm: f64 = plan
        .ofdm
        .iter()
        .map(|c| c.last_active_mhz - c.first_active_mhz + c.subcarrier_spacing_khz / 1000.0)
        .sum();
    (sc + ofdm) / CHANNEL_MHZ
}

/// Loaded upstream bandwidth in 6 MHz channels, as [`forward_channels`].
pub fn return_channels(plan: &ChannelPlan) -> f64 {
    let sc: f64 = plan.us_sc_qam.iter().map(|c| c.width_mhz).sum();
    let ofdma: f64 = plan
        .ofdma
        .iter()
        .map(|c| c.last_active_mhz - c.first_active_mhz + c.subcarrier_spacing_khz / 1000.0)
        .sum();
    (sc + ofdma) / CHANNEL_MHZ
}

/// A cable's loss at `wavelength_nm`, dB per 1,000 ft: linear between the wavelengths it states, held at the end ones
/// beyond them (approx).
pub fn fiber_loss_db_per_kft(spec: &FiberCableSpec, wavelength_nm: f64) -> Option<f64> {
    let mut points: Vec<(f64, f64)> = spec
        .loss
        .iter()
        .map(|p| (p.wavelength_nm, p.loss_db_per_kft))
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let first = points.first()?;
    let last = points.last()?;
    if wavelength_nm <= first.0 {
        return Some(first.1);
    }
    if wavelength_nm >= last.0 {
        return Some(last.1);
    }
    points.windows(2).find_map(|w| {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        (wavelength_nm >= x0 && wavelength_nm <= x1)
            .then(|| y0 + (y1 - y0) * (wavelength_nm - x0) / (x1 - x0))
    })
}

/// Either end of a link.
enum End<'l> {
    Tx(&'l OpticalTxSpec),
    Rx(&'l OpticalRxSpec),
    Optic(&'l TransceiverSpec),
}

fn invalid(message: String) -> PhysicsError {
    PhysicsError::InvalidPlant(message)
}

/// A part's spec as an end of a link.
fn end_of<'l>(
    library: &'l ComponentLibrary,
    part: &schema::common::ComponentId,
) -> Option<End<'l>> {
    match &library.get(part)?.spec {
        ComponentSpec::OpticalTx(spec) => Some(End::Tx(spec)),
        ComponentSpec::OpticalRx(spec) => Some(End::Rx(spec)),
        ComponentSpec::Transceiver(spec) => Some(End::Optic(spec)),
        _ => None,
    }
}

/// The hops in the order light travels this way, and the hub end's element.
fn oriented<'f>(
    fiber: &'f FiberPlant,
    link: &'f OpticalLink,
    toward_node: bool,
) -> Result<(Vec<&'f Hop>, &'f schema::fiber::FiberElement), PhysicsError> {
    let ends = (link.hops.first(), link.hops.last());
    let is_node = |hop: Option<&Hop>| {
        hop.and_then(Hop::element)
            .and_then(|id| fiber.element(id))
            .is_some_and(|e| matches!(e.kind, FiberElementKind::NodeEnd { .. }))
    };
    let node_last = is_node(ends.1);
    let node_first = is_node(ends.0);
    if !node_last && !node_first {
        return Err(invalid(format!(
            "link {} doesn't reach a node end",
            link.id
        )));
    }
    let mut hops: Vec<&Hop> = link.hops.iter().collect();
    // The listed order runs from the transmitting end; a link that ends at the node runs toward it.
    if node_last != toward_node {
        hops.reverse();
    }
    let hub_hop = if toward_node {
        hops.first()
    } else {
        hops.last()
    };
    let hub = hub_hop
        .and_then(|h| h.element())
        .and_then(|id| fiber.element(id))
        .ok_or_else(|| invalid(format!("link {} has no hub end", link.id)))?;
    Ok((hops, hub))
}

/// A link's hops summed.
struct Summed {
    hops: Vec<HopLoss>,
    fiber_ft: f64,
    /// (input dBm, NF dB) at each optical amplifier, in hop order.
    amps: Vec<(f64, f64)>,
}

/// Sums one link's hops from `launch_dbm`, as §16's budget does.
fn budget(
    fiber: &FiberPlant,
    library: &ComponentLibrary,
    link: &OpticalLink,
    hops: &[&Hop],
    launch_dbm: f64,
    faults: &Active<'_>,
    seed: Option<u64>,
) -> Result<Summed, PhysicsError> {
    // §8: a splice's or a fiber connector's loss, drawn within its part's tolerance, on a stream of its own by link and hop.
    let drawn = |index: usize, part: Option<&schema::common::ComponentId>| -> f64 {
        let (Some(seed), Some(range)) = (
            seed,
            part.and_then(|p| library.get(p))
                .and_then(|c| c.tolerance.as_ref())
                .and_then(|t| t.loss_db),
        ) else {
            return 0.0;
        };
        Rng::derive(seed, &[hash_str(link.id.as_str()), 7, index as u64]).range(-range, range)
    };
    let mut power_dbm = launch_dbm;
    let mut out = Vec::with_capacity(hops.len());
    let mut fiber_ft = 0.0;
    let mut amps: Vec<(f64, f64)> = Vec::new();
    // An element's event loss counts once on a link, however many of its hops are there.
    let mut seen: std::collections::BTreeSet<&schema::common::FiberElementId> = Default::default();
    for (index, hop) in hops.iter().enumerate() {
        let fault_db = match hop {
            Hop::Fiber { cable, .. } if faults.cut.contains(cable) => f64::INFINITY,
            Hop::Fiber { cable, .. } => faults.on_cable.get(cable).copied().unwrap_or(0.0),
            Hop::Splice { at, .. } => {
                let before = index.checked_sub(1).and_then(|i| hops.get(i));
                faults.splice_loss_db(at, strand_of(before), strand_of(hops.get(index + 1)))
                    + if seen.insert(at) {
                        faults.on_element.get(at).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    }
            }
            Hop::Connector { at, .. } | Hop::Through { element: at, .. } => {
                if seen.insert(at) {
                    faults.on_element.get(at).copied().unwrap_or(0.0)
                } else {
                    0.0
                }
            }
        };
        let (name, loss_db) = match hop {
            Hop::Fiber { cable, strand } => {
                let found = fiber
                    .cable(cable)
                    .ok_or_else(|| invalid(format!("link {}: no cable {cable}", link.id)))?;
                let alpha = match library.get(&found.component_ref).map(|c| &c.spec) {
                    Some(ComponentSpec::FiberCable(spec)) => {
                        fiber_loss_db_per_kft(spec, link.wavelength_nm)
                    }
                    _ => None,
                }
                .ok_or_else(|| {
                    invalid(format!(
                        "cable {cable} states no loss at {} nm",
                        link.wavelength_nm
                    ))
                })?;
                fiber_ft += found.len_ft;
                (
                    format!("fiber {cable}/{strand}"),
                    alpha * found.len_ft / 1000.0,
                )
            }
            Hop::Splice { at, loss_db } => {
                let stated = || {
                    let element = fiber.element(at)?;
                    match &library.get(&element.component_ref)?.spec {
                        ComponentSpec::SpliceClosure(spec) => spec.splice_loss_db,
                        ComponentSpec::PatchPanel(spec) => spec.splice_loss_db,
                        _ => None,
                    }
                };
                let loss = match loss_db {
                    Some(measured) => *measured,
                    None => {
                        let stated = stated().ok_or_else(|| {
                            invalid(format!(
                                "link {}: the splice at {at} has no measured loss and its part states none",
                                link.id
                            ))
                        })?;
                        let part = fiber.element(at).map(|e| &e.component_ref);
                        (stated + drawn(index, part)).max(0.0)
                    }
                };
                (format!("splice {at}"), loss)
            }
            Hop::Connector { at, component_ref } => {
                let loss = match library.get(component_ref).map(|c| &c.spec) {
                    Some(ComponentSpec::FiberConnector(spec)) => {
                        (spec.insertion_loss_db + drawn(index, Some(component_ref))).max(0.0)
                    }
                    _ => {
                        return Err(invalid(format!(
                            "link {}: {component_ref} isn't a fiber connector",
                            link.id
                        )));
                    }
                };
                (format!("connector {at}"), loss)
            }
            Hop::Through { element, .. } => {
                let found = fiber
                    .element(element)
                    .ok_or_else(|| invalid(format!("link {}: no element {element}", link.id)))?;
                match library.get(&found.component_ref).map(|c| &c.spec) {
                    Some(ComponentSpec::OpticalPassive(spec)) => {
                        (format!("through {element}"), spec.insertion_loss_db)
                    }
                    Some(ComponentSpec::OpticalAmp(spec)) => {
                        if link.wavelength_nm < spec.band_lo_nm
                            || link.wavelength_nm > spec.band_hi_nm
                        {
                            return Err(invalid(format!(
                                "link {} at {} nm passes {element}, which amplifies {}-{} nm",
                                link.id, link.wavelength_nm, spec.band_lo_nm, spec.band_hi_nm
                            )));
                        }
                        // Constant output power, whatever the input (approx), and none with no light in.
                        let in_dbm = power_dbm - fault_db;
                        if in_dbm.is_finite() {
                            amps.push((in_dbm, spec.nf_db));
                            (format!("through {element}"), power_dbm - spec.out_dbm)
                        } else {
                            (format!("through {element}"), f64::INFINITY)
                        }
                    }
                    _ => {
                        return Err(invalid(format!(
                            "link {}: light can't pass through {element}",
                            link.id
                        )));
                    }
                }
            }
        };
        // An amplifier's own line already reads its input after the fault.
        let fault_db = match hop {
            Hop::Through { element, .. }
                if matches!(
                    fiber
                        .element(element)
                        .and_then(|e| library.get(&e.component_ref))
                        .map(|c| &c.spec),
                    Some(ComponentSpec::OpticalAmp(_))
                ) =>
            {
                0.0
            }
            _ => fault_db,
        };
        let total_db = loss_db + fault_db;
        power_dbm -= total_db;
        out.push(HopLoss {
            hop: name,
            loss_db: light(total_db),
            power_dbm: light(power_dbm),
        });
    }
    Ok(Summed {
        hops: out,
        fiber_ft,
        amps,
    })
}

/// A finite power, or `None` for none.
fn light(dbm: f64) -> Option<f64> {
    dbm.is_finite().then_some(dbm)
}

/// The fiber faults on, at one time, in the terms `budget` reads.
#[derive(Default)]
struct Active<'f> {
    /// Cables cut through.
    cut: std::collections::BTreeSet<&'f schema::common::FiberCableId>,
    /// Event loss on a cable, at an element, or at a splice joining two strands, dB.
    on_cable: BTreeMap<&'f schema::common::FiberCableId, f64>,
    on_element: BTreeMap<&'f schema::common::FiberElementId, f64>,
    on_splice: Vec<(&'f schema::common::FiberElementId, [&'f StrandRef; 2], f64)>,
    /// A transmitter's launch and RIN change, dB, by its element.
    drift: BTreeMap<&'f schema::common::FiberElementId, (f64, f64)>,
}

impl<'f> Active<'f> {
    fn at(faults: &'f [FiberFault], t: Timestamp) -> Active<'f> {
        let mut on = Active::default();
        for fault in faults {
            let share = onset_share(&fault.onset, t);
            if share <= 0.0 {
                continue;
            }
            match &fault.kind {
                FiberFaultKind::FiberCut {
                    cable, repair_s, ..
                } => {
                    let start = match &fault.onset {
                        Onset::Instant => 0,
                        Onset::Ramp { start, .. } => start.0,
                    };
                    let elapsed_s = (t.0 - start) as f64;
                    if repair_s.is_none_or(|r| elapsed_s < r) {
                        on.cut.insert(cable);
                    }
                }
                FiberFaultKind::EventLoss { location, loss_db } => {
                    let loss_db = loss_db * share;
                    match location {
                        FiberLocation::Cable { cable, .. } => {
                            *on.on_cable.entry(cable).or_default() += loss_db;
                        }
                        FiberLocation::Element { element } => {
                            *on.on_element.entry(element).or_default() += loss_db;
                        }
                        FiberLocation::Splice { element, strands } => {
                            on.on_splice
                                .push((element, [&strands[0], &strands[1]], loss_db));
                        }
                    }
                }
                FiberFaultKind::TransmitterDrift {
                    element,
                    power_delta_db,
                    rin_delta_db,
                } => {
                    let entry = on.drift.entry(element).or_default();
                    entry.0 += power_delta_db * share;
                    entry.1 += rin_delta_db * share;
                }
            }
        }
        on
    }

    /// Loss at the splice at `at`, between the strands on either side of it.
    fn splice_loss_db(
        &self,
        at: &schema::common::FiberElementId,
        a: Option<StrandRef>,
        b: Option<StrandRef>,
    ) -> f64 {
        let (Some(a), Some(b)) = (a, b) else {
            return 0.0;
        };
        self.on_splice
            .iter()
            .filter(|(element, strands, _)| {
                *element == at
                    && ((strands[0] == &a && strands[1] == &b)
                        || (strands[0] == &b && strands[1] == &a))
            })
            .map(|(_, _, loss)| loss)
            .sum()
    }
}

/// The strand a fiber hop is on.
fn strand_of(hop: Option<&&Hop>) -> Option<StrandRef> {
    match hop {
        Some(Hop::Fiber { cable, strand }) => Some(StrandRef {
            cable: cable.clone(),
            strand: *strand,
        }),
        _ => None,
    }
}

/// One link read one way: forward to the node, or return from it.
/// What a link is read under: the fiber faults on at the time, and the seed for §8's draws, if any.
struct Conditions<'a, 'f> {
    faults: &'a Active<'f>,
    seed: Option<u64>,
}

fn link_result(
    fiber: &FiberPlant,
    library: &ComponentLibrary,
    link: &OpticalLink,
    toward_node: bool,
    node_part: Option<&schema::common::ComponentId>,
    channels: f64,
    conditions: &Conditions<'_, '_>,
) -> Result<LinkResult, PhysicsError> {
    let (faults, seed) = (conditions.faults, conditions.seed);
    let (hops, hub) = oriented(fiber, link, toward_node)?;
    // A transmitter's drift, by the element it's at: the hub end's, or the node end's on a return link.
    let node_end = if toward_node {
        hops.last()
    } else {
        hops.first()
    }
    .and_then(|h| h.element());
    let tx_element = if toward_node { Some(&hub.id) } else { node_end };
    let (drift_db, rin_drift_db) = tx_element
        .and_then(|e| faults.drift.get(e))
        .copied()
        .unwrap_or((0.0, 0.0));
    let node_part = node_part.ok_or_else(|| {
        invalid(format!(
            "link {} reaches the node of {}, whose root has no optic fitted for it",
            link.id, link.service_group
        ))
    })?;
    let hub_end = end_of(library, &hub.component_ref);
    let node_end = end_of(library, node_part);
    let (transmitter, receiver) = if toward_node {
        (hub.component_ref.clone(), node_part.clone())
    } else {
        (node_part.clone(), hub.component_ref.clone())
    };
    let (tx, rx) = if toward_node {
        (hub_end, node_end)
    } else {
        (node_end, hub_end)
    };
    let direction = if toward_node {
        OpticalDirection::Forward
    } else {
        OpticalDirection::Return
    };
    let finish = |launch_dbm: f64| budget(fiber, library, link, &hops, launch_dbm, faults, seed);
    match (tx, rx) {
        (Some(End::Tx(tx)), Some(End::Rx(rx))) => {
            let launch_dbm = tx.out_dbm + drift_db;
            let Summed {
                hops: hop_losses,
                fiber_ft,
                amps,
            } = finish(launch_dbm)?;
            let received_dbm = match hop_losses.last() {
                Some(h) => h.power_dbm,
                None => Some(launch_dbm),
            };
            // Driven at its reference input per channel, so its stated index (approx).
            let m = tx.omi_pct / 100.0;
            let drifted;
            let tx = if rin_drift_db == 0.0 {
                tx
            } else {
                drifted = OpticalTxSpec {
                    rin_db_per_hz: tx.rin_db_per_hz + rin_drift_db,
                    ..tx.clone()
                };
                &drifted
            };
            let cnr = received_dbm.map(|p_r| {
                let edfa_db = amps
                    .iter()
                    .map(|(in_dbm, nf_db)| edfa_term_db(m, *in_dbm, *nf_db, CNR_BANDWIDTH_HZ))
                    .collect();
                analog_cnr(m, channels, p_r, tx, rx, edfa_db)
            });
            Ok(LinkResult {
                link: link.id.clone(),
                direction,
                wavelength_nm: link.wavelength_nm,
                analog: true,
                transmitter: transmitter.clone(),
                receiver: receiver.clone(),
                launch_dbm,
                loss_db: received_dbm.map(|p| launch_dbm - p),
                received_dbm,
                fiber_ft,
                latency_us: fiber_ft / 1000.0 * FIBER_DELAY_US_PER_KFT,
                hops: hop_losses,
                up: cnr.is_some(),
                cnr,
                margin_db: None,
                overload_margin_db: None,
            })
        }
        (Some(End::Optic(tx)), Some(End::Optic(rx))) => {
            // A pluggable launches at the middle of its stated range (approx).
            let launch_dbm = (tx.tx_min_dbm + tx.tx_max_dbm) / 2.0 + drift_db;
            let Summed {
                hops: hop_losses,
                fiber_ft,
                ..
            } = finish(launch_dbm)?;
            let received_dbm = match hop_losses.last() {
                Some(h) => h.power_dbm,
                None => Some(launch_dbm),
            };
            let margin_db = received_dbm.map(|p| p - rx.rx_sensitivity_dbm);
            let overload_margin_db = received_dbm.map(|p| rx.rx_overload_dbm - p);
            Ok(LinkResult {
                link: link.id.clone(),
                direction,
                wavelength_nm: link.wavelength_nm,
                analog: false,
                transmitter: transmitter.clone(),
                receiver: receiver.clone(),
                launch_dbm,
                loss_db: received_dbm.map(|p| launch_dbm - p),
                received_dbm,
                fiber_ft,
                latency_us: fiber_ft / 1000.0 * FIBER_DELAY_US_PER_KFT,
                hops: hop_losses,
                cnr: None,
                up: margin_db.is_some_and(|m| m >= 0.0)
                    && overload_margin_db.is_some_and(|m| m >= 0.0),
                margin_db,
                overload_margin_db,
            })
        }
        _ => Err(invalid(format!(
            "link {}: its two ends aren't an analog transmitter and receiver, or two digital optics",
            link.id
        ))),
    }
}

/// §16: `plant`'s node's links in `fiber`, read against what's fitted on its root, with no fiber fault. `None` where no link
/// serves its service group, so the sim keeps §4.5's and §7.3's constants.
pub fn node_link(
    fiber: &FiberPlant,
    plant: &Plant,
    library: &ComponentLibrary,
    plan: &ChannelPlan,
) -> Result<Option<NodeLink>, PhysicsError> {
    node_link_at(fiber, plant, library, plan, &[], Timestamp(0))
}

/// [`node_link`] under the region's fiber faults at time `t`.
pub fn node_link_at(
    fiber: &FiberPlant,
    plant: &Plant,
    library: &ComponentLibrary,
    plan: &ChannelPlan,
    faults: &[FiberFault],
    t: Timestamp,
) -> Result<Option<NodeLink>, PhysicsError> {
    node_link_drawn(fiber, plant, library, plan, faults, t, None)
}

/// [`node_link_at`], with §8's draws within each splice's and fiber connector's stated tolerance where `seed` is given.
pub fn node_link_drawn(
    fiber: &FiberPlant,
    plant: &Plant,
    library: &ComponentLibrary,
    plan: &ChannelPlan,
    faults: &[FiberFault],
    t: Timestamp,
    seed: Option<u64>,
) -> Result<Option<NodeLink>, PhysicsError> {
    let active = Active::at(faults, t);
    let conditions = Conditions {
        faults: &active,
        seed,
    };
    let serving: Vec<&OpticalLink> = fiber
        .links
        .iter()
        .filter(|l| l.service_group == plant.service_group)
        .collect();
    if serving.is_empty() {
        return Ok(None);
    }
    let fitted = match plant.root().and_then(|r| r.fitted.as_ref()) {
        Some(Fitted::Active(active)) => Some(active),
        _ => None,
    };
    let primary_optic = fitted.and_then(|f| f.transceivers.first());
    let forward_part = fitted.and_then(|f| f.forward_rx.as_ref()).or(primary_optic);
    let return_part = fitted.and_then(|f| f.return_tx.as_ref()).or(primary_optic);
    let forward_link = serving.iter().find(|l| {
        matches!(
            l.direction,
            OpticalDirection::Forward | OpticalDirection::Both
        )
    });
    let return_link = serving.iter().find(|l| {
        matches!(
            l.direction,
            OpticalDirection::Return | OpticalDirection::Both
        )
    });
    let forward = forward_link
        .map(|l| {
            link_result(
                fiber,
                library,
                l,
                true,
                forward_part,
                forward_channels(plan),
                &conditions,
            )
        })
        .transpose()?;
    let ret = return_link
        .map(|l| {
            link_result(
                fiber,
                library,
                l,
                false,
                return_part,
                return_channels(plan),
                &conditions,
            )
        })
        .transpose()?;
    // §17: the CMTS behind the node, by the fiber plant's binding.
    let cmts = fiber
        .bindings
        .iter()
        .find(|b| b.service_group == plant.service_group)
        .and_then(|b| fiber.element(&b.cmts))
        .and_then(|e| library.get(&e.component_ref))
        .and_then(|c| match &c.spec {
            ComponentSpec::Cmts(spec) => Some(spec),
            _ => None,
        });
    Ok(Some(NodeLink {
        service_group: plant.service_group.clone(),
        forward,
        ret,
        cmts_source_mer: cmts.map(|c| c.source_mer.clone()).unwrap_or_default(),
        cmts_us_rx_ceiling_db: cmts.and_then(|c| c.us_rx_ceiling_db),
    }))
}

/// §16, a region at once: each node's link state under the region's fiber faults at `t`, by service group. A node no link
/// serves is left out. One fault can reach several nodes, and a node whose links it doesn't cross comes out as it would
/// without it.
pub fn region_links(
    fiber: &FiberPlant,
    nodes: &[(&Plant, &ChannelPlan)],
    library: &ComponentLibrary,
    faults: &[FiberFault],
    t: Timestamp,
) -> Result<BTreeMap<ServiceGroupId, NodeLink>, PhysicsError> {
    let mut out = BTreeMap::new();
    for (plant, plan) in nodes {
        if let Some(link) = node_link_at(fiber, plant, library, plan, faults, t)? {
            out.insert(plant.service_group.clone(), link);
        }
    }
    Ok(out)
}

/// §16, `check_physics`'s optical margin: each link's received power against its receiver's window, and a return laser's drive
/// against the composite its part says it clips at.
pub fn link_issues(
    link: &NodeLink,
    library: &ComponentLibrary,
    plan: &ChannelPlan,
) -> Vec<crate::budget::PhysicsIssue> {
    let mut out = Vec::new();
    for one in [&link.forward, &link.ret].into_iter().flatten() {
        let window = match library.get(&one.receiver).map(|c| &c.spec) {
            Some(ComponentSpec::OpticalRx(rx)) => Some((rx.in_min_dbm, rx.in_max_dbm)),
            Some(ComponentSpec::Transceiver(rx)) => {
                Some((rx.rx_sensitivity_dbm, rx.rx_overload_dbm))
            }
            _ => None,
        };
        if let Some((lo_dbm, hi_dbm)) = window {
            let outside = one.received_dbm.is_none_or(|p| p < lo_dbm || p > hi_dbm);
            if outside {
                out.push(crate::budget::PhysicsIssue::OpticalInputOutOfRange {
                    link: one.link.clone(),
                    received_dbm: one.received_dbm,
                    lo_dbm,
                    hi_dbm,
                });
            }
        }
        if one.direction == OpticalDirection::Return
            && let Some(ComponentSpec::OpticalTx(tx)) =
                library.get(&one.transmitter).map(|c| &c.spec)
            && let Some(clip_dbmv) = tx.clip_total_in_dbmv
        {
            // Padded to its reference per 6 MHz across the loaded return (§16).
            let total_dbmv = tx.ref_rf_in_dbmv + db(return_channels(plan).max(1e-9));
            if total_dbmv > clip_dbmv {
                out.push(crate::budget::PhysicsIssue::LaserOverdriven {
                    link: one.link.clone(),
                    total_in_dbmv: total_dbmv,
                    clip_dbmv,
                });
            }
        }
    }
    out
}

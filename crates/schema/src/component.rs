//! The component library: real or generic parts and what they can do. A profile holds
//! targets and a component holds capability, so output level, tilt and supply voltage
//! each live in one place.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capability::CapabilitySet;
use crate::common::{ComponentId, PortId, is_false};
use crate::profile::{Direction, Profile};

/// Version of this schema. Version 2 added classes and optional fields, and changed nothing version 1 had
/// (`docs/tasks/21-as-built-schemas.md`).
pub const SCHEMA_VERSION: &str = "2";

/// Brings a library from an earlier version to this one. Version 2 only added classes and optional fields that are
/// left out when absent, so a version 1 library means the same.
pub fn migrate(mut library: ComponentLibrary) -> Result<ComponentLibrary, String> {
    match library.schema_version.as_str() {
        "1" | "2" => {
            library.schema_version = SCHEMA_VERSION.to_string();
            Ok(library)
        }
        other => Err(format!(
            "no migration from component schema version {other}"
        )),
    }
}

/// A collection of component entries.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComponentLibrary {
    /// Version of this schema.
    pub schema_version: String,
    /// The entries.
    pub components: Vec<Component>,
}

/// A problem with a component library.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum LibraryError {
    /// Two entries share an id.
    #[error("duplicate component id {0}")]
    DuplicateId(ComponentId),
    /// A part with an RF path has no rated band, or a band that is empty.
    #[error("component {0} has no usable rated band")]
    BadRatedBand(ComponentId),
}

impl ComponentLibrary {
    /// Finds an entry by id.
    pub fn get(&self, id: &ComponentId) -> Option<&Component> {
        self.components.iter().find(|c| &c.id == id)
    }

    /// Checks ids are unique and every RF part has a rated band.
    pub fn validate(&self) -> Result<(), Vec<LibraryError>> {
        let mut errors = Vec::new();
        let mut seen = BTreeSet::new();
        for component in &self.components {
            if !seen.insert(&component.id) {
                errors.push(LibraryError::DuplicateId(component.id.clone()));
            }
            let band_ok = match component.rated_band() {
                Some((lo_mhz, hi_mhz)) => lo_mhz < hi_mhz,
                None => !component.spec.has_rf_path(),
            };
            if !band_ok {
                errors.push(LibraryError::BadRatedBand(component.id.clone()));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// How far to trust a component's numbers (invariant 9).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecConfidence {
    /// From a published data sheet.
    Datasheet,
    /// Typical values or from memory.
    Generic,
    /// Supplied by an operator.
    OperatorSupplied,
}

/// One library entry.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct Component {
    /// Stable id, referenced by plant elements and edges.
    pub id: ComponentId,
    /// Revision of this entry.
    pub version: u32,
    /// Vendor name.
    pub vendor: String,
    /// Model name.
    pub model: String,
    /// Where the numbers came from, if published.
    pub source_url: Option<String>,
    /// How far to trust the numbers.
    pub spec_confidence: SpecConfidence,
    /// Lowest rated frequency, MHz. `None` only on a part with no RF path ([`ComponentSpec::has_rf_path`]).
    pub rated_lo_mhz: Option<f64>,
    /// Highest rated frequency, MHz. `None` only on a part with no RF path ([`ComponentSpec::has_rf_path`]).
    /// A table lookup outside the rated band is an error (§2.1, §11).
    pub rated_hi_mhz: Option<f64>,
    /// How far its stated values may stray, which is what §8's randomization draws within. Absent means §8's
    /// defaults. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<Tolerance>,
    /// The class, as `class`, and its fields, as `spec`.
    #[serde(flatten)]
    pub spec: ComponentSpec,
}

/// How far a part's stated values may stray (§8, domain randomization). Each is a ± bound. Absent means §8's default.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tolerance {
    /// On its losses, ± dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loss_db: Option<f64>,
    /// On its gain, ± dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<f64>,
    /// On its loop resistance, ± percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r_loop_pct: Option<f64>,
    /// On its return loss, ± dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_loss_db: Option<f64>,
    /// On its noise figure, ± dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nf_db: Option<f64>,
    /// On a modem's implementation MER ceiling, ± dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impl_mer_db: Option<f64>,
}

impl Component {
    /// The rated band as (low, high) MHz, if the part has an RF path.
    pub fn rated_band(&self) -> Option<(f64, f64)> {
        Some((self.rated_lo_mhz?, self.rated_hi_mhz?))
    }

    /// The component class.
    pub fn class(&self) -> ComponentClass {
        self.spec.class()
    }

    /// Every version 2 field set on this entry, where its class is one version 1 had: those that change RF or power
    /// ([`ComponentSpec::version_2_rf_fields`]), and those that don't. Names are the JSON field names. A version 2
    /// class isn't listed here, since a plant only reaches one through a version 2 element or fitting.
    pub fn version_2_fields(&self) -> Vec<&'static str> {
        let mut found = self.spec.version_2_rf_fields();
        if self.tolerance.is_some() {
            found.push("tolerance");
        }
        if self
            .spec
            .active()
            .is_some_and(|a| a.plugin_family.is_some())
        {
            found.push("plugin_family");
        }
        match &self.spec {
            ComponentSpec::Tap(tap) if tap.plugin_family.is_some() => found.push("plugin_family"),
            ComponentSpec::PowerInserter(inserter) if inserter.port_i_rated_a.is_some() => {
                found.push("port_i_rated_a");
            }
            ComponentSpec::Cable(cable) | ComponentSpec::Drop(cable) => {
                if cable.family.is_some() {
                    found.push("family");
                }
                if cable.ampacity_a.is_some() {
                    found.push("ampacity_a");
                }
            }
            _ => {}
        }
        found
    }
}

/// The classes of component.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentClass {
    /// Analog fiber node.
    Node,
    /// Remote PHY device.
    Rpd,
    /// RF amplifier.
    Amp,
    /// Multi-port tap.
    Tap,
    /// Splitter.
    Splitter,
    /// Directional coupler.
    DirectionalCoupler,
    /// Power inserter.
    PowerInserter,
    /// Power supply.
    PowerSupply,
    /// Terminator.
    Terminator,
    /// Hardline cable.
    Cable,
    /// Drop cable.
    Drop,
    /// Cable modem.
    Modem,
    /// A plug-in: pad, equalizer, cable simulator, return pad or equalizer, tap conditioner. Version 2.
    PlugIn,
    /// An in-line equalizer in its own housing. Version 2.
    LineEqualizer,
    /// A connector, splice or adapter on a cable. Version 2.
    Connector,
    /// A ground block where a drop enters a home. Version 2.
    GroundBlock,
    /// An amplifier in a home. Version 2.
    DropAmp,
    /// A filter on a drop or in a home. Version 2.
    Filter,
    /// Customer equipment that reports nothing: a set-top, a television, an adapter. Version 2.
    Cpe,
    /// A standby battery. Version 2.
    Battery,
    /// A pole, pedestal, vault or cabinet. Version 2.
    Structure,
    /// Messenger strand that aerial cable is lashed to. Version 2.
    Strand,
    /// Conduit that buried cable runs in. Version 2.
    Conduit,
    /// Fiber optic cable. Version 2.
    FiberCable,
    /// A splice closure. Version 2.
    SpliceClosure,
    /// A fiber patch panel. Version 2.
    PatchPanel,
    /// A fiber connector. Version 2.
    FiberConnector,
    /// An optical splitter, multiplexer or demultiplexer. Version 2.
    OpticalPassive,
    /// An analog optical transmitter. Version 2.
    OpticalTx,
    /// An analog optical receiver. Version 2.
    OpticalRx,
    /// An optical amplifier. Version 2.
    OpticalAmp,
    /// A pluggable digital optic. Version 2.
    Transceiver,
    /// A network switch in the converged interconnect network. Version 2.
    Switch,
    /// A CMTS, integrated or a Remote PHY core. Version 2.
    Cmts,
    /// A timing source. Version 2.
    TimingSource,
}

/// Per-class fields. The tag is the component class, so the two can't disagree.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(tag = "class", content = "spec")]
pub enum ComponentSpec {
    /// See [`NodeSpec`].
    Node(NodeSpec),
    /// See [`RpdSpec`].
    Rpd(RpdSpec),
    /// See [`ActiveSpec`].
    Amp(ActiveSpec),
    /// See [`TapSpec`].
    Tap(TapSpec),
    /// See [`SplitterSpec`].
    Splitter(SplitterSpec),
    /// See [`DirectionalCouplerSpec`].
    DirectionalCoupler(DirectionalCouplerSpec),
    /// See [`PowerInserterSpec`].
    PowerInserter(PowerInserterSpec),
    /// See [`PowerSupplySpec`].
    PowerSupply(PowerSupplySpec),
    /// See [`TerminatorSpec`].
    Terminator(TerminatorSpec),
    /// See [`CableSpec`].
    Cable(CableSpec),
    /// See [`CableSpec`].
    Drop(CableSpec),
    /// See [`ModemSpec`].
    Modem(ModemSpec),
    /// See [`PlugInSpec`].
    PlugIn(PlugInSpec),
    /// See [`LineEqualizerSpec`].
    LineEqualizer(LineEqualizerSpec),
    /// See [`ConnectorSpec`].
    Connector(ConnectorSpec),
    /// See [`GroundBlockSpec`].
    GroundBlock(GroundBlockSpec),
    /// See [`DropAmpSpec`].
    DropAmp(DropAmpSpec),
    /// See [`FilterSpec`].
    Filter(FilterSpec),
    /// See [`CpeSpec`].
    Cpe(CpeSpec),
    /// See [`BatterySpec`].
    Battery(BatterySpec),
    /// See [`StructureSpec`].
    Structure(StructureSpec),
    /// See [`StrandSpec`].
    Strand(StrandSpec),
    /// See [`ConduitSpec`].
    Conduit(ConduitSpec),
    /// See [`FiberCableSpec`].
    FiberCable(FiberCableSpec),
    /// See [`SpliceClosureSpec`].
    SpliceClosure(SpliceClosureSpec),
    /// See [`PatchPanelSpec`].
    PatchPanel(PatchPanelSpec),
    /// See [`FiberConnectorSpec`].
    FiberConnector(FiberConnectorSpec),
    /// See [`OpticalPassiveSpec`].
    OpticalPassive(OpticalPassiveSpec),
    /// See [`OpticalTxSpec`].
    OpticalTx(OpticalTxSpec),
    /// See [`OpticalRxSpec`].
    OpticalRx(OpticalRxSpec),
    /// See [`OpticalAmpSpec`].
    OpticalAmp(OpticalAmpSpec),
    /// See [`TransceiverSpec`].
    Transceiver(TransceiverSpec),
    /// See [`SwitchSpec`].
    Switch(SwitchSpec),
    /// See [`CmtsSpec`].
    Cmts(CmtsSpec),
    /// See [`TimingSourceSpec`].
    TimingSource(TimingSourceSpec),
}

impl ComponentSpec {
    /// The component class this spec belongs to.
    pub fn class(&self) -> ComponentClass {
        match self {
            ComponentSpec::Node(_) => ComponentClass::Node,
            ComponentSpec::Rpd(_) => ComponentClass::Rpd,
            ComponentSpec::Amp(_) => ComponentClass::Amp,
            ComponentSpec::Tap(_) => ComponentClass::Tap,
            ComponentSpec::Splitter(_) => ComponentClass::Splitter,
            ComponentSpec::DirectionalCoupler(_) => ComponentClass::DirectionalCoupler,
            ComponentSpec::PowerInserter(_) => ComponentClass::PowerInserter,
            ComponentSpec::PowerSupply(_) => ComponentClass::PowerSupply,
            ComponentSpec::Terminator(_) => ComponentClass::Terminator,
            ComponentSpec::Cable(_) => ComponentClass::Cable,
            ComponentSpec::Drop(_) => ComponentClass::Drop,
            ComponentSpec::Modem(_) => ComponentClass::Modem,
            ComponentSpec::PlugIn(_) => ComponentClass::PlugIn,
            ComponentSpec::LineEqualizer(_) => ComponentClass::LineEqualizer,
            ComponentSpec::Connector(_) => ComponentClass::Connector,
            ComponentSpec::GroundBlock(_) => ComponentClass::GroundBlock,
            ComponentSpec::DropAmp(_) => ComponentClass::DropAmp,
            ComponentSpec::Filter(_) => ComponentClass::Filter,
            ComponentSpec::Cpe(_) => ComponentClass::Cpe,
            ComponentSpec::Battery(_) => ComponentClass::Battery,
            ComponentSpec::Structure(_) => ComponentClass::Structure,
            ComponentSpec::Strand(_) => ComponentClass::Strand,
            ComponentSpec::Conduit(_) => ComponentClass::Conduit,
            ComponentSpec::FiberCable(_) => ComponentClass::FiberCable,
            ComponentSpec::SpliceClosure(_) => ComponentClass::SpliceClosure,
            ComponentSpec::PatchPanel(_) => ComponentClass::PatchPanel,
            ComponentSpec::FiberConnector(_) => ComponentClass::FiberConnector,
            ComponentSpec::OpticalPassive(_) => ComponentClass::OpticalPassive,
            ComponentSpec::OpticalTx(_) => ComponentClass::OpticalTx,
            ComponentSpec::OpticalRx(_) => ComponentClass::OpticalRx,
            ComponentSpec::OpticalAmp(_) => ComponentClass::OpticalAmp,
            ComponentSpec::Transceiver(_) => ComponentClass::Transceiver,
            ComponentSpec::Switch(_) => ComponentClass::Switch,
            ComponentSpec::Cmts(_) => ComponentClass::Cmts,
            ComponentSpec::TimingSource(_) => ComponentClass::TimingSource,
        }
    }

    /// Whether RF passes through or out of this part, so it has a rated band. A supply, a battery, a structure, the
    /// fiber plant's passive and digital parts, and a Remote PHY core have none. An analog optical transmitter or
    /// receiver, and an integrated CMTS, have an RF side.
    pub fn has_rf_path(&self) -> bool {
        match self {
            ComponentSpec::PowerSupply(_)
            | ComponentSpec::Battery(_)
            | ComponentSpec::Structure(_)
            | ComponentSpec::Strand(_)
            | ComponentSpec::Conduit(_)
            | ComponentSpec::FiberCable(_)
            | ComponentSpec::SpliceClosure(_)
            | ComponentSpec::PatchPanel(_)
            | ComponentSpec::FiberConnector(_)
            | ComponentSpec::OpticalPassive(_)
            | ComponentSpec::OpticalAmp(_)
            | ComponentSpec::Transceiver(_)
            | ComponentSpec::Switch(_)
            | ComponentSpec::TimingSource(_) => false,
            ComponentSpec::Cmts(cmts) => cmts.kind == CmtsKind::Integrated,
            _ => true,
        }
    }

    /// Whether this is a class version 1 had. A plant that uses any other is a version 2 plant.
    pub fn is_version_1_class(&self) -> bool {
        matches!(
            self,
            ComponentSpec::Node(_)
                | ComponentSpec::Rpd(_)
                | ComponentSpec::Amp(_)
                | ComponentSpec::Tap(_)
                | ComponentSpec::Splitter(_)
                | ComponentSpec::DirectionalCoupler(_)
                | ComponentSpec::PowerInserter(_)
                | ComponentSpec::PowerSupply(_)
                | ComponentSpec::Terminator(_)
                | ComponentSpec::Cable(_)
                | ComponentSpec::Drop(_)
                | ComponentSpec::Modem(_)
        )
    }

    /// The version 2 fields set on a version 1 class that change what the plant does to a signal or to power: the
    /// ones the sim has to price, not ignore. Fields that only check compatibility (a plug-in or size family) or
    /// state a rating nothing exceeds yet (ampacity) aren't listed, nor is `gain_flatness_db`, a stated bound with no shape:
    /// the sim reads shape from `gain_table` (§15, task 22). Names are the JSON field names.
    pub fn version_2_rf_fields(&self) -> Vec<&'static str> {
        let mut found = Vec::new();
        let mut note = |set: bool, name: &'static str| {
            if set {
                found.push(name);
            }
        };
        if let Some(active) = self.active() {
            note(!active.outputs.is_empty(), "outputs");
            note(!active.nf_table.is_empty(), "nf_table");
            note(!active.cin_table.is_empty(), "cin_table");
            note(active.ref_composite_dbmv.is_some(), "ref_composite_dbmv");
            note(!active.gain_table.is_empty(), "gain_table");
            note(!active.agc_pilots_mhz.is_empty(), "agc_pilots_mhz");
            note(!active.return_loss_table.is_empty(), "return_loss_table");
        }
        match self {
            ComponentSpec::Node(node) => note(!node.source_mer.is_empty(), "source_mer"),
            ComponentSpec::Rpd(rpd) => {
                note(!rpd.source_mer.is_empty(), "source_mer");
                note(rpd.us_rx_ceiling_db.is_some(), "us_rx_ceiling_db");
            }
            ComponentSpec::Tap(tap) => {
                note(tap.terminating, "terminating");
                note(!tap.return_loss_by_port.is_empty(), "return_loss_by_port");
            }
            ComponentSpec::Splitter(splitter) => {
                note(splitter.leg_isolation_db.is_some(), "leg_isolation_db");
                note(!splitter.return_loss_table.is_empty(), "return_loss_table");
            }
            ComponentSpec::DirectionalCoupler(coupler) => {
                note(!coupler.return_loss_table.is_empty(), "return_loss_table");
            }
            ComponentSpec::Terminator(terminator) => {
                note(
                    !terminator.return_loss_table.is_empty(),
                    "return_loss_table",
                );
            }
            ComponentSpec::Modem(modem) => {
                note(!modem.return_loss_table.is_empty(), "return_loss_table");
            }
            ComponentSpec::PowerInserter(inserter) => {
                note(!inserter.insertion_loss.is_empty(), "insertion_loss");
                note(inserter.return_loss_db.is_some(), "return_loss_db");
            }
            ComponentSpec::Cable(cable) | ComponentSpec::Drop(cable) => {
                note(cable.r_loop_k_temp.is_some(), "r_loop_k_temp");
            }
            _ => {}
        }
        found
    }

    /// The RF module of an active, if this is one.
    pub fn active(&self) -> Option<&ActiveSpec> {
        match self {
            ComponentSpec::Node(node) => Some(&node.active),
            ComponentSpec::Rpd(rpd) => Some(&rpd.active),
            ComponentSpec::Amp(amp) => Some(amp),
            _ => None,
        }
    }
}

/// Cable or drop attenuation, delay and resistance.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CableSpec {
    /// Skin-effect coefficient `a` in `A(f) = a·√f + b·f`, dB per 100 ft at f in MHz (§1.1).
    pub atten_a: f64,
    /// Dielectric coefficient `b` in `A(f) = a·√f + b·f`, dB per 100 ft at f in MHz (§1.1).
    pub atten_b: f64,
    /// Attenuation temperature coefficient `k_T`, per °C (§1.2).
    pub k_temp: f64,
    /// Velocity factor, dimensionless (§1.3).
    pub v_f: f64,
    /// DC loop resistance, Ω per 1000 ft (§1.4).
    pub r_loop_ohm_per_kft: f64,
    /// Size family, such as `500` or `series-6`, so a connector can be checked against its cable. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Current it can carry, amps, at 20 °C ambient [SCTE 32]. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ampacity_a: Option<f64>,
    /// How loop resistance rises with conductor temperature, per °C from 20 °C (§6.2, §19). Absent holds
    /// `r_loop_ohm_per_kft` at every temperature, as version 1 did. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r_loop_k_temp: Option<f64>,
}

/// A point in a loss table, interpolated linearly in dB (§2.1).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LossPoint {
    /// Frequency, MHz.
    pub f_mhz: f64,
    /// Loss, dB.
    pub loss_db: f64,
}

/// A point in a return loss table (§5.1). Interpolated linearly in dB, and held flat beyond the table out to the
/// rated band.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReturnLossPoint {
    /// Frequency, MHz.
    pub f_mhz: f64,
    /// Return loss, dB.
    pub return_loss_db: f64,
}

/// A point in a noise figure table (§4.2).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NoiseFigurePoint {
    /// Frequency, MHz.
    pub f_mhz: f64,
    /// Noise figure, dB.
    pub nf_db: f64,
}

/// A point in a gain table (§15).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GainPoint {
    /// Frequency, MHz.
    pub f_mhz: f64,
    /// Gain, dB.
    pub gain_db: f64,
}

/// A point in a CIN table, at the part's `ref_out_dbmv` (§4.4).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CinPoint {
    /// Frequency, MHz.
    pub f_mhz: f64,
    /// Carrier to intermodulation noise, dB.
    pub cin_db: f64,
}

/// A point in a source MER table: what a transmitter's own modulator makes, before any plant (§4.5).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MerPoint {
    /// Frequency, MHz.
    pub f_mhz: f64,
    /// MER, dB.
    pub mer_db: f64,
}

/// A band of frequencies.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FreqRange {
    /// Lower edge, MHz.
    pub lo_mhz: f64,
    /// Upper edge, MHz.
    pub hi_mhz: f64,
}

/// A point in a group delay table (§5.6).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroupDelayPoint {
    /// Frequency, MHz.
    pub f_mhz: f64,
    /// Group delay, ns.
    pub delay_ns: f64,
}

/// One diplexer setting. Group delay is here because diplex filters cause it, so it moves
/// with the split (§5.6).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Split {
    /// Top of the upstream band, MHz.
    pub us_hi_mhz: f64,
    /// Bottom of the downstream band, MHz.
    pub ds_lo_mhz: f64,
    /// Band-edge group delay at this setting, ns (§5.6).
    pub group_delay_ns: Vec<GroupDelayPoint>,
}

/// How an active separates the two directions.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum Duplex {
    /// Frequency division, at one of a set of splits.
    Diplexed {
        /// The splits this part supports.
        splits: Vec<Split>,
        /// Whether the split can change without a truck roll.
        remote_switchable: bool,
    },
    /// Echo-cancelled full duplex (SCTE 296).
    FullDuplexEc {
        /// Band-edge group delay, ns (§5.6).
        group_delay_ns: Vec<GroupDelayPoint>,
    },
}

/// An amp or a node's or RPD's RF module.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ActiveSpec {
    /// Highest output it can deliver at the band's top frequency, dBmV per 6 MHz (§3.4).
    pub max_out_dbmv_at_hi: f64,
    /// Highest forward gain, dB (§3.4).
    pub max_gain_db: f64,
    /// Lowest forward input at the band's top frequency, dBmV per 6 MHz (§3.4).
    pub min_in_dbmv: f64,
    /// Forward noise figure, dB (§4.2).
    pub nf_db: f64,
    /// Return noise figure, dB (§7.3).
    pub return_nf_db: f64,
    /// CTB at `ref_out_dbmv`, dB (§4.4).
    pub ctb_ref_db: f64,
    /// CSO at `ref_out_dbmv`, dB (§4.4).
    pub cso_ref_db: f64,
    /// CIN at `ref_out_dbmv`, dB (§4.4).
    pub cin_ref_db: f64,
    /// Output level the distortion figures are quoted at, dBmV per 6 MHz at the band's
    /// top frequency (§4.4).
    pub ref_out_dbmv: f64,
    /// Highest return gain, dB (§7.1).
    pub return_gain_db: f64,
    /// AGC range either side of nominal, dB (§3.4).
    pub agc_range_db: f64,
    /// Return loss at its ports, dB (§5.1).
    pub return_loss_db: f64,
    /// How it separates the two directions.
    pub duplex: Duplex,
    /// Power draw, watts (§6.1).
    pub power_w: f64,
    /// Lowest input voltage it runs at, volts (§6.3).
    pub v_min: f64,
    /// Its RF outputs, when it has more than one or they differ: per output port, its level below the main output and
    /// the return combining loss into it. Absent means every output is the main output. Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutputSpec>,
    /// Which plug-ins fit it: a plug-in fits a host of the same family. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_family: Option<String>,
    /// Forward noise figure by frequency, in place of `nf_db` where given (§4.2). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nf_table: Vec<NoiseFigurePoint>,
    /// CIN by frequency at `ref_out_dbmv`, in place of `cin_ref_db` where given (§4.4). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cin_table: Vec<CinPoint>,
    /// Total composite power at the output that the distortion and CIN figures are quoted at, dBmV (§4.4, §17). Where
    /// given, CIN follows the composite power the channel plan loads rather than the level at the top frequency.
    /// Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_composite_dbmv: Option<f64>,
    /// Forward gain flatness, peak to valley, dB. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_flatness_db: Option<f64>,
    /// Forward gain by frequency at full gain with no plug-ins, its built-in slope included, in place of `max_gain_db`
    /// flat where given. Alignment reads it (§15). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gain_table: Vec<GainPoint>,
    /// Frequencies its AGC reads level at, MHz (§3.4). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agc_pilots_mhz: Vec<f64>,
    /// Return loss at its ports by frequency, in place of `return_loss_db` where given (§5.1). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_loss_table: Vec<ReturnLossPoint>,
}

/// One RF output of an active. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// The output's port id, on an element built from this part.
    pub port: PortId,
    /// Its forward level below the main output, dB: 0 for the main output, more for a split or coupled one.
    pub loss_from_main_db: f64,
    /// Return loss from this output port to the return amplifier's input, through the combiner, dB.
    pub return_combining_loss_db: f64,
}

/// An analog fiber node.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NodeSpec {
    /// The RF module.
    pub active: ActiveSpec,
    /// Forward analog optical link CNR, dB (§4.5).
    pub optics_cnr_db: f64,
    /// Return analog optical link SNR, dB (§7.3).
    pub us_optics_snr_db: f64,
    /// MER the node's own RF stages allow by frequency, before any plant (§4.5). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_mer: Vec<MerPoint>,
}

/// A PNM test an RPD can run, from its `SupportedPnmTests` bits. UTSC is reported apart
/// from these.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpdPnmTest {
    /// Upstream active and quiet probes.
    Upc,
    /// Upstream RxMER per subcarrier.
    UsRxmer,
    /// Upstream impulse noise.
    ImpulseNoise,
    /// Downstream symbol capture.
    DsSymbolCapture,
    /// Histogram.
    Histogram,
}

/// A Remote PHY device. Digital optics, so no optical noise terms (§4.5, §7.3).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RpdSpec {
    /// The RF module.
    pub active: ActiveSpec,
    /// PNM tests it can run.
    pub supported_pnm_tests: Vec<RpdPnmTest>,
    /// MER its downstream modulators make by frequency, before any plant (§4.5). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_mer: Vec<MerPoint>,
    /// The highest upstream RxMER its burst receivers can report, dB: the receiver's own ceiling. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub us_rx_ceiling_db: Option<f64>,
}

/// A multi-port tap (§2.1).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TapSpec {
    /// Number of tap ports.
    pub ports: u8,
    /// Nominal tap value, dB.
    pub tap_value_db: f64,
    /// Input to through port, dB; held flat beyond the table out to the rated band.
    pub insertion_loss: Vec<LossPoint>,
    /// Input to each tap port, dB; held flat beyond the table out to the rated band.
    pub tap_loss: Vec<LossPoint>,
    /// Tap port to tap port isolation, dB (§5.1).
    pub port_isolation_db: f64,
    /// Return loss at its ports, dB (§5.1).
    pub return_loss_db: f64,
    /// Whether AC passes from input to through.
    pub power_passing: bool,
    /// An end-of-line tap: no through port, its last leg is its own termination. Version 2.
    #[serde(default, skip_serializing_if = "is_false")]
    pub terminating: bool,
    /// Which plug-ins fit its faceplate. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_family: Option<String>,
    /// Return loss by frequency, one table per class of port, in place of `return_loss_db` where given. Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_loss_by_port: Vec<PortReturnLoss>,
}

/// A class of port on a tap, for a table that differs between them.
#[derive(
    Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum TapPortClass {
    /// The input.
    Input,
    /// The through port.
    Through,
    /// A tap port.
    TapPort,
}

/// Return loss by frequency at one class of port. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PortReturnLoss {
    /// Which ports.
    pub port: TapPortClass,
    /// The table.
    pub points: Vec<ReturnLossPoint>,
}

/// A splitter (§2.2).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SplitterSpec {
    /// Number of output legs.
    pub legs: u8,
    /// Input to each leg, dB.
    pub leg_loss: Vec<LossPoint>,
    /// Return loss at its ports, dB (§5.1).
    pub return_loss_db: f64,
    /// Whether AC passes to the legs.
    pub power_passing: bool,
    /// Leg to leg isolation, dB. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leg_isolation_db: Option<f64>,
    /// Return loss by frequency, in place of `return_loss_db` where given. Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_loss_table: Vec<ReturnLossPoint>,
}

/// A directional coupler (§2.2). Through loss is computed from these.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectionalCouplerSpec {
    /// Coupled port value `X`, dB.
    pub coupled_value_db: f64,
    /// Excess through loss, dB.
    pub excess_loss_db: f64,
    /// Return loss at its ports, dB (§5.1).
    pub return_loss_db: f64,
    /// Whether AC passes to the through port.
    pub power_passing: bool,
    /// Return loss by frequency, in place of `return_loss_db` where given. Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_loss_table: Vec<ReturnLossPoint>,
}

/// A power inserter (§6.2).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PowerInserterSpec {
    /// Rated current, amps.
    pub i_rated_a: f64,
    /// Whether AC passes through it.
    pub power_passing: bool,
    /// RF input to output, dB; held flat beyond the table out to the rated band. Empty is version 1's lossless inserter
    /// (§14). Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub insertion_loss: Vec<LossPoint>,
    /// Return loss at its RF ports, dB (§14). Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_loss_db: Option<f64>,
    /// Most current through one RF port, amps, where it's less than `i_rated_a` (§6.3). Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_i_rated_a: Option<f64>,
}

/// A power supply (§6.3).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PowerSupplySpec {
    /// Output voltage, volts.
    pub v_out: f64,
    /// Rated current, amps.
    pub i_rated_a: f64,
}

/// A terminator.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TerminatorSpec {
    /// Return loss, dB (§5.1).
    pub return_loss_db: f64,
    /// Return loss by frequency, in place of `return_loss_db` where given. Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_loss_table: Vec<ReturnLossPoint>,
}

/// A modem's DOCSIS generation.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModemGeneration {
    /// DOCSIS 3.0: SC-QAM only.
    D30,
    /// DOCSIS 3.1.
    D31,
}

/// A cable modem.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModemSpec {
    /// DOCSIS generation.
    pub generation: ModemGeneration,
    /// Implementation MER ceiling, dB (§4.5).
    pub impl_mer_db: f64,
    /// Noise figure, dB (§4.5).
    pub nf_db: f64,
    /// Return loss at its port, dB (§5.1).
    pub return_loss_db: f64,
    /// `Pmax`, total transmit power, dBmV. The only stored transmit limit, because
    /// per-channel limits are derived (§7.2).
    pub tx_max_total_dbmv: f64,
    /// What this modem can report.
    pub capability: CapabilitySet,
    /// Return loss by frequency, in place of `return_loss_db` where given. Version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_loss_table: Vec<ReturnLossPoint>,
}

/// What a plug-in does. Forward kinds sit after the forward diplexer leg, return kinds after the return leg, and a tap
/// conditioner in a tap's faceplate carries both.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlugInKind {
    /// Flat forward attenuation.
    Pad,
    /// Forward equalizer: more loss low in the band than high, to undo cable tilt.
    Equalizer,
    /// Forward cable simulator: more loss high in the band than low, as cable would add.
    CableSimulator,
    /// Flat return attenuation.
    ReturnPad,
    /// Return equalizer.
    ReturnEqualizer,
    /// In a tap's faceplate, shaping or attenuating every port.
    TapConditioner,
}

/// A plug-in: a module that fits a host of its family. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlugInSpec {
    /// What it does.
    pub kind: PlugInKind,
    /// The host family it fits: an active's or tap's `plugin_family`.
    pub family: String,
    /// Its value, dB: the attenuation of a pad, or for a shaped kind the difference in loss across `slope_band`.
    pub value_db: f64,
    /// For a shaped kind, the band its value is stated across. `None` for a flat one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slope_band: Option<FreqRange>,
    /// Loss at its least-loss frequency on top of its value, dB.
    pub insertion_loss_db: f64,
    /// Measured loss by frequency, in place of the shape where given.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loss_table: Vec<LossPoint>,
    /// Return loss, dB (§5.1), if stated. A plug-in sits inside its host, which reflects as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_loss_db: Option<f64>,
}

/// An in-line equalizer in its own housing, cut into a span. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LineEqualizerSpec {
    /// The difference in loss across `slope_band`, dB, more loss low in the band.
    pub value_db: f64,
    /// The band its value is stated across.
    pub slope_band: FreqRange,
    /// Loss at its least-loss frequency on top of its value, dB.
    pub insertion_loss_db: f64,
    /// Measured loss by frequency, in place of the shape where given.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loss_table: Vec<LossPoint>,
    /// Return loss at its ports, dB (§5.1).
    pub return_loss_db: f64,
    /// Whether AC passes through it.
    pub power_passing: bool,
    /// How it separates the two directions, when its forward equalizer sits behind a diplexer. `None` for one that
    /// equalizes the whole band.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplex: Option<Duplex>,
}

/// The kinds of coax connector.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind {
    /// A hardline pin connector into a housing port.
    HardlinePin,
    /// A splice joining two lengths of hardline.
    HardlineSplice,
    /// An adapter at a housing port: an elbow or an extension.
    HousingAdapter,
    /// A drop's F connector.
    DropF,
}

/// A connector, splice or adapter, at a point along a cable. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConnectorSpec {
    /// What it is.
    pub kind: ConnectorKind,
    /// The cable size family it fits, matched against `CableSpec::family`. `None` fits any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cable_family: Option<String>,
    /// Insertion loss, dB.
    pub insertion_loss_db: f64,
    /// Return loss, dB (§5.1).
    pub return_loss_db: f64,
    /// Return loss by frequency, in place of `return_loss_db` where given.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_loss_table: Vec<ReturnLossPoint>,
}

/// A ground block where a drop enters a home. How well the home's wiring keeps ambient signals out is the home's,
/// not the part's, so it's on the plant element. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroundBlockSpec {
    /// Input to output, dB; held flat beyond the table out to the rated band.
    pub insertion_loss: Vec<LossPoint>,
    /// Return loss at its ports, dB (§5.1).
    pub return_loss_db: f64,
}

/// An amplifier in a home. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DropAmpSpec {
    /// Forward outputs.
    pub outputs: u8,
    /// Forward gain to each output, dB.
    pub gain_db: f64,
    /// Return gain, dB. `None` for a passive return path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_gain_db: Option<f64>,
    /// Loss through a passive return path, dB. `None` when the return is amplified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_passive_loss_db: Option<f64>,
    /// Forward noise figure, dB (§4.2).
    pub nf_db: f64,
    /// Return noise figure, dB, on an amplified return (§7.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_nf_db: Option<f64>,
    /// Highest output at the band's top frequency, dBmV per 6 MHz, if stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_out_dbmv_at_hi: Option<f64>,
    /// Return loss at its ports, dB (§5.1).
    pub return_loss_db: f64,
    /// How it separates the two directions.
    pub duplex: Duplex,
}

/// The shape of a filter.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterShape {
    /// Passes above its stop band.
    HighPass,
    /// Passes below its stop band.
    LowPass,
    /// Stops one band and passes the rest, such as a point-of-entry filter.
    BandStop,
    /// Passes one band and stops the rest.
    BandPass,
}

/// A filter, on a tap port or in a home. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterSpec {
    /// Its shape.
    pub shape: FilterShape,
    /// The bands it stops.
    pub stop_bands: Vec<FreqRange>,
    /// Width of each edge's roll-off from pass to stop, MHz, if stated. `None` is an abrupt edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_mhz: Option<f64>,
    /// Attenuation in a stop band, dB.
    pub stop_depth_db: f64,
    /// Loss in the pass band, dB.
    pub pass_loss_db: f64,
    /// Return loss in the pass band, dB (§5.1).
    pub return_loss_db: f64,
    /// Return loss in a stop band, dB: a filter reflects what it stops.
    pub stop_return_loss_db: f64,
}

/// What a piece of customer equipment is.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpeDevice {
    /// A set-top box.
    SetTop,
    /// A television tuned directly from the coax.
    Television,
    /// An in-home network adapter.
    MocaAdapter,
    /// Anything else on a port.
    Other,
}

/// Customer equipment that reports nothing to the twin. It loads a port and reflects. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CpeSpec {
    /// What it is.
    pub device: CpeDevice,
    /// Return loss at its port, dB (§5.1).
    pub return_loss_db: f64,
    /// Return loss by frequency, in place of `return_loss_db` where given.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub return_loss_table: Vec<ReturnLossPoint>,
}

/// A standby battery. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BatterySpec {
    /// Nominal voltage, volts.
    pub v_nominal: f64,
    /// Capacity, amp-hours.
    pub capacity_ah: f64,
    /// Voltage it mustn't be discharged below, volts.
    pub v_end_discharge: f64,
}

/// The kinds of structure plant is mounted on or in.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureKind {
    /// A utility pole.
    Pole,
    /// A pedestal at grade.
    Pedestal,
    /// A buried vault or handhole.
    Vault,
    /// A cabinet, such as a power supply's.
    Cabinet,
}

/// A structure. It carries no RF and no power. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StructureSpec {
    /// What it is.
    pub kind: StructureKind,
}

/// Messenger strand, counted by the foot. Nothing about it reaches the sim. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrandSpec {}

/// Conduit, counted by the foot. Nothing about it reaches the sim. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConduitSpec {}

/// A fiber type.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiberType {
    /// Standard single-mode, ITU-T G.652.D.
    G652D,
    /// Bend-insensitive single-mode, ITU-T G.657.A1.
    G657A1,
    /// Bend-insensitive single-mode, ITU-T G.657.A2.
    G657A2,
}

/// How a fiber cable is built.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiberConstruction {
    /// Loose buffer tubes, lashed or in duct.
    LooseTube,
    /// Ribbon.
    Ribbon,
    /// All-dielectric self-supporting, for aerial spans without strand.
    Adss,
    /// Armored, for direct burial.
    Armored,
    /// For blowing into microduct.
    Microduct,
    /// A drop or a jumper.
    Drop,
}

/// Fiber loss at one wavelength.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WavelengthLoss {
    /// Wavelength, nm.
    pub wavelength_nm: f64,
    /// Loss, dB per 1000 ft.
    pub loss_db_per_kft: f64,
}

/// A fiber optic cable. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FiberCableSpec {
    /// Fibers in the sheath.
    pub fiber_count: u32,
    /// Fibers per buffer tube or ribbon.
    pub fibers_per_tube: u32,
    /// The fiber.
    pub fiber_type: FiberType,
    /// How it's built.
    pub construction: FiberConstruction,
    /// Loss by wavelength, interpolated linearly between the wavelengths given.
    pub loss: Vec<WavelengthLoss>,
}

/// A splice closure. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpliceClosureSpec {
    /// Splices it holds.
    pub splice_capacity: u32,
    /// Splice trays.
    pub trays: u32,
    /// Loss of a splice made in it that has no measured value, dB (§16). Its spread is the part's `tolerance.loss_db`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub splice_loss_db: Option<f64>,
}

/// A fiber patch panel. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PatchPanelSpec {
    /// Adapter ports.
    pub ports: u32,
    /// Loss of a splice made in it that has no measured value, dB (§16). Its spread is the part's `tolerance.loss_db`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub splice_loss_db: Option<f64>,
}

/// A fiber connector's end-face polish.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiberPolish {
    /// Ultra physical contact.
    Upc,
    /// Angled physical contact.
    Apc,
}

/// A fiber connector, mated pair. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FiberConnectorSpec {
    /// Its style, such as `SC` or `LC`.
    pub style: String,
    /// End-face polish.
    pub polish: FiberPolish,
    /// Insertion loss of a mated pair, dB.
    pub insertion_loss_db: f64,
    /// Return loss, dB.
    pub return_loss_db: f64,
}

/// The kinds of optical passive.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpticalPassiveKind {
    /// A power splitter, one to many.
    Splitter,
    /// A wavelength multiplexer, many to one.
    Mux,
    /// A wavelength demultiplexer, one to many.
    Demux,
}

/// An optical splitter, multiplexer or demultiplexer. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpticalPassiveSpec {
    /// What it is.
    pub kind: OpticalPassiveKind,
    /// Outputs of a splitter, or channels of a mux or demux.
    pub ports: u32,
    /// Common port to any other, dB.
    pub insertion_loss_db: f64,
    /// Channel wavelengths of a mux or demux, nm, in port order. Empty for a splitter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels_nm: Vec<f64>,
    /// Isolation between adjacent channels, dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation_db: Option<f64>,
    /// Return loss, dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_loss_db: Option<f64>,
}

/// Which way light carries the signal, away from the hub or towards it.
#[derive(
    Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum OpticalDirection {
    /// Hub to node: the downstream.
    Forward,
    /// Node to hub: the upstream.
    Return,
    /// Both, on one strand or one optic.
    Both,
}

/// The kind of laser in an analog transmitter.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaserKind {
    /// Distributed feedback, directly modulated.
    Dfb,
    /// Fabry–Perot.
    FabryPerot,
    /// Externally modulated.
    External,
}

/// An analog optical transmitter: RF in, light out. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpticalTxSpec {
    /// Forward at the hub, or return in a node.
    pub direction: OpticalDirection,
    /// The laser.
    pub laser: LaserKind,
    /// Wavelength, nm.
    pub wavelength_nm: f64,
    /// Optical output, dBm.
    pub out_dbm: f64,
    /// Relative intensity noise, dB/Hz.
    pub rin_db_per_hz: f64,
    /// Optical modulation index per channel at `ref_rf_in_dbmv`, percent.
    pub omi_pct: f64,
    /// RF input per channel the modulation index is stated at, dBmV.
    pub ref_rf_in_dbmv: f64,
    /// Total RF input above which it clips, dBmV, if stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_total_in_dbmv: Option<f64>,
}

/// An analog optical receiver: light in, RF out. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpticalRxSpec {
    /// Forward in a node, or return at the hub.
    pub direction: OpticalDirection,
    /// Lowest optical input, dBm.
    pub in_min_dbm: f64,
    /// Highest optical input, dBm.
    pub in_max_dbm: f64,
    /// Responsivity, A/W.
    pub responsivity_a_per_w: f64,
    /// Equivalent input noise current density, pA/√Hz.
    pub ein_pa_per_rthz: f64,
    /// RF output per channel at `ref_in_dbm` and the transmitter's stated index, dBmV.
    pub rf_out_dbmv: f64,
    /// Optical input `rf_out_dbmv` is stated at, dBm.
    pub ref_in_dbm: f64,
    /// Optical input range over which its AGC holds RF output, dB. `None` for none: output moves 2 dB per optical dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agc_range_db: Option<f64>,
}

/// An optical amplifier. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpticalAmpSpec {
    /// Total optical output, dBm.
    pub out_dbm: f64,
    /// Noise figure, dB.
    pub nf_db: f64,
    /// Lowest optical input, dBm.
    pub in_min_dbm: f64,
    /// Highest optical input, dBm.
    pub in_max_dbm: f64,
    /// The band it amplifies, nm.
    pub band_lo_nm: f64,
    /// The band it amplifies, nm.
    pub band_hi_nm: f64,
}

/// A pluggable digital optic. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransceiverSpec {
    /// Its form factor, such as `SFP+`.
    pub form_factor: String,
    /// Line rate, Gbit/s.
    pub rate_gbps: f64,
    /// Transmit wavelength, nm.
    pub wavelength_nm: f64,
    /// Receive wavelength of a bidirectional optic, nm. `None` receives where it transmits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rx_wavelength_nm: Option<f64>,
    /// Lowest launch power, dBm.
    pub tx_min_dbm: f64,
    /// Highest launch power, dBm.
    pub tx_max_dbm: f64,
    /// Receiver sensitivity, dBm.
    pub rx_sensitivity_dbm: f64,
    /// Receiver overload, dBm.
    pub rx_overload_dbm: f64,
}

/// A switch in the converged interconnect network. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SwitchSpec {
    /// Optic ports.
    pub ports: u32,
    /// Rate per port, Gbit/s.
    pub port_rate_gbps: f64,
}

/// The kinds of CMTS.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmtsKind {
    /// MAC and PHY in one hub chassis, with RF ports to analog optics.
    Integrated,
    /// A Remote PHY core, physical or virtual: the PHY is in the node.
    Core,
}

/// A CMTS or Remote PHY core. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CmtsSpec {
    /// Which kind.
    pub kind: CmtsKind,
    /// Service group ports.
    pub ports: u32,
    /// MER its downstream modulators make by frequency, before any plant. Integrated only (§4.5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_mer: Vec<MerPoint>,
    /// The highest upstream RxMER its burst receivers can report, dB. Integrated only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub us_rx_ceiling_db: Option<f64>,
}

/// A timing source. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TimingSourceSpec {
    /// Ports it serves.
    pub ports: u32,
}

/// Whether a part can be used on a profile. Computed, never stored (invariant 1).
///
/// A plant part must cover the whole band plan and, if active, support its duplex. A part that carries one direction
/// only (a forward or return plug-in, an analog optical transmitter or receiver) must cover that direction's
/// segments. A modem only has to reach some of each direction: a D3.0 modem shares a high-split node using the
/// channels it can reach (§11). Other customer equipment only has to reach some of the downstream. A part with no RF
/// path qualifies, as a supply does.
pub fn qualifies(component: &Component, profile: &Profile) -> bool {
    let Some((rated_lo_mhz, rated_hi_mhz)) = component.rated_band() else {
        return !component.spec.has_rf_path();
    };
    let reaches = |direction: Direction| {
        profile
            .band_plan
            .iter()
            .filter(|segment| segment.carries(direction))
            .any(|segment| segment.lo_mhz < rated_hi_mhz && rated_lo_mhz < segment.hi_mhz)
    };
    let covers = |direction: Option<Direction>| {
        profile
            .band_plan
            .iter()
            .filter(|segment| direction.is_none_or(|d| segment.carries(d)))
            .all(|segment| rated_lo_mhz <= segment.lo_mhz && segment.hi_mhz <= rated_hi_mhz)
    };
    match &component.spec {
        ComponentSpec::Modem(_) => reaches(Direction::Us) && reaches(Direction::Ds),
        ComponentSpec::Cpe(_) => reaches(Direction::Ds),
        ComponentSpec::PlugIn(plugin) => match plugin.kind {
            PlugInKind::Pad | PlugInKind::Equalizer | PlugInKind::CableSimulator => {
                covers(Some(Direction::Ds))
            }
            PlugInKind::ReturnPad | PlugInKind::ReturnEqualizer => covers(Some(Direction::Us)),
            PlugInKind::TapConditioner => covers(None),
        },
        ComponentSpec::OpticalTx(OpticalTxSpec { direction, .. })
        | ComponentSpec::OpticalRx(OpticalRxSpec { direction, .. }) => match direction {
            OpticalDirection::Forward => covers(Some(Direction::Ds)),
            OpticalDirection::Return => covers(Some(Direction::Us)),
            OpticalDirection::Both => covers(None),
        },
        ComponentSpec::DropAmp(amp) => covers(None) && supports_duplex(&amp.duplex, profile),
        ComponentSpec::LineEqualizer(eq) => {
            covers(None)
                && eq
                    .duplex
                    .as_ref()
                    .is_none_or(|duplex| supports_duplex(duplex, profile))
        }
        spec => {
            covers(None)
                && spec
                    .active()
                    .is_none_or(|active| supports_duplex(&active.duplex, profile))
        }
    }
}

fn supports_duplex(duplex: &Duplex, profile: &Profile) -> bool {
    let shared = profile
        .band_plan
        .iter()
        .any(|segment| segment.direction == Direction::Both);
    match duplex {
        Duplex::FullDuplexEc { .. } => shared,
        Duplex::Diplexed { splits, .. } => {
            let Some((us_hi_mhz, ds_lo_mhz)) = profile.split_edges() else {
                return false;
            };
            !shared
                && splits.iter().any(|split| {
                    (split.us_hi_mhz - us_hi_mhz).abs() < 1e-9
                        && (split.ds_lo_mhz - ds_lo_mhz).abs() < 1e-9
                })
        }
    }
}

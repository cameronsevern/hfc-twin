//! Design rules: an operator's design specification as a data file, as a profile is for an architecture and the
//! policy table is for repairs. Every value carries its basis, a section of a reference note that sources it or
//! `Generic` (`docs/as-built-plan.md`, rule 6). See `docs/tasks/21-as-built-schemas.md` and
//! `docs/reference/design-practice.md`.
//!
//! Version 2 (task 23) adds what the version 2 designer reads that version 1 didn't hold. Every new field is optional and
//! left out of the JSON when absent, so a version 1 file is a version 2 file once its version string is bumped; the
//! designer asks for what it needs with [`DesignRules::for_design`]. See `docs/tasks/23-as-built-designer.md`.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::common::{ComponentId, ProfileId};
use crate::component::{ComponentClass, ComponentLibrary, ComponentSpec, CpeDevice};

/// Version of this schema. Version 2 added the optional fields marked so, and changed nothing version 1 had.
pub const SCHEMA_VERSION: &str = "2";

/// Brings a rules file from an earlier version to this one. Version 2 only added optional fields, so a version 1 file
/// reads as it was; [`DesignRules::for_design`] says what it lacks for the version 2 designer.
pub fn migrate(mut rules: DesignRules) -> Result<DesignRules, String> {
    match rules.schema_version.as_str() {
        "1" | "2" => {
            rules.schema_version = SCHEMA_VERSION.to_string();
            Ok(rules)
        }
        other => Err(format!(
            "no migration from design rules schema version {other}"
        )),
    }
}

/// Where a rule's value comes from (invariant 9, for rules).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
pub enum Basis {
    /// A reference note that cites a public document: `file.md §section`.
    Sourced {
        /// The note and section, such as `design-practice.md §2`.
        note: String,
    },
    /// Typical, or from memory. To confirm.
    Generic,
}

/// A value and its basis.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Ruled<T> {
    /// The value.
    pub value: T,
    /// Where it comes from.
    pub basis: Basis,
}

/// One design specification.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignRules {
    /// Version of this schema.
    pub schema_version: String,
    /// Stable id.
    pub id: String,
    /// Level at the tap port, per profile.
    pub tap_port: Vec<TapPortTarget>,
    /// What the design leaves for the drop and the home.
    pub premises_allowance: PremisesAllowance,
    /// How a line ends.
    pub end_of_line: EndOfLine,
    /// The taps a design may place, by port count.
    pub tap_families: Vec<TapFamily>,
    /// Cable and amp by the role of the line.
    pub roles: Vec<RoleRule>,
    /// Drop cable by length, shortest reach first.
    pub drops: Vec<DropRule>,
    /// Node size and port use.
    pub node: NodeRules,
    /// Powering.
    pub powering: PoweringRules,
    /// Structures along routes.
    pub structures: StructureRules,
    /// What's in a home, and how often.
    pub premises: Vec<PremisesTemplate>,
    /// Fiber.
    pub fiber: FiberRules,
    /// What things cost.
    pub costs: CostTable,
    /// What the rules say per profile: the root, station levels, premises allowed and optics. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<Vec<ProfileRules>>,
    /// Tap conditioners and line equalizers. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditioning: Option<ConditioningRules>,
    /// How a line branches away from an active. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branching: Option<BranchRules>,
    /// The parts in a home, and what an unused tap port gets. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub premises_parts: Option<PremisesParts>,
    /// Connectors and splices on cable. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fittings: Option<FittingRules>,
    /// The alternatives a design's candidate set tries. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates: Option<CandidateRules>,
    /// The moves an upgrade in place may make. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<UpgradeRules>,
}

/// The level a tap port is designed to, stated per 6 MHz at the top and bottom of the forward band. The two
/// together state the tilt at the port.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TapPortTarget {
    /// The profile it applies to.
    pub profile_ref: ProfileId,
    /// At the top forward frequency, dBmV per 6 MHz.
    pub at_hi_dbmv: Ruled<f64>,
    /// At the bottom forward frequency, dBmV per 6 MHz. `None` (version 2): the port is held at one level, at the top,
    /// as taps are nominally set [SCTE 183 §16.4] and as the one worked run states its devices (design-practice.md §1).
    /// The bottom of the band is then bounded by the modem's window and its spread limit, which the sim checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_lo_dbmv: Option<Ruled<f64>>,
    /// How far a port may sit from the target either way, dB.
    pub tolerance_db: Ruled<f64>,
    /// How far a port's tilt, `at_hi − at_lo`, may sit from the target's before a conditioner or line equalizer goes in,
    /// dB. Version 2. `None`: no limit, as North America sets none at the modem (plant-and-powering.md §5). It needs
    /// `at_lo_dbmv`, which gives the target's tilt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt_tolerance_db: Option<Ruled<f64>>,
}

/// The loss the design leaves between the tap port and the device, at the top forward frequency.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PremisesAllowance {
    /// For the drop, dB.
    pub drop_loss_db: Ruled<f64>,
    /// For the ground block, in-home splitting and cable, dB.
    pub house_loss_db: Ruled<f64>,
}

/// How a line ends.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EndOfLine {
    /// Whether the last tap on a line is a terminating tap.
    pub last_tap_terminating: Ruled<bool>,
    /// The lowest a last tap's port may be at the top forward frequency, dBmV per 6 MHz.
    pub min_port_dbmv_at_hi: Ruled<f64>,
}

/// One family of taps.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TapFamily {
    /// Tap ports.
    pub ports: u8,
    /// Through taps' values, dB.
    pub values_db: Ruled<Vec<f64>>,
    /// Terminating taps' values, dB.
    pub terminating_values_db: Ruled<Vec<f64>>,
    /// The library's parts that make up the family, through and terminating, each a `Tap` of `ports` ports at one of the
    /// values above. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Ruled<Vec<ComponentId>>>,
}

/// The role of a line.
#[derive(
    Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum LineRole {
    /// Carries signal to amps, with no taps.
    Express,
    /// Feeds taps.
    Feeder,
}

/// Cable and amp for one role.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RoleRule {
    /// The role.
    pub role: LineRole,
    /// The hardline, a `Cable`.
    pub cable_ref: Ruled<ComponentId>,
    /// The amp, an `Amp`.
    pub amp_ref: Ruled<ComponentId>,
}

/// Drop cable up to a length.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DropRule {
    /// Longest drop this cable is used for, feet.
    pub max_len_ft: Ruled<f64>,
    /// The cable, a `Drop`.
    pub cable_ref: Ruled<ComponentId>,
}

/// Node size and port use.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NodeRules {
    /// Homes passed per node, at most.
    pub max_homes: Ruled<u32>,
    /// RF ports used.
    pub rf_ports: Ruled<u8>,
    /// Amps in cascade below the node, at most.
    pub max_cascade: Ruled<u8>,
    /// Actives below the node, at most, the node not counted. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_actives: Option<Ruled<u32>>,
}

/// Powering.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PoweringRules {
    /// The supply, a `PowerSupply`.
    pub supply_ref: Ruled<ComponentId>,
    /// Share of a supply's rating a design may load it to, percent.
    pub design_load_pct: Ruled<f64>,
    /// Most current through one power inserter or active port, amps.
    pub port_max_a: Ruled<f64>,
    /// The battery, a `Battery`.
    pub battery_ref: Ruled<ComponentId>,
    /// Batteries in series per string.
    pub batteries_per_string: Ruled<u8>,
    /// Strings per supply.
    pub battery_strings: Ruled<u8>,
    /// The power inserter a supply feeds, a `PowerInserter`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inserter_ref: Option<Ruled<ComponentId>>,
    /// Volts each active keeps above its part's minimum input, at both temperatures. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_margin_v: Option<Ruled<f64>>,
}

/// Structures along routes.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StructureRules {
    /// Span between poles, feet.
    pub pole_spacing_ft: Ruled<f64>,
    /// Longest buried run between pedestals or vaults, feet.
    pub pedestal_spacing_ft: Ruled<f64>,
    /// Whether aerial hardline takes an expansion loop at each pole.
    pub expansion_loop_each_pole: Ruled<bool>,
    /// The strand, a `Strand`.
    pub strand_ref: Ruled<ComponentId>,
    /// The conduit, a `Conduit`.
    pub conduit_ref: Ruled<ComponentId>,
    /// Which side of a street, facing the way it's drawn, plant runs on. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Ruled<StreetSide>>,
    /// How far from the street's centerline structures stand, feet. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setback_ft: Option<Ruled<f64>>,
    /// The pole, a `Structure` of kind `Pole`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pole_ref: Option<Ruled<ComponentId>>,
    /// The pedestal, a `Structure` of kind `Pedestal`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pedestal_ref: Option<Ruled<ComponentId>>,
}

/// A side of a street, facing the way its points are drawn. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreetSide {
    /// To the left.
    Left,
    /// To the right.
    Right,
}

/// One kind of home: what's between the drop and the devices.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PremisesTemplate {
    /// Stable id.
    pub id: String,
    /// Share of homes with a modem that look like this, 0 to 1.
    pub share: Ruled<f64>,
    /// Where the template's parts come from.
    pub basis: Basis,
    /// The ground block, a `GroundBlock`, or `None` for a modem straight on the drop.
    pub ground_block_ref: Option<ComponentId>,
    /// A drop amp, a `DropAmp`.
    pub drop_amp_ref: Option<ComponentId>,
    /// A filter, a `Filter`.
    pub filter_ref: Option<ComponentId>,
    /// The in-home splitter, a `Splitter`.
    pub splitter_ref: Option<ComponentId>,
    /// In-home cable to each device, feet.
    pub house_cable_ft: f64,
    /// Modems.
    pub modems: u8,
    /// Other devices.
    pub cpe: Vec<CpeDevice>,
}

/// Fiber.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FiberRules {
    /// Fibers per analog node.
    pub fibers_per_analog_node: Ruled<u32>,
    /// Fibers per RPD.
    pub fibers_per_rpd: Ruled<u32>,
    /// Fiber counts cable comes in.
    pub cable_counts: Ruled<Vec<u32>>,
    /// Longest continuous cable, feet: a splice closure at least this often.
    pub reel_len_ft: Ruled<f64>,
    /// Margin a link budget keeps, dB.
    pub link_margin_db: Ruled<f64>,
    /// Allowance per fusion splice, dB.
    pub splice_loss_db: Ruled<f64>,
    /// Allowance per mated connector pair, dB.
    pub connector_loss_db: Ruled<f64>,
    /// Outside fiber cable by count, each a `FiberCable` whose count is one of `cable_counts`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cable_refs: Option<Ruled<Vec<ComponentId>>>,
    /// The splice closure, a `SpliceClosure`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure_ref: Option<Ruled<ComponentId>>,
    /// The hub's patch panel, a `PatchPanel`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_ref: Option<Ruled<ComponentId>>,
    /// The connector on every mated pair, a `FiberConnector`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_ref: Option<Ruled<ComponentId>>,
    /// A one-fiber jumper in the hub, a `FiberCable`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jumper_ref: Option<Ruled<ComponentId>>,
    /// A two-fiber jumper in the hub, for a duplex optic, a `FiberCable`. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplex_jumper_ref: Option<Ruled<ComponentId>>,
    /// A jumper's length, feet. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jumper_len_ft: Option<Ruled<f64>>,
    /// The most an analog forward link's per-channel modulation index may be, times the square root of the channels
    /// loaded: the composite a transmitter is chosen to stay under, clear of clipping. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite_index_max: Option<Ruled<f64>>,
}

/// What things cost, in US dollars.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CostTable {
    /// Per part, installed.
    pub parts: Vec<PartCost>,
    /// Per foot of construction.
    pub construction: Vec<ConstructionCost>,
    /// Plug-ins, by family. Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plug_in_families: Option<Vec<FamilyCost>>,
    /// The most a design spends per home passed before it says coax isn't the answer for those homes, US dollars.
    /// Version 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceiling_usd_per_home: Option<Ruled<f64>>,
}

/// What a plug-in of one family costs, any value. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FamilyCost {
    /// The family, as a `PlugIn` part states it.
    pub family: String,
    /// Each, US dollars.
    pub usd: Ruled<f64>,
}

/// One part's cost.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PartCost {
    /// The part.
    pub component_ref: ComponentId,
    /// Each, US dollars.
    pub usd: Ruled<f64>,
}

/// The kinds of construction priced by the foot.
#[derive(
    Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum ConstructionKind {
    /// Lashing hardline to strand.
    AerialCoax,
    /// Burying hardline in conduit.
    UndergroundCoax,
    /// Lashing fiber to strand.
    AerialFiber,
    /// Burying fiber in conduit.
    UndergroundFiber,
    /// An aerial drop.
    AerialDrop,
    /// A buried drop.
    BuriedDrop,
}

/// One kind of construction's cost.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConstructionCost {
    /// What's built.
    pub kind: ConstructionKind,
    /// Per foot, US dollars.
    pub usd_per_ft: Ruled<f64>,
}

/// What the rules say for one profile. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProfileRules {
    /// The profile.
    pub profile_ref: ProfileId,
    /// The node or RPD, a `Node` or `Rpd` part that qualifies for the profile.
    pub root_ref: Ruled<ComponentId>,
    /// The levels a station is set to.
    pub station: StationLevels,
    /// The premises templates a design may draw on this profile, by id.
    pub premises: Ruled<Vec<String>>,
    /// The hub and node optics.
    pub optics: Ruled<NodeOptics>,
}

/// The output a station is set to, at the top forward frequency, and its tilt across the forward band. A design sets each
/// station within these, so the taps nearest it fit (`SetPoint`, `docs/physics.md` §15). Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StationLevels {
    /// The highest output a station is set to, dBmV per 6 MHz.
    pub out_dbmv_at_hi: Ruled<f64>,
    /// The lowest, dBmV per 6 MHz.
    pub min_out_dbmv_at_hi: Ruled<f64>,
    /// The most tilt, dB.
    pub tilt_db: Ruled<f64>,
    /// The least tilt, dB.
    pub min_tilt_db: Ruled<f64>,
}

/// A node's optics and what serves it in the hub. Each list is in order of preference: the designer takes the first
/// combination that `sim::optics` passes with the rules' margin. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum NodeOptics {
    /// An analog node behind an integrated CMTS.
    Analog {
        /// The CMTS, a `Cmts`.
        cmts_ref: ComponentId,
        /// Forward transmitters in the hub, `OpticalTx`.
        forward_tx: Vec<ComponentId>,
        /// Hub couplers between a forward transmitter and its fiber, `OpticalPassive`, tried after none.
        hub_couplers: Vec<ComponentId>,
        /// Optical amplifiers in the hub after a 1550 nm forward transmitter, `OpticalAmp`, tried after none: links
        /// beyond about 25 km need one (fiber-optics.md §7).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        hub_amps: Vec<ComponentId>,
        /// The node's forward receiver, `OpticalRx`.
        node_rx: Vec<ComponentId>,
        /// The node's return laser, `OpticalTx`.
        return_tx: Vec<ComponentId>,
        /// Return receivers in the hub, `OpticalRx`.
        hub_rx: Vec<ComponentId>,
    },
    /// A Remote PHY device on the converged interconnect network.
    Digital {
        /// The Remote PHY core, a `Cmts`.
        core_ref: ComponentId,
        /// The switch, a `Switch`.
        switch_ref: ComponentId,
        /// The optic at both ends, `Transceiver`.
        transceivers: Vec<ComponentId>,
        /// The timing source, a `TimingSource`.
        timing_ref: ComponentId,
    },
}

/// Tap conditioners and line equalizers. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConditioningRules {
    /// Tap conditioners, `PlugIn` parts of kind `TapConditioner`, any value.
    pub conditioners: Ruled<Vec<ComponentId>>,
    /// Line equalizers, `LineEqualizer`.
    pub line_equalizers: Ruled<Vec<ComponentId>>,
}

/// How a line branches away from an active. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BranchRules {
    /// The splitter for an equal branch, a two-leg `Splitter`.
    pub splitter_ref: Ruled<ComponentId>,
    /// Couplers for an unequal one, `DirectionalCoupler`.
    pub couplers: Ruled<Vec<ComponentId>>,
}

/// What an unused tap port gets. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnusedPort {
    /// A terminator.
    Terminated,
    /// Nothing.
    Open,
}

/// The part for one kind of customer device. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CpeRef {
    /// The device.
    pub device: CpeDevice,
    /// The part, a `Cpe`.
    pub component_ref: ComponentId,
}

/// The parts in a home, and what an unused tap port gets. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PremisesParts {
    /// A DOCSIS 3.0 modem, a `Modem`.
    pub modem_d30_ref: Ruled<ComponentId>,
    /// A DOCSIS 3.1 modem, a `Modem`.
    pub modem_d31_ref: Ruled<ComponentId>,
    /// The part for each kind of device a template names.
    pub cpe_refs: Ruled<Vec<CpeRef>>,
    /// In-home cable, a `Drop`.
    pub house_cable_ref: Ruled<ComponentId>,
    /// What an unused tap port gets.
    pub unused_port: Ruled<UnusedPort>,
    /// The terminator, a `Terminator`.
    pub terminator_ref: Ruled<ComponentId>,
    /// A template with a drop amp is drawn only for a home whose modem would otherwise sit below this at the top forward
    /// frequency, dBmV per 6 MHz.
    pub drop_amp_below_dbmv: Ruled<f64>,
}

/// Connectors and splices on cable. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FittingRules {
    /// Connectors and splices, `Connector` parts. A cable takes the one whose family is its own, and a drop the `DropF`.
    pub connectors: Ruled<Vec<ComponentId>>,
    /// Longest hardline between splices, feet.
    pub hardline_reel_len_ft: Ruled<f64>,
    /// The hardline jumper between two housings at one structure, feet: the one span a design lays shorter than a run.
    pub jumper_len_ft: Ruled<f64>,
}

/// The alternatives a design's candidate set tries. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CandidateRules {
    /// Node counts tried, as steps from the count the homes per node target gives.
    pub node_count_steps: Ruled<Vec<i32>>,
    /// Express cables tried, `Cable`. The express role's own is always tried.
    pub express_cables: Ruled<Vec<ComponentId>>,
}

/// A move an upgrade in place may make when a part no longer fits the target profile. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeMove {
    /// Swap an amp's or node's module for one rated to the target, in the same housing at the same site.
    SwapActiveModules,
    /// Swap a tap's faceplate for one of the same port count rated to the target, in the same housing.
    SwapTapFaceplates,
    /// Swap passives rated short of the target.
    SwapPassives,
    /// Change conditioners and plug-ins for the new split.
    ChangePlugIns,
    /// Take out premises parts that block the target: filters, drop amps, splitters.
    RemovePremisesParts,
    /// Swap a modem for one that qualifies.
    SwapModems,
    /// Take RF set-tops and televisions on the coax off it, replaced by devices on the home's own network, where the
    /// target's return band reaches the tuner band; an in-home splitter left with one leg comes out with them (SCTE 300's
    /// upgrade expense lines, design-practice.md §10).
    ReplaceSetTops,
}

/// The moves an upgrade in place may make. Version 2.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpgradeRules {
    /// The moves, each with its basis.
    pub moves: Vec<Ruled<UpgradeMove>>,
}

/// Every version 2 value the designer reads, present. [`DesignRules::for_design`] makes one.
#[derive(Debug, Clone, Copy)]
pub struct Complete<'a> {
    /// The rules.
    pub rules: &'a DesignRules,
    /// Per profile.
    pub profiles: &'a [ProfileRules],
    /// Conditioning.
    pub conditioning: &'a ConditioningRules,
    /// Branching.
    pub branching: &'a BranchRules,
    /// Premises parts.
    pub premises_parts: &'a PremisesParts,
    /// Fittings.
    pub fittings: &'a FittingRules,
    /// Candidates.
    pub candidates: &'a CandidateRules,
}

impl<'a> Complete<'a> {
    /// The rules for one profile.
    pub fn profile(&self, profile: &ProfileId) -> Option<&'a ProfileRules> {
        self.profiles.iter().find(|p| &p.profile_ref == profile)
    }

    /// The tap-port target for one profile.
    pub fn tap_port(&self, profile: &ProfileId) -> Option<&'a TapPortTarget> {
        self.rules
            .tap_port
            .iter()
            .find(|t| &t.profile_ref == profile)
    }
}

/// A problem with a rules file.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum RulesError {
    /// A part the rules name isn't in the library.
    #[error("{rule} names unknown component {component}")]
    UnknownComponent {
        /// The rule.
        rule: String,
        /// The part.
        component: ComponentId,
    },
    /// A part the rules name is the wrong class.
    #[error("{rule} needs a {expected:?}, {component} is a {found:?}")]
    WrongClass {
        /// The rule.
        rule: String,
        /// The part.
        component: ComponentId,
        /// What it needs.
        expected: ComponentClass,
        /// What it is.
        found: ComponentClass,
    },
    /// Premises shares that aren't each 0 to 1 or don't sum to 1.
    #[error("premises shares sum to {0}")]
    Shares(f64),
    /// Drop rules not in order of length.
    #[error("drop rules aren't in order of length")]
    DropOrder,
    /// Two of something that must be unique.
    #[error("duplicate {0}")]
    Duplicate(String),
    /// A sourced value whose note isn't `file.md §section`. Version 2.
    #[error("{rule}: basis {note:?} isn't `file.md §section`")]
    BadNote {
        /// The rule.
        rule: String,
        /// What it says.
        note: String,
    },
    /// A part the rules name doesn't fit where it's named: a tap of the wrong size or value, a structure of the wrong
    /// kind, a premises template that doesn't exist. Version 2.
    #[error("{rule}: {why}")]
    Misfit {
        /// The rule.
        rule: String,
        /// Why.
        why: String,
    },
    /// A part or plug-in family the version 2 designer can place has no cost. Version 2.
    #[error("no cost for {0}")]
    MissingCost(String),
    /// A value the version 2 designer reads isn't in the file. Version 2.
    #[error("the version 2 designer needs {0}")]
    Incomplete(String),
}

impl DesignRules {
    /// Checks the rules against a library: the parts they name exist and are the right class, shares sum to one, and
    /// drop rules run from short to long.
    pub fn validate(&self, library: &ComponentLibrary) -> Result<(), Vec<RulesError>> {
        let mut errors = Vec::new();
        let mut part =
            |rule: String, component: &ComponentId, expected: ComponentClass| match library
                .get(component)
            {
                None => errors.push(RulesError::UnknownComponent {
                    rule,
                    component: component.clone(),
                }),
                Some(found) if found.class() != expected => errors.push(RulesError::WrongClass {
                    rule,
                    component: component.clone(),
                    expected,
                    found: found.class(),
                }),
                Some(_) => {}
            };
        for role in &self.roles {
            part(
                format!("role {:?} cable", role.role),
                &role.cable_ref.value,
                ComponentClass::Cable,
            );
            part(
                format!("role {:?} amp", role.role),
                &role.amp_ref.value,
                ComponentClass::Amp,
            );
        }
        for (index, drop) in self.drops.iter().enumerate() {
            part(
                format!("drop rule {index}"),
                &drop.cable_ref.value,
                ComponentClass::Drop,
            );
        }
        part(
            "powering supply".into(),
            &self.powering.supply_ref.value,
            ComponentClass::PowerSupply,
        );
        part(
            "powering battery".into(),
            &self.powering.battery_ref.value,
            ComponentClass::Battery,
        );
        part(
            "structures strand".into(),
            &self.structures.strand_ref.value,
            ComponentClass::Strand,
        );
        part(
            "structures conduit".into(),
            &self.structures.conduit_ref.value,
            ComponentClass::Conduit,
        );
        for template in &self.premises {
            let refs = [
                (&template.ground_block_ref, ComponentClass::GroundBlock),
                (&template.drop_amp_ref, ComponentClass::DropAmp),
                (&template.filter_ref, ComponentClass::Filter),
                (&template.splitter_ref, ComponentClass::Splitter),
            ];
            for (component, class) in refs {
                if let Some(component) = component {
                    part(format!("premises {}", template.id), component, class);
                }
            }
        }
        for cost in &self.costs.parts {
            if library.get(&cost.component_ref).is_none() {
                errors.push(RulesError::UnknownComponent {
                    rule: "costs".into(),
                    component: cost.component_ref.clone(),
                });
            }
        }

        let total: f64 = self.premises.iter().map(|t| t.share.value).sum();
        let each_ok = self
            .premises
            .iter()
            .all(|t| (0.0..=1.0).contains(&t.share.value));
        if !each_ok || (total - 1.0).abs() > 1e-9 {
            errors.push(RulesError::Shares(total));
        }
        if self
            .drops
            .windows(2)
            .any(|pair| pair[0].max_len_ft.value >= pair[1].max_len_ft.value)
        {
            errors.push(RulesError::DropOrder);
        }
        let mut unique = |label: &str, keys: Vec<String>| {
            let mut seen = BTreeSet::new();
            for key in keys {
                if !seen.insert(key.clone()) {
                    errors.push(RulesError::Duplicate(format!("{label} {key}")));
                }
            }
        };
        unique(
            "tap port target",
            self.tap_port
                .iter()
                .map(|t| t.profile_ref.to_string())
                .collect(),
        );
        unique(
            "tap family",
            self.tap_families
                .iter()
                .map(|f| f.ports.to_string())
                .collect(),
        );
        unique(
            "role",
            self.roles.iter().map(|r| format!("{:?}", r.role)).collect(),
        );
        unique(
            "premises template",
            self.premises.iter().map(|t| t.id.clone()).collect(),
        );
        unique(
            "construction cost",
            self.costs
                .construction
                .iter()
                .map(|c| format!("{:?}", c.kind))
                .collect(),
        );
        self.validate_version_2(library, &mut errors);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Every part a version 2 section names, with the rule and the classes it may be.
    fn version_2_parts(&self) -> Vec<(String, &ComponentId, &'static [ComponentClass])> {
        use ComponentClass as C;
        let mut out: Vec<(String, &ComponentId, &'static [ComponentClass])> = Vec::new();
        for family in &self.tap_families {
            if let Some(parts) = &family.parts {
                for part in &parts.value {
                    out.push((format!("tap family {}", family.ports), part, &[C::Tap]));
                }
            }
        }
        for p in self.profiles.iter().flatten() {
            let rule = |what: &str| format!("profile {} {what}", p.profile_ref);
            out.push((rule("root"), &p.root_ref.value, &[C::Node, C::Rpd]));
            match &p.optics.value {
                NodeOptics::Analog {
                    cmts_ref,
                    forward_tx,
                    hub_couplers,
                    hub_amps,
                    node_rx,
                    return_tx,
                    hub_rx,
                } => {
                    out.push((rule("cmts"), cmts_ref, &[C::Cmts]));
                    for (what, list, class) in [
                        ("forward_tx", forward_tx, &[C::OpticalTx][..]),
                        ("hub_couplers", hub_couplers, &[C::OpticalPassive]),
                        ("hub_amps", hub_amps, &[C::OpticalAmp]),
                        ("node_rx", node_rx, &[C::OpticalRx]),
                        ("return_tx", return_tx, &[C::OpticalTx]),
                        ("hub_rx", hub_rx, &[C::OpticalRx]),
                    ] {
                        for part in list {
                            out.push((rule(what), part, class));
                        }
                    }
                }
                NodeOptics::Digital {
                    core_ref,
                    switch_ref,
                    transceivers,
                    timing_ref,
                } => {
                    out.push((rule("core"), core_ref, &[C::Cmts]));
                    out.push((rule("switch"), switch_ref, &[C::Switch]));
                    out.push((rule("timing"), timing_ref, &[C::TimingSource]));
                    for part in transceivers {
                        out.push((rule("transceivers"), part, &[C::Transceiver]));
                    }
                }
            }
        }
        if let Some(c) = &self.conditioning {
            for part in &c.conditioners.value {
                out.push(("conditioning conditioners".into(), part, &[C::PlugIn]));
            }
            for part in &c.line_equalizers.value {
                out.push((
                    "conditioning line_equalizers".into(),
                    part,
                    &[C::LineEqualizer],
                ));
            }
        }
        if let Some(b) = &self.branching {
            out.push((
                "branching splitter".into(),
                &b.splitter_ref.value,
                &[C::Splitter],
            ));
            for part in &b.couplers.value {
                out.push(("branching couplers".into(), part, &[C::DirectionalCoupler]));
            }
        }
        if let Some(p) = &self.premises_parts {
            out.push((
                "premises_parts d30".into(),
                &p.modem_d30_ref.value,
                &[C::Modem],
            ));
            out.push((
                "premises_parts d31".into(),
                &p.modem_d31_ref.value,
                &[C::Modem],
            ));
            for cpe in &p.cpe_refs.value {
                out.push((
                    format!("premises_parts cpe {:?}", cpe.device),
                    &cpe.component_ref,
                    &[C::Cpe],
                ));
            }
            out.push((
                "premises_parts house cable".into(),
                &p.house_cable_ref.value,
                &[C::Drop],
            ));
            out.push((
                "premises_parts terminator".into(),
                &p.terminator_ref.value,
                &[C::Terminator],
            ));
        }
        if let Some(f) = &self.fittings {
            for part in &f.connectors.value {
                out.push(("fittings connectors".into(), part, &[C::Connector]));
            }
        }
        if let Some(c) = &self.candidates {
            for part in &c.express_cables.value {
                out.push(("candidates express_cables".into(), part, &[C::Cable]));
            }
        }
        if let Some(r) = &self.powering.inserter_ref {
            out.push(("powering inserter".into(), &r.value, &[C::PowerInserter]));
        }
        if let Some(r) = &self.structures.pole_ref {
            out.push(("structures pole".into(), &r.value, &[C::Structure]));
        }
        if let Some(r) = &self.structures.pedestal_ref {
            out.push(("structures pedestal".into(), &r.value, &[C::Structure]));
        }
        let f = &self.fiber;
        for part in f.cable_refs.iter().flat_map(|r| &r.value) {
            out.push(("fiber cables".into(), part, &[C::FiberCable]));
        }
        for (what, r, class) in [
            ("fiber closure", &f.closure_ref, &[C::SpliceClosure][..]),
            ("fiber panel", &f.panel_ref, &[C::PatchPanel]),
            ("fiber connector", &f.connector_ref, &[C::FiberConnector]),
            ("fiber jumper", &f.jumper_ref, &[C::FiberCable]),
            (
                "fiber duplex jumper",
                &f.duplex_jumper_ref,
                &[C::FiberCable],
            ),
        ] {
            if let Some(r) = r {
                out.push((what.into(), &r.value, class));
            }
        }
        out
    }

    /// Version 2's checks, on whatever version 2 sections the file has.
    fn validate_version_2(&self, library: &ComponentLibrary, errors: &mut Vec<RulesError>) {
        for t in &self.tap_port {
            if t.tilt_tolerance_db.is_some() && t.at_lo_dbmv.is_none() {
                errors.push(RulesError::Misfit {
                    rule: format!("tap_port {} tilt_tolerance_db", t.profile_ref),
                    why: "a tilt limit needs a target at the bottom of the band".into(),
                });
            }
        }
        for (rule, component, classes) in self.version_2_parts() {
            match library.get(component) {
                None => errors.push(RulesError::UnknownComponent {
                    rule,
                    component: component.clone(),
                }),
                Some(found) if !classes.contains(&found.class()) => {
                    errors.push(RulesError::WrongClass {
                        rule,
                        component: component.clone(),
                        expected: classes[0],
                        found: found.class(),
                    })
                }
                Some(_) => {}
            }
        }
        let misfit = |rule: String, why: String| RulesError::Misfit { rule, why };
        for family in &self.tap_families {
            for part in family.parts.iter().flat_map(|p| &p.value) {
                if let Some(ComponentSpec::Tap(tap)) = library.get(part).map(|c| &c.spec) {
                    let values = if tap.terminating {
                        &family.terminating_values_db.value
                    } else {
                        &family.values_db.value
                    };
                    let value_ok = values.iter().any(|v| (v - tap.tap_value_db).abs() < 1e-9);
                    if tap.ports != family.ports || !value_ok {
                        errors.push(misfit(
                            format!("tap family {}", family.ports),
                            format!("{part} is a {}-port {} dB tap", tap.ports, tap.tap_value_db),
                        ));
                    }
                }
            }
        }
        if let Some(c) = &self.conditioning {
            for part in &c.conditioners.value {
                if let Some(ComponentSpec::PlugIn(p)) = library.get(part).map(|c| &c.spec)
                    && p.kind != crate::component::PlugInKind::TapConditioner
                {
                    errors.push(misfit(
                        "conditioning conditioners".into(),
                        format!("{part} isn't a tap conditioner"),
                    ));
                }
            }
        }
        if let Some(b) = &self.branching
            && let Some(ComponentSpec::Splitter(sp)) =
                library.get(&b.splitter_ref.value).map(|c| &c.spec)
            && sp.legs != 2
        {
            errors.push(misfit(
                "branching splitter".into(),
                format!("{} has {} legs", b.splitter_ref.value, sp.legs),
            ));
        }
        use crate::component::StructureKind;
        for (rule, r, kind) in [
            (
                "structures pole",
                &self.structures.pole_ref,
                StructureKind::Pole,
            ),
            (
                "structures pedestal",
                &self.structures.pedestal_ref,
                StructureKind::Pedestal,
            ),
        ] {
            if let Some(r) = r
                && let Some(ComponentSpec::Structure(st)) = library.get(&r.value).map(|c| &c.spec)
                && st.kind != kind
            {
                errors.push(misfit(
                    rule.into(),
                    format!("{} is a {:?}", r.value, st.kind),
                ));
            }
        }
        for part in self.fiber.cable_refs.iter().flat_map(|r| &r.value) {
            if let Some(ComponentSpec::FiberCable(cable)) = library.get(part).map(|c| &c.spec)
                && !self.fiber.cable_counts.value.contains(&cable.fiber_count)
            {
                errors.push(misfit(
                    "fiber cables".into(),
                    format!(
                        "{part} has {} fibers, not one of the counts",
                        cable.fiber_count
                    ),
                ));
            }
        }
        let templates: BTreeSet<&str> = self.premises.iter().map(|t| t.id.as_str()).collect();
        let mut seen_profiles = BTreeSet::new();
        for p in self.profiles.iter().flatten() {
            if !seen_profiles.insert(p.profile_ref.to_string()) {
                errors.push(RulesError::Duplicate(format!("profile {}", p.profile_ref)));
            }
            for id in &p.premises.value {
                if !templates.contains(id.as_str()) {
                    errors.push(misfit(
                        format!("profile {} premises", p.profile_ref),
                        format!("no template {id}"),
                    ));
                }
            }
        }
        for (rule, basis) in self.bases() {
            if let Basis::Sourced { note } = basis {
                let ok = note
                    .split_once(" §")
                    .is_some_and(|(file, section)| file.ends_with(".md") && !section.is_empty());
                if !ok {
                    errors.push(RulesError::BadNote {
                        rule,
                        note: note.clone(),
                    });
                }
            }
        }
        // A file with version 2's sections prices everything the designer can place from them.
        if self.profiles.is_some() {
            let priced: BTreeSet<&ComponentId> =
                self.costs.parts.iter().map(|c| &c.component_ref).collect();
            let families: BTreeSet<&str> = self
                .costs
                .plug_in_families
                .iter()
                .flatten()
                .map(|f| f.family.as_str())
                .collect();
            let mut named: Vec<&ComponentId> = self
                .version_2_parts()
                .into_iter()
                .map(|(_, c, _)| c)
                .collect();
            for role in &self.roles {
                named.push(&role.cable_ref.value);
                named.push(&role.amp_ref.value);
            }
            named.extend(self.drops.iter().map(|d| &d.cable_ref.value));
            named.push(&self.powering.supply_ref.value);
            named.push(&self.powering.battery_ref.value);
            for t in &self.premises {
                named.extend(
                    [
                        &t.ground_block_ref,
                        &t.drop_amp_ref,
                        &t.filter_ref,
                        &t.splitter_ref,
                    ]
                    .into_iter()
                    .flatten(),
                );
            }
            let mut missing = BTreeSet::new();
            for part in named {
                // A plug-in is priced by its family. A modem or a customer's device isn't plant a design builds.
                let class = library.get(part).map(|c| c.class());
                let by_family_or_unpriced = matches!(
                    class,
                    Some(ComponentClass::PlugIn | ComponentClass::Modem | ComponentClass::Cpe)
                );
                if !by_family_or_unpriced && !priced.contains(part) {
                    missing.insert(part.to_string());
                }
                let family = library.get(part).and_then(|c| match &c.spec {
                    ComponentSpec::Amp(a) => a.plugin_family.clone(),
                    ComponentSpec::Node(n) => n.active.plugin_family.clone(),
                    ComponentSpec::Rpd(r) => r.active.plugin_family.clone(),
                    ComponentSpec::Tap(t) => t.plugin_family.clone(),
                    ComponentSpec::PlugIn(p) => Some(p.family.clone()),
                    _ => None,
                });
                if let Some(family) = family
                    && !families.contains(family.as_str())
                {
                    missing.insert(format!("plug-in family {family}"));
                }
            }
            errors.extend(missing.into_iter().map(RulesError::MissingCost));
        }
    }

    /// The version 2 sections the designer reads, or what's missing.
    pub fn for_design(&self) -> Result<Complete<'_>, Vec<RulesError>> {
        let mut missing = Vec::new();
        let mut need = |present: bool, what: &str| {
            if !present {
                missing.push(RulesError::Incomplete(what.to_string()));
            }
        };
        need(self.profiles.is_some(), "profiles");
        need(self.conditioning.is_some(), "conditioning");
        need(self.branching.is_some(), "branching");
        need(self.premises_parts.is_some(), "premises_parts");
        need(self.fittings.is_some(), "fittings");
        need(self.candidates.is_some(), "candidates");
        need(self.node.max_actives.is_some(), "node.max_actives");
        need(
            self.powering.inserter_ref.is_some(),
            "powering.inserter_ref",
        );
        need(
            self.powering.min_margin_v.is_some(),
            "powering.min_margin_v",
        );
        need(self.structures.side.is_some(), "structures.side");
        need(
            self.structures.setback_ft.is_some(),
            "structures.setback_ft",
        );
        need(self.structures.pole_ref.is_some(), "structures.pole_ref");
        need(
            self.structures.pedestal_ref.is_some(),
            "structures.pedestal_ref",
        );
        need(self.fiber.cable_refs.is_some(), "fiber.cable_refs");
        need(self.fiber.closure_ref.is_some(), "fiber.closure_ref");
        need(self.fiber.panel_ref.is_some(), "fiber.panel_ref");
        need(self.fiber.connector_ref.is_some(), "fiber.connector_ref");
        need(self.fiber.jumper_ref.is_some(), "fiber.jumper_ref");
        need(
            self.fiber.duplex_jumper_ref.is_some(),
            "fiber.duplex_jumper_ref",
        );
        need(self.fiber.jumper_len_ft.is_some(), "fiber.jumper_len_ft");
        need(
            self.fiber.composite_index_max.is_some(),
            "fiber.composite_index_max",
        );
        need(
            self.costs.plug_in_families.is_some(),
            "costs.plug_in_families",
        );
        need(
            self.costs.ceiling_usd_per_home.is_some(),
            "costs.ceiling_usd_per_home",
        );
        for f in &self.tap_families {
            need(
                f.parts.is_some(),
                &format!("tap_families {} parts", f.ports),
            );
        }
        match (
            &self.profiles,
            &self.conditioning,
            &self.branching,
            &self.premises_parts,
            &self.fittings,
            &self.candidates,
        ) {
            (
                Some(profiles),
                Some(conditioning),
                Some(branching),
                Some(premises_parts),
                Some(fittings),
                Some(candidates),
            ) if missing.is_empty() => Ok(Complete {
                rules: self,
                profiles,
                conditioning,
                branching,
                premises_parts,
                fittings,
                candidates,
            }),
            _ => Err(missing),
        }
    }

    /// Every basis in the file, with the rule it's on, in file order.
    pub fn bases(&self) -> Vec<(String, &Basis)> {
        let mut out: Vec<(String, &Basis)> = Vec::new();
        for t in &self.tap_port {
            out.push((
                format!("tap_port {} at_hi", t.profile_ref),
                &t.at_hi_dbmv.basis,
            ));
            if let Some(r) = &t.at_lo_dbmv {
                out.push((format!("tap_port {} at_lo", t.profile_ref), &r.basis));
            }
            out.push((
                format!("tap_port {} tolerance", t.profile_ref),
                &t.tolerance_db.basis,
            ));
        }
        out.push((
            "premises_allowance drop".into(),
            &self.premises_allowance.drop_loss_db.basis,
        ));
        out.push((
            "premises_allowance house".into(),
            &self.premises_allowance.house_loss_db.basis,
        ));
        out.push((
            "end_of_line terminating".into(),
            &self.end_of_line.last_tap_terminating.basis,
        ));
        out.push((
            "end_of_line min".into(),
            &self.end_of_line.min_port_dbmv_at_hi.basis,
        ));
        for f in &self.tap_families {
            out.push((format!("tap_family {} values", f.ports), &f.values_db.basis));
            out.push((
                format!("tap_family {} terminating", f.ports),
                &f.terminating_values_db.basis,
            ));
        }
        for r in &self.roles {
            out.push((format!("role {:?} cable", r.role), &r.cable_ref.basis));
            out.push((format!("role {:?} amp", r.role), &r.amp_ref.basis));
        }
        for d in &self.drops {
            out.push((format!("drop {}", d.cable_ref.value), &d.max_len_ft.basis));
            out.push((
                format!("drop {} cable", d.cable_ref.value),
                &d.cable_ref.basis,
            ));
        }
        out.push(("node max_homes".into(), &self.node.max_homes.basis));
        out.push(("node rf_ports".into(), &self.node.rf_ports.basis));
        out.push(("node max_cascade".into(), &self.node.max_cascade.basis));
        let p = &self.powering;
        out.push(("powering supply".into(), &p.supply_ref.basis));
        out.push(("powering design_load".into(), &p.design_load_pct.basis));
        out.push(("powering port_max".into(), &p.port_max_a.basis));
        out.push(("powering battery".into(), &p.battery_ref.basis));
        out.push(("powering per_string".into(), &p.batteries_per_string.basis));
        out.push(("powering strings".into(), &p.battery_strings.basis));
        let s = &self.structures;
        out.push(("structures pole_spacing".into(), &s.pole_spacing_ft.basis));
        out.push((
            "structures pedestal_spacing".into(),
            &s.pedestal_spacing_ft.basis,
        ));
        out.push((
            "structures expansion_loop".into(),
            &s.expansion_loop_each_pole.basis,
        ));
        out.push(("structures strand".into(), &s.strand_ref.basis));
        out.push(("structures conduit".into(), &s.conduit_ref.basis));
        for t in &self.premises {
            out.push((format!("premises {} share", t.id), &t.share.basis));
            out.push((format!("premises {}", t.id), &t.basis));
        }
        let f = &self.fiber;
        out.push(("fiber analog".into(), &f.fibers_per_analog_node.basis));
        out.push(("fiber rpd".into(), &f.fibers_per_rpd.basis));
        out.push(("fiber counts".into(), &f.cable_counts.basis));
        out.push(("fiber reel".into(), &f.reel_len_ft.basis));
        out.push(("fiber margin".into(), &f.link_margin_db.basis));
        out.push(("fiber splice".into(), &f.splice_loss_db.basis));
        out.push(("fiber connector".into(), &f.connector_loss_db.basis));
        for c in &self.costs.parts {
            out.push((format!("cost {}", c.component_ref), &c.usd.basis));
        }
        for c in &self.costs.construction {
            out.push((format!("cost {:?}", c.kind), &c.usd_per_ft.basis));
        }
        self.version_2_bases(&mut out);
        out
    }

    /// Version 2's bases, in file order after version 1's.
    fn version_2_bases<'a>(&'a self, out: &mut Vec<(String, &'a Basis)>) {
        let mut push = |rule: String, basis: &'a Basis| out.push((rule, basis));
        for t in &self.tap_port {
            if let Some(r) = &t.tilt_tolerance_db {
                push(
                    format!("tap_port {} tilt_tolerance", t.profile_ref),
                    &r.basis,
                );
            }
        }
        for f in &self.tap_families {
            if let Some(r) = &f.parts {
                push(format!("tap_family {} parts", f.ports), &r.basis);
            }
        }
        if let Some(r) = &self.node.max_actives {
            push("node max_actives".into(), &r.basis);
        }
        let p = &self.powering;
        for (rule, r) in [
            (
                "powering inserter",
                p.inserter_ref.as_ref().map(|r| &r.basis),
            ),
            (
                "powering min_margin",
                p.min_margin_v.as_ref().map(|r| &r.basis),
            ),
        ] {
            if let Some(b) = r {
                push(rule.into(), b);
            }
        }
        let s = &self.structures;
        if let Some(r) = &s.side {
            push("structures side".into(), &r.basis);
        }
        if let Some(r) = &s.setback_ft {
            push("structures setback".into(), &r.basis);
        }
        if let Some(r) = &s.pole_ref {
            push("structures pole".into(), &r.basis);
        }
        if let Some(r) = &s.pedestal_ref {
            push("structures pedestal".into(), &r.basis);
        }
        let f = &self.fiber;
        if let Some(r) = &f.cable_refs {
            push("fiber cables".into(), &r.basis);
        }
        for (rule, r) in [
            ("fiber closure", &f.closure_ref),
            ("fiber panel", &f.panel_ref),
            ("fiber connector", &f.connector_ref),
            ("fiber jumper", &f.jumper_ref),
            ("fiber duplex jumper", &f.duplex_jumper_ref),
        ] {
            if let Some(r) = r {
                push(rule.into(), &r.basis);
            }
        }
        if let Some(r) = &f.jumper_len_ft {
            push("fiber jumper_len".into(), &r.basis);
        }
        if let Some(r) = &f.composite_index_max {
            push("fiber composite_index_max".into(), &r.basis);
        }
        for c in self.costs.plug_in_families.iter().flatten() {
            push(format!("cost family {}", c.family), &c.usd.basis);
        }
        if let Some(r) = &self.costs.ceiling_usd_per_home {
            push("cost ceiling".into(), &r.basis);
        }
        for p in self.profiles.iter().flatten() {
            let id = &p.profile_ref;
            push(format!("profile {id} root"), &p.root_ref.basis);
            push(
                format!("profile {id} station out"),
                &p.station.out_dbmv_at_hi.basis,
            );
            push(
                format!("profile {id} station min_out"),
                &p.station.min_out_dbmv_at_hi.basis,
            );
            push(
                format!("profile {id} station tilt"),
                &p.station.tilt_db.basis,
            );
            push(
                format!("profile {id} station min_tilt"),
                &p.station.min_tilt_db.basis,
            );
            push(format!("profile {id} premises"), &p.premises.basis);
            push(format!("profile {id} optics"), &p.optics.basis);
        }
        if let Some(c) = &self.conditioning {
            push("conditioning conditioners".into(), &c.conditioners.basis);
            push(
                "conditioning line_equalizers".into(),
                &c.line_equalizers.basis,
            );
        }
        if let Some(b) = &self.branching {
            push("branching splitter".into(), &b.splitter_ref.basis);
            push("branching couplers".into(), &b.couplers.basis);
        }
        if let Some(p) = &self.premises_parts {
            push("premises_parts d30".into(), &p.modem_d30_ref.basis);
            push("premises_parts d31".into(), &p.modem_d31_ref.basis);
            push("premises_parts cpe".into(), &p.cpe_refs.basis);
            push(
                "premises_parts house cable".into(),
                &p.house_cable_ref.basis,
            );
            push("premises_parts unused port".into(), &p.unused_port.basis);
            push("premises_parts terminator".into(), &p.terminator_ref.basis);
            push(
                "premises_parts drop amp below".into(),
                &p.drop_amp_below_dbmv.basis,
            );
        }
        if let Some(f) = &self.fittings {
            push("fittings connectors".into(), &f.connectors.basis);
            push("fittings reel".into(), &f.hardline_reel_len_ft.basis);
            push("fittings jumper".into(), &f.jumper_len_ft.basis);
        }
        if let Some(c) = &self.candidates {
            push(
                "candidates node_count_steps".into(),
                &c.node_count_steps.basis,
            );
            push("candidates express_cables".into(), &c.express_cables.basis);
        }
        if let Some(u) = &self.upgrade {
            for m in &u.moves {
                push(format!("upgrade {:?}", m.value), &m.basis);
            }
        }
    }
}

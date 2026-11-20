//! The tools: everything anyone can do with the twin. The page calls these, and so can any MCP client
//! (invariant 8).

use schema::common::{ElementId, LonLat, Origin, ServiceGroupId, Timestamp};
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::Fault;
use schema::plant::{EdgeKind, ElementKind, Installation, RecordStatus};
use schema::region::MapSource;
use schema::work_order::WorkOrderStatus;
use schemars::{JsonSchema, schema_for};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sim::Overlay;
use sim::budget::{check_physics, level_budget};
use sim::telemetry::{Detail, Scenario, SimOptions, simulate};
use thiserror::Error;

use crate::state::{DesignInput, Twin, TwinError};

/// A tool's description, for `tools/list`.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    /// Its name.
    pub name: &'static str,
    /// What it does, for whoever's choosing.
    pub description: &'static str,
    /// Its input's JSON Schema.
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// A call that couldn't be done.
#[derive(Error, Debug)]
pub enum ToolError {
    /// No such tool.
    #[error("no tool {0}")]
    Unknown(String),
    /// Its arguments didn't parse.
    #[error("bad arguments: {0}")]
    BadArguments(String),
    /// The twin refused or failed.
    #[error(transparent)]
    Twin(#[from] TwinError),
}

/// No arguments.
#[derive(Deserialize, JsonSchema)]
pub struct NoArgs {}

/// A service group.
#[derive(Deserialize, JsonSchema)]
pub struct GroupArgs {
    /// The service group.
    pub service_group: ServiceGroupId,
}

/// `set_environment`.
#[derive(Deserialize, JsonSchema)]
pub struct EnvironmentArgs {
    /// Simulated time, seconds, with 0 at midnight.
    pub t: Option<i64>,
    /// Air temperature, °C.
    pub temp_c: Option<f64>,
    /// Ground temperature, °C.
    pub ground_temp_c: Option<f64>,
    /// Rain, mm per hour.
    pub rain_mm_per_hr: Option<f64>,
}

/// `inject_fault` and `what_if`.
#[derive(Deserialize, JsonSchema)]
pub struct FaultArgs {
    /// The service group.
    pub service_group: ServiceGroupId,
    /// The fault, as physical parameters.
    pub fault: Fault,
}

/// `inject_fault`: a fault on one node's plant, or a fiber fault on a region's fiber plant, which can reach several nodes.
#[derive(Deserialize, JsonSchema)]
pub struct InjectArgs {
    /// The service group, for a fault on its plant.
    pub service_group: Option<ServiceGroupId>,
    /// The fault, as physical parameters.
    pub fault: Option<Fault>,
    /// The region, for a fiber fault.
    pub region: Option<String>,
    /// The fiber fault, as physical parameters at a fiber location (§16).
    pub fiber_fault: Option<schema::fiber_fault::FiberFault>,
}

/// `clear_faults`: one node's, or a region's fiber faults.
#[derive(Deserialize, JsonSchema)]
pub struct ClearArgs {
    /// The service group whose faults to clear.
    pub service_group: Option<ServiceGroupId>,
    /// The region whose fiber faults to clear.
    pub region: Option<String>,
}

/// `get_modem_telemetry`.
#[derive(Deserialize, JsonSchema)]
pub struct ModemArgs {
    /// The service group.
    pub service_group: ServiceGroupId,
    /// The modem.
    pub modem: ElementId,
    /// Every subcarrier, rather than a decimated summary.
    #[serde(default)]
    pub full: bool,
    /// Read it under these faults instead of the node's own, to compare a hypothesis with what was seen.
    pub assume_faults: Option<Vec<Fault>>,
}

/// `get_level_budget` and `get_design_issues`.
#[derive(Deserialize, JsonSchema)]
pub struct BudgetArgs {
    /// The service group.
    pub service_group: ServiceGroupId,
    /// Temperature to check at, °C. Defaults to now.
    pub temp_c: Option<f64>,
}

/// A work order.
#[derive(Deserialize, JsonSchema)]
pub struct OrderArgs {
    /// The service group.
    pub service_group: ServiceGroupId,
    /// The order.
    pub id: uuid::Uuid,
}

/// `generate_layout`.
#[derive(Deserialize, JsonSchema)]
pub struct GenerateArgs {
    /// The family.
    pub family: schema::region::LayoutFamily,
    /// The seed.
    pub seed: u64,
    /// About how many homes.
    pub homes: usize,
}

/// `design_region`.
#[derive(Deserialize, JsonSchema)]
pub struct DesignArgs {
    /// A map from `generate_layout` or an import.
    pub map_id: String,
    /// The profile every node runs.
    pub profile: schema::common::ProfileId,
    /// Design controls. Defaults if left out.
    pub controls: Option<schema::region::DesignControls>,
}

/// Refuses a value outside `lo..=hi`, naming the field and the range in words a person can act on.
fn within(field: &str, value: f64, lo: f64, hi: f64, unit: &str) -> Result<(), String> {
    if value.is_finite() && (lo..=hi).contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "{field} is {value}{unit}; it must be from {lo}{unit} to {hi}{unit}"
        ))
    }
}

impl EnvironmentArgs {
    /// Air and ground from −50 to 60 °C, which covers any plant on Earth, and rain from 0 to 300 mm/h, above the
    /// heaviest hourly rain recorded.
    pub fn check(&self) -> Result<(), String> {
        if let Some(t) = self.temp_c {
            within("temp_c", t, -50.0, 60.0, " °C")?;
        }
        if let Some(t) = self.ground_temp_c {
            within("ground_temp_c", t, -50.0, 60.0, " °C")?;
        }
        if let Some(r) = self.rain_mm_per_hr {
            within("rain_mm_per_hr", r, 0.0, 300.0, " mm/h")?;
        }
        if let Some(t) = self.t
            && t < 0
        {
            return Err(format!("t is {t} s; simulated time starts at 0"));
        }
        Ok(())
    }
}

/// The fewest and most homes `generate_layout` takes. Below ten there's no street to lay out; above the top, one
/// call would build a city-sized map in the twin's lock.
pub const GENERATE_HOMES: (usize, usize) = (10, 20_000);

impl GenerateArgs {
    /// Refuses a map too small to lay out or too big for one call.
    pub fn check(&self) -> Result<(), String> {
        let (lo, hi) = GENERATE_HOMES;
        if (lo..=hi).contains(&self.homes) {
            Ok(())
        } else {
            Err(format!(
                "homes is {}; it must be from {lo} to {hi}",
                self.homes
            ))
        }
    }
}

/// The fewest homes a node may be designed for. Fewer makes a node per house or two, which no operator builds.
pub const MIN_HOMES_PER_NODE: u32 = 10;

/// Refuses design controls no one would design to: shares outside 0 to 1, a node per home, a drop longer than a block.
pub fn check_controls(c: &schema::region::DesignControls) -> Result<(), String> {
    if c.homes_per_node < MIN_HOMES_PER_NODE {
        return Err(format!(
            "homes_per_node is {}; it must be at least {MIN_HOMES_PER_NODE}",
            c.homes_per_node
        ));
    }
    if !(c.take_rate.is_finite() && c.take_rate > 0.0 && c.take_rate <= 1.0) {
        return Err(format!(
            "take_rate is {}; it's a share of homes, above 0 and at most 1",
            c.take_rate
        ));
    }
    within("d30_share", c.d30_share, 0.0, 1.0, "")?;
    within("max_drop_ft", c.max_drop_ft, 10.0, 1000.0, " ft")?;
    Ok(())
}

/// The largest area one import may ask for, km². Overpass is a shared public service, and a region is well inside this.
pub const MAX_IMPORT_KM2: f64 = 40.0;

/// `import_osm_area`.
#[derive(Deserialize, JsonSchema, Debug, Clone)]
pub struct AreaArgs {
    /// Southern edge, degrees.
    pub south_deg: f64,
    /// Western edge, degrees.
    pub west_deg: f64,
    /// Northern edge, degrees.
    pub north_deg: f64,
    /// Eastern edge, degrees.
    pub east_deg: f64,
    /// How plant runs along every street. OpenStreetMap doesn't record it. Aerial if left out.
    pub installation: Option<Installation>,
}

impl AreaArgs {
    /// The map's id, so asking for the same box again finds the same map.
    pub fn map_id(&self) -> String {
        format!(
            "osm_{:.4}_{:.4}_{:.4}_{:.4}",
            self.south_deg, self.west_deg, self.north_deg, self.east_deg
        )
    }

    /// Its size, km².
    pub fn km2(&self) -> f64 {
        const FT_PER_KM: f64 = 3280.84;
        let at = |lon_deg, lat_deg| LonLat { lon_deg, lat_deg };
        let wide = design::geo::dist_ft(
            at(self.west_deg, self.south_deg),
            at(self.east_deg, self.south_deg),
        );
        let tall = design::geo::dist_ft(
            at(self.west_deg, self.south_deg),
            at(self.west_deg, self.north_deg),
        );
        wide * tall / (FT_PER_KM * FT_PER_KM)
    }

    /// Refuses a box that's inside out, off the planet or too big.
    pub fn check(&self) -> Result<(), String> {
        let lat_ok = |d: f64| (-90.0..=90.0).contains(&d);
        let lon_ok = |d: f64| (-180.0..=180.0).contains(&d);
        if !(lat_ok(self.south_deg) && lat_ok(self.north_deg))
            || !(lon_ok(self.west_deg) && lon_ok(self.east_deg))
            || self.south_deg >= self.north_deg
            || self.west_deg >= self.east_deg
        {
            return Err("that isn't a box: south below north, west left of east".to_string());
        }
        let km2 = self.km2();
        if km2 > MAX_IMPORT_KM2 {
            return Err(format!(
                "{km2:.1} km² is more than the {MAX_IMPORT_KM2:.0} km² one import can ask for"
            ));
        }
        Ok(())
    }

    /// Its middle.
    pub fn centre(&self) -> LonLat {
        LonLat {
            lon_deg: (self.west_deg + self.east_deg) / 2.0,
            lat_deg: (self.south_deg + self.north_deg) / 2.0,
        }
    }
}

/// `import_osm`.
#[derive(Deserialize, JsonSchema)]
pub struct ImportArgs {
    /// The id to give the map.
    pub map_id: String,
    /// An Overpass JSON extract. Leave it out to read back a map already imported.
    pub overpass_json: Option<String>,
    /// How plant runs along every street. Aerial if left out.
    pub installation: Option<Installation>,
    /// Where fiber comes from. The first street's start if left out.
    pub headend: Option<LonLat>,
}

/// A map, by id.
#[derive(Deserialize, JsonSchema)]
pub struct MapArgs {
    /// The map.
    pub map_id: String,
}

/// `get_fiber_plant`.
#[derive(Deserialize, JsonSchema)]
pub struct FiberArgs {
    /// The fiber plant, as a region's `fiber_ref` names it.
    pub fiber_id: String,
}

/// `discard_map`.
#[derive(Deserialize, JsonSchema)]
pub struct DiscardArgs {
    /// The map.
    pub map_id: String,
    /// Discard even if its nodes hold built records, faults, work orders or a migration, which go with them, and any verified labels too.
    pub force: Option<bool>,
}

/// `set_headend`.
#[derive(Deserialize, JsonSchema)]
pub struct HeadendArgs {
    /// The map.
    pub map_id: String,
    /// Where fiber comes from.
    pub headend: LonLat,
}

/// `plan_migration` and `apply_migration`.
#[derive(Deserialize, JsonSchema)]
pub struct MigrationArgs {
    /// The node.
    pub service_group: ServiceGroupId,
    /// The profile to compare or move to.
    pub to_profile: schema::common::ProfileId,
}

fn spec<T: JsonSchema>(name: &'static str, description: &'static str) -> ToolSpec {
    let schema = schema_for!(T);
    ToolSpec {
        name,
        description,
        input_schema: serde_json::to_value(schema).unwrap_or_else(|_| json!({"type": "object"})),
    }
}

/// Every tool.
pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        spec::<crate::operate::StartArgs>(
            "ops_start",
            "Starts a designed region (by id, or the last by id whose map is loaded) running day to day (task 26): two worlds on one seed, one dispatching from calls and one with the twin. Hazards are physical faults at sourced rates times an intensity; households call from what their modems show; trucks drive the streets. A scenario from ops_scenarios writes its actions into both worlds ahead. All synthetic.",
        ),
        spec::<crate::operate::RunArgs>("ops_run", "Runs both worlds on by world minutes."),
        spec::<crate::operate::StateArgs>(
            "ops_state",
            "The worlds now, each with its homes not working, trucks with their routes and times, jobs, tickets, calls and story since a time, and what's really wrong (the audience's view, never the dispatcher's); and what both share: the clock, the sky, both worlds' counts, a chart's series and the actions taken.",
        ),
        spec::<crate::operate::ActArgs>(
            "ops_act",
            "Something happens in both worlds, from the next minute or at a later world time: a storm, a heat wave, a power cut, a dig-in, a car into the plant, a fiber cut.",
        ),
        spec::<NoArgs>(
            "ops_rates",
            "The rates the worlds run on, each with its source and confidence.",
        ),
        spec::<NoArgs>(
            "ops_layout",
            "What doesn't change while the worlds run, to draw the region: every served home and its node, the hub, the streets, the plant's spans and node sites, the fiber, the bounds, and whether the streets are a real place's.",
        ),
        spec::<NoArgs>(
            "ops_scenarios",
            "Demo days written ahead (task 28): each a seed, an intensity, and the user's actions at world times, for ops_start.",
        ),
        spec::<NoArgs>(
            "list_service_groups",
            "Every node in the twin, with its profile and whether it's synthetic.",
        ),
        spec::<GroupArgs>(
            "get_plant",
            "A node's plant as GeoJSON: elements, spans, drops, supplies, each with its record status.",
        ),
        spec::<GroupArgs>(
            "get_parts_list",
            "A node's parts list, derived from its plant.",
        ),
        spec::<FiberArgs>(
            "get_fiber_plant",
            "A fiber plant as stored: hubs, CMTS ports, optics, cables, closures and links, each with its record status, with its parts list and whether every node it serves has a link each way. Read only.",
        ),
        spec::<BudgetArgs>(
            "get_level_budget",
            "Per modem: level range, tilt, total input and transmit power against its limit, with any injected faults.",
        ),
        spec::<BudgetArgs>(
            "get_design_issues",
            "What a healthy version of this plant violates: cascade, parts, powering, level window, voltage, transmit limit.",
        ),
        spec::<EnvironmentArgs>(
            "set_environment",
            "Sets simulated time, temperature and rain. Fields left out keep their value.",
        ),
        spec::<InjectArgs>(
            "inject_fault",
            "Adds a fault to the sim, as physical parameters: a fault on a node's plant (service_group and fault), or a fiber fault on a region's fiber plant (region and fiber_fault), which reaches every node whose links cross it. Its telemetry signature is computed, never scripted.",
        ),
        spec::<GroupArgs>(
            "list_faults",
            "The faults injected into a node, and the fiber faults on its region.",
        ),
        spec::<ClearArgs>(
            "clear_faults",
            "Removes every injected fault from a node (service_group), or every fiber fault from a region (region).",
        ),
        spec::<GroupArgs>(
            "get_optical_budget",
            "A node's optical links: each hop's loss, received power, CNR or margin, whether it's up and its latency, under the region's fiber faults, with any optical margin issue. Null where no fiber plant serves the node.",
        ),
        spec::<ModemArgs>(
            "get_modem_telemetry",
            "One modem's observation in the PNM schema, as a device would report it.",
        ),
        spec::<GroupArgs>(
            "get_status_map",
            "Per modem and active: features relative to design and siblings, and whether it looks affected.",
        ),
        spec::<FaultArgs>(
            "what_if",
            "What a node's status would be under one more fault. Changes nothing.",
        ),
        spec::<GroupArgs>(
            "diagnose",
            "Who is affected, causes ranked most likely first with parameters and confidence, and the work order for the first one. No order when healthy leads or nobody can feel a low-confidence fault; order_withheld_reason says which.",
        ),
        spec::<GroupArgs>("list_work_orders", "Work orders issued against a node."),
        spec::<NoArgs>(
            "get_region_status",
            "Every node at once: modems, how many look affected, how many are down, injected faults and open work orders. Only what changed is recomputed.",
        ),
        spec::<NoArgs>(
            "get_recompute_stats",
            "How many answers came from the cache, and how many recomputes each service group has had.",
        ),
        spec::<GroupArgs>(
            "localize_ingress",
            "Attenuates each amp's return leg in turn, as a technician's switch test does, and reports which leg the node's noise follows, and whether the node's upstream reads degraded at all.",
        ),
        spec::<OrderArgs>(
            "dispatch_work_order",
            "Sends a proposed work order to a crew.",
        ),
        spec::<OrderArgs>(
            "complete_work_order",
            "Records that a crew finished. It repairs what's actually on the component the order names, and nothing else.",
        ),
        spec::<OrderArgs>(
            "verify_work_order",
            "Reads fresh telemetry and decides whether the symptoms cleared: Verified or NotVerified.",
        ),
        spec::<NoArgs>(
            "list_verified_labels",
            "Telemetry-verified outcomes across the twin, as training labels. All synthetic until field data exists.",
        ),
        spec::<GenerateArgs>(
            "generate_layout",
            "Generates streets and homes by family and seed. Invented geography, labeled as such.",
        ),
        spec::<DesignArgs>(
            "design_region",
            "Designs a whole map for a profile, adding its nodes to the twin with every element Designed, and returns the parts lists and any issues. With `controls.rules_id`, the version 2 designer runs by those design rules: structures, express and feeder, taps to a port level, homes, alignment, powering, a stored fiber plant, and a cost, the cheapest of a fixed candidate set kept; `controls.mode` LegacyThenUpgrade designs under legacy rules and profile first and upgrades in place.",
        ),
        spec::<MapArgs>(
            "get_design_cost",
            "A version 2 design's cost, worked out from its records and the rules it was designed by: each part, construction by the foot, each node's share and per home passed, with every candidate's cost from the last design. The generic prices are the rules' own, Generic, and the result says so.",
        ),
        spec::<NoArgs>(
            "list_design_rules",
            "The design rules files the version 2 designer can design by: the profiles each covers, tap-port targets, node size, the cost ceiling, the upgrade moves, and how many values are sourced and how many Generic.",
        ),
        spec::<AreaArgs>(
            "import_osm_area",
            "Fetches a lat/lon box of real streets and buildings from OpenStreetMap and imports it as a map. The geography is real and any plant designed on it is invented. Asking for the same box again returns the map already there.",
        ),
        spec::<ImportArgs>(
            "import_osm",
            "Imports an Overpass JSON extract the caller already has as a map, without going to the network.",
        ),
        spec::<NoArgs>(
            "list_maps",
            "Every map in the twin: where it came from, its size and how many nodes are designed on it.",
        ),
        spec::<MapArgs>(
            "get_map",
            "A map as GeoJSON: streets and homes, the headend, what it was last designed with, each node's site and spans, the fiber route, and which node, if any, locks it against re-solving and why.",
        ),
        spec::<HeadendArgs>(
            "set_headend",
            "Moves a map's headend and returns the new fiber route. The sim has no optical budget, so this changes no RF design.",
        ),
        spec::<DiscardArgs>(
            "discard_map",
            "Removes a map, its region and its nodes. Refused while any node holds a built element, a fault, a work order past Proposed or a migration, unless force says to throw those away too.",
        ),
        spec::<GroupArgs>(
            "mark_built",
            "Moves a node's records from Designed to Built.",
        ),
        spec::<MigrationArgs>(
            "plan_migration",
            "Compares a node on another profile: capacity each way, transmit headroom, parts that must change, modems that lose service. Changes nothing.",
        ),
        spec::<MigrationArgs>(
            "apply_migration",
            "Moves a node to another profile, with its changed parts marked Designed.",
        ),
        spec::<ModemArgs>(
            "get_modem_identity",
            "A modem's pseudonym. Refused unless the server was started allowing it.",
        ),
    ]
}

fn args<T: DeserializeOwned>(value: &Value) -> Result<T, ToolError> {
    serde_json::from_value(value.clone()).map_err(|e| ToolError::BadArguments(e.to_string()))
}

/// Runs one tool. Everything returned is JSON the sim could have produced, with no NaN or infinity.
pub fn call_tool(twin: &mut Twin, name: &str, arguments: &Value) -> Result<Value, ToolError> {
    match name {
        "list_service_groups" => Ok(list_groups(twin)),
        "ops_start" => {
            let a: crate::operate::StartArgs = args(arguments)?;
            Ok(crate::operate::start(twin, &a)?)
        }
        "ops_run" => {
            let a: crate::operate::RunArgs = args(arguments)?;
            Ok(crate::operate::run(twin, &a)?)
        }
        "ops_state" => {
            let a: crate::operate::StateArgs = args(arguments)?;
            Ok(crate::operate::state(twin, &a)?)
        }
        "ops_act" => {
            let a: crate::operate::ActArgs = args(arguments)?;
            Ok(crate::operate::act(twin, &a)?)
        }
        "ops_rates" => Ok(crate::operate::rates()?),
        "ops_layout" => Ok(crate::operate::layout(twin)?),
        "ops_scenarios" => Ok(crate::operate::scenarios()?),
        "get_plant" => {
            let a: GroupArgs = args(arguments)?;
            Ok(plant_geojson(twin, &a.service_group)?)
        }
        "get_fiber_plant" => {
            let a: FiberArgs = args(arguments)?;
            Ok(fiber_plant_json(twin, &a.fiber_id)?)
        }
        "get_parts_list" => {
            let a: GroupArgs = args(arguments)?;
            let node = twin.node(&a.service_group)?;
            Ok(json!({
                "origin": node.plant.origin,
                "parts": node.plant.parts_list(),
            }))
        }
        "get_level_budget" => {
            let a: BudgetArgs = args(arguments)?;
            Ok(budget(twin, &a)?)
        }
        "get_design_issues" => {
            let a: BudgetArgs = args(arguments)?;
            Ok(design_issues(twin, &a)?)
        }
        "set_environment" => {
            let a: EnvironmentArgs = args(arguments)?;
            a.check().map_err(ToolError::BadArguments)?;
            let sample = &mut twin.sample;
            if let Some(t) = a.t {
                sample.t = Timestamp(t);
            }
            if let Some(temp_c) = a.temp_c {
                sample.temp_c = temp_c;
            }
            if let Some(ground_temp_c) = a.ground_temp_c {
                sample.ground_temp_c = ground_temp_c;
            }
            if let Some(rain) = a.rain_mm_per_hr {
                sample.rain_mm_per_hr = rain;
            }
            Ok(serde_json::to_value(&twin.sample).unwrap_or(Value::Null))
        }
        "inject_fault" => {
            let a: InjectArgs = args(arguments)?;
            match (a.service_group, a.fault, a.region, a.fiber_fault) {
                (Some(group), Some(fault), None, None) => {
                    check_fault(twin, &group, &fault)?;
                    let node = twin.node_mut(&group)?;
                    node.faults.push(fault);
                    Ok(json!({"origin": Origin::Synthetic, "faults": node.faults}))
                }
                (None, None, Some(region), Some(fiber_fault)) => {
                    check_fiber_fault(twin, &region, &fiber_fault)?;
                    let faults = twin.fiber_faults.entry(region.clone()).or_default();
                    faults.push(fiber_fault);
                    Ok(json!({"origin": Origin::Synthetic, "region": region, "fiber_faults": faults}))
                }
                _ => Err(ToolError::BadArguments(
                    "give service_group and fault, or region and fiber_fault".into(),
                )),
            }
        }
        "list_faults" => {
            let a: GroupArgs = args(arguments)?;
            let node = twin.node(&a.service_group)?;
            let region = twin.region_of(&a.service_group).map(|r| r.id.clone());
            let fiber_faults = region
                .as_ref()
                .and_then(|r| twin.fiber_faults.get(r))
                .cloned()
                .unwrap_or_default();
            Ok(json!({
                "origin": Origin::Synthetic,
                "faults": node.faults,
                "region": region,
                "fiber_faults": fiber_faults,
            }))
        }
        "clear_faults" => {
            let a: ClearArgs = args(arguments)?;
            match (a.service_group, a.region) {
                (Some(group), None) => {
                    let node = twin.node_mut(&group)?;
                    node.faults.clear();
                    Ok(json!({"origin": Origin::Synthetic, "faults": node.faults}))
                }
                (None, Some(region)) => {
                    twin.fiber_faults.remove(&region);
                    Ok(json!({"origin": Origin::Synthetic, "region": region, "fiber_faults": []}))
                }
                _ => Err(ToolError::BadArguments(
                    "give service_group or region, not both".into(),
                )),
            }
        }
        "get_optical_budget" => {
            let a: GroupArgs = args(arguments)?;
            Ok(optical_budget(twin, &a.service_group)?)
        }
        "get_modem_telemetry" => {
            let a: ModemArgs = args(arguments)?;
            Ok(modem_telemetry(twin, &a)?)
        }
        "get_status_map" => {
            let a: GroupArgs = args(arguments)?;
            let features = twin.features(&a.service_group)?;
            Ok(status_map(twin, &a.service_group, &features)?)
        }
        "what_if" => {
            let a: FaultArgs = args(arguments)?;
            check_fault(twin, &a.service_group, &a.fault)?;
            let mut faults = twin.node(&a.service_group)?.faults.clone();
            faults.push(a.fault);
            let features = twin.features_under(&a.service_group, &faults)?;
            Ok(status_map(twin, &a.service_group, &features)?)
        }
        "diagnose" => {
            let a: GroupArgs = args(arguments)?;
            Ok(run_diagnosis(twin, &a.service_group)?)
        }
        "list_work_orders" => {
            let a: GroupArgs = args(arguments)?;
            let node = twin.node(&a.service_group)?;
            Ok(json!({"origin": Origin::Synthetic, "work_orders": node.work_orders}))
        }
        "get_region_status" => Ok(region_status(twin)?),
        "get_recompute_stats" => {
            Ok(serde_json::to_value(twin.cache.stats()).map_err(TwinError::from)?)
        }
        "localize_ingress" => {
            let a: GroupArgs = args(arguments)?;
            let mut found = localize(twin, &a.service_group)?;
            // Whether there's noise to find at all, so "not below an amp" isn't read as "no ingress".
            let features = twin.features(&a.service_group)?;
            let who = twin.who(&a.service_group, &features)?;
            found["upstream_degraded"] = json!(who.upstream_degraded);
            Ok(found)
        }
        "dispatch_work_order" => {
            let a: OrderArgs = args(arguments)?;
            let order = crate::lifecycle::dispatch(twin, &a.service_group, a.id)?;
            Ok(serde_json::to_value(order).map_err(TwinError::from)?)
        }
        "complete_work_order" => {
            let a: OrderArgs = args(arguments)?;
            Ok(crate::lifecycle::complete(twin, &a.service_group, a.id)?)
        }
        "verify_work_order" => {
            let a: OrderArgs = args(arguments)?;
            Ok(crate::lifecycle::verify(twin, &a.service_group, a.id)?)
        }
        "list_verified_labels" => Ok(crate::lifecycle::labels(twin)),
        "generate_layout" => {
            let a: GenerateArgs = args(arguments)?;
            a.check().map_err(ToolError::BadArguments)?;
            let map = design::generate(a.family, a.seed, a.homes);
            let summary = json!({
                "map_id": map.id,
                "source": map.source,
                "homes": map.homes.len(),
                "streets": map.streets.len(),
            });
            twin.maps.insert(map.id.clone(), map);
            Ok(summary)
        }
        "import_osm_area" => Err(TwinError::Refused(
            "this twin isn't connected to a server that can reach OpenStreetMap; use import_osm with an extract".to_string(),
        )
        .into()),
        "import_osm" => {
            let a: ImportArgs = args(arguments)?;
            Ok(import_osm(twin, a)?)
        }
        "list_maps" => Ok(list_maps(twin)),
        "get_design_cost" => {
            let a: MapArgs = args(arguments)?;
            Ok(design_cost(twin, &a.map_id)?)
        }
        "list_design_rules" => Ok(list_design_rules(twin)),
        "get_map" => {
            let a: MapArgs = args(arguments)?;
            Ok(get_map(twin, &a.map_id)?)
        }
        "set_headend" => {
            let a: HeadendArgs = args(arguments)?;
            let map = twin
                .maps
                .get_mut(&a.map_id)
                .ok_or_else(|| TwinError::Refused(format!("no map {}", a.map_id)))?;
            map.headend = a.headend;
            let map = &twin.maps[&a.map_id];
            Ok(json!({"map_id": a.map_id, "headend": map.headend, "fiber": fiber_json(twin, map)}))
        }
        "discard_map" => {
            let a: DiscardArgs = args(arguments)?;
            if !twin.maps.contains_key(&a.map_id) {
                return Err(TwinError::Refused(format!("no map {}", a.map_id)).into());
            }
            if a.force != Some(true) {
                refuse_if_recorded(twin, &a.map_id, "discard this map")?;
            }
            let nodes = nodes_on(twin, &a.map_id);
            for id in &nodes {
                twin.nodes.remove(id);
            }
            twin.maps.remove(&a.map_id);
            let region_id = format!("region-{}", a.map_id);
            if let Some(old) = twin.regions.get(&region_id).and_then(|r| r.fiber_ref.clone()) {
                twin.fiber_plants.remove(&old);
            }
            twin.fiber_faults.remove(&region_id);
            twin.design_reports.remove(&a.map_id);
            twin.regions.remove(&region_id);
            twin.design_inputs.remove(&a.map_id);
            twin.osm_reports.remove(&a.map_id);
            Ok(json!({"map_id": a.map_id, "nodes_removed": nodes.len()}))
        }
        "design_region" => {
            let a: DesignArgs = args(arguments)?;
            if let Some(controls) = &a.controls {
                check_controls(controls).map_err(ToolError::BadArguments)?;
            }
            Ok(design_tool(twin, &a)?)
        }
        "mark_built" => {
            let a: GroupArgs = args(arguments)?;
            let node = twin.node_mut(&a.service_group)?;
            design::optimize::mark_all_built(&mut node.plant);
            Ok(
                json!({"origin": node.plant.origin, "records": design::optimize::record_counts(&node.plant)}),
            )
        }
        "plan_migration" => {
            let a: MigrationArgs = args(arguments)?;
            let plan = migration_plan(twin, &a)?;
            let mut value = serde_json::to_value(&plan).map_err(TwinError::from)?;
            // The converted plant is what `apply_migration` is for; it'd swamp a reader here.
            if let Some(map) = value.as_object_mut() {
                map.remove("converted_plant");
            }
            Ok(value)
        }
        "apply_migration" => {
            let a: MigrationArgs = args(arguments)?;
            let plan = migration_plan(twin, &a)?;
            let node = twin.node_mut(&a.service_group)?;
            node.plant = plan.converted_plant;
            node.faults.clear();
            Ok(json!({
                "origin": node.plant.origin,
                "profile": node.plant.profile_ref,
                "issues_after": plan.issues_after,
            }))
        }
        "get_modem_identity" => {
            let a: ModemArgs = args(arguments)?;
            if !twin.identity_allowed {
                return Err(TwinError::Refused(
                    "subscriber identity is gated; start the server with --allow-identity".into(),
                )
                .into());
            }
            let node = twin.node(&a.service_group)?;
            let pseudonym = node
                .plant
                .modem_bindings
                .iter()
                .find(|b| b.element_id == a.modem)
                .map(|b| b.pseudonym.clone());
            Ok(json!({"modem": a.modem, "pseudonym": pseudonym}))
        }
        other => Err(ToolError::Unknown(other.to_string())),
    }
}

fn region_status(twin: &Twin) -> Result<Value, TwinError> {
    let ids: Vec<ServiceGroupId> = twin.nodes.keys().cloned().collect();
    let features = twin.features_many(&ids)?;
    let mut nodes = Vec::new();
    for (id, step) in ids.iter().zip(&features) {
        let who = twin.who(id, step)?;
        let node = twin.node(id)?;
        let geo = node.plant.root().and_then(|r| r.geo);
        nodes.push(json!({
            "service_group": id,
            "profile": node.plant.profile_ref,
            "origin": node.plant.origin,
            "geo": geo,
            "modems": step.modems.len(),
            "affected": who.affected.len(),
            "out_of_service": who.out_of_service,
            "upstream_degraded": who.upstream_degraded,
            "rpd_abnormal": who.rpd_abnormal,
            "faults": node.faults.len(),
            "work_orders": node.work_orders.len(),
            "open_work_orders": node.work_orders.iter().filter(|o| !matches!(o.status, WorkOrderStatus::Verified | WorkOrderStatus::NotVerified)).count(),
        }));
    }
    Ok(json!({"origin": Origin::Synthetic, "environment": twin.sample, "nodes": nodes}))
}

/// §7.3: the node's noise with each amp's return leg attenuated in turn. On the emulator or a plant this is a technician at a switch.
fn localize(twin: &Twin, id: &ServiceGroupId) -> Result<Value, TwinError> {
    let node = twin.node(id)?;
    let graph = twin.graph(node)?;
    let profile = graph.profile();
    let set = twin.fault_set(id, &node.faults);
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples: vec![twin.sample.clone()],
        ambient_sources: Vec::new(),
    };
    let measure = |amp: Option<&ElementId>| -> Option<f64> {
        let plant = match amp {
            Some(id) => diagnosis::switching::with_return_atten(&node.plant, id),
            None => node.plant.clone(),
        };
        let g = plant_graph::PlantGraph::new(&plant, &twin.library, profile).ok()?;
        let link = twin.link(id).ok().flatten();
        let steps = simulate(&Scenario {
            graph: &g,
            environment: &environment,
            faults: &set,
            seed: twin.group_seed(id),
            options: SimOptions {
                detail: Detail::Summary {
                    stride: features::reference::SUMMARY_STRIDE,
                },
                randomize: None,
                measurement_noise_db: 0.0,
                rxmer_scatter_db: 0.0,
            },
            link: link.as_ref(),
        })
        .ok()?;
        // Reads as noise: the lowest SNR across channels, the other way up.
        steps
            .first()?
            .service_group
            .us_snr
            .iter()
            .map(|r| r.us_snr_db)
            .reduce(f64::min)
            .map(|snr| -snr)
    };
    let below = diagnosis::switching::localize_by_switching(&graph, measure);
    Ok(json!({
        "origin": Origin::Synthetic,
        "method": "Attenuates each amp's return leg by 20 dB and watches the node's noise.",
        "noise_is_below": below,
    }))
}

fn migration_plan(
    twin: &Twin,
    a: &MigrationArgs,
) -> Result<design::migrate::MigrationPlan, TwinError> {
    let node = twin.node(&a.service_group)?;
    let from = twin
        .profiles
        .get(&node.plant.profile_ref)
        .ok_or_else(|| TwinError::NoProfile(node.plant.profile_ref.to_string()))?;
    let to = twin
        .profiles
        .get(&a.to_profile)
        .ok_or_else(|| TwinError::NoProfile(a.to_profile.to_string()))?;
    design::migrate::plan_migration(&node.plant, &twin.library, from, to)
        .map_err(|e| TwinError::Refused(e.to_string()))
}

fn design_tool(twin: &mut Twin, a: &DesignArgs) -> Result<Value, TwinError> {
    let map = twin
        .maps
        .get(&a.map_id)
        .ok_or_else(|| TwinError::Refused(format!("no map {}", a.map_id)))?;
    let profile = twin
        .profiles
        .get(&a.profile)
        .ok_or_else(|| TwinError::NoProfile(a.profile.to_string()))?;
    let controls = a.controls.clone().unwrap_or_default();
    if controls.rules_id.is_some() {
        return design_v2_tool(twin, a, controls);
    }
    let design = design::design_region(
        map,
        profile,
        &twin.library,
        &controls,
        &design::DesignParts::default(),
    )
    .map_err(|e| TwinError::Refused(e.to_string()))?;
    if design.nodes.is_empty() {
        return Err(TwinError::Refused(format!(
            "nothing to design: none of this map's {} homes is within {:.0} ft of a street",
            map.homes.len(),
            controls.max_drop_ft
        )));
    }
    // A re-solve replaces the map's nodes, so it can't go over anything someone has recorded on them.
    refuse_if_recorded(twin, &a.map_id, "re-solve this map")?;
    for id in nodes_on(twin, &a.map_id) {
        twin.nodes.remove(&id);
    }
    // Node names start at sg-0 on each design, so this one's are prefixed with its map to stay apart from another's.
    let mut nodes = Vec::new();
    let mut region = design.region.clone();
    for (node, entry) in design.nodes.iter().zip(region.nodes.iter_mut()) {
        let mut plant = node.plant.clone();
        let id = ServiceGroupId::new(format!("{}/{}", a.map_id, plant.service_group));
        plant.service_group = id.clone();
        entry.service_group = id.clone();
        nodes.push(json!({
            "service_group": id,
            "homes_passed": node.homes_passed,
            "amps": node.amps,
            "hardline_ft": node.hardline_ft,
            "max_cascade": node.max_cascade,
            "parts": plant.parts_list(),
            "topology_issues": node.topology_issues,
            "physics_issues": node.physics_issues,
        }));
        twin.add_plant(plant)?;
    }
    twin.design_inputs.insert(
        a.map_id.clone(),
        DesignInput {
            profile: a.profile.clone(),
            controls,
        },
    );
    let fiber = twin.maps.get(&a.map_id).map(|map| fiber_json(twin, map));
    let summary = json!({
        "origin": region.origin,
        "region": region.id,
        "unserved": design.unserved,
        "nodes": nodes,
        "fiber": fiber,
    });
    twin.regions.insert(region.id.clone(), region);
    Ok(summary)
}

/// A fiber plant as GeoJSON: each cable along its path, each element on a structure or at the hub.
pub(crate) fn fiber_geojson(fiber: &schema::fiber::FiberPlant) -> Value {
    let mut features = Vec::new();
    let geo_of = |id: &schema::common::FiberElementId| fiber.element(id).and_then(|e| e.geo);
    let carried: std::collections::BTreeMap<&str, std::collections::BTreeSet<String>> = {
        let mut m: std::collections::BTreeMap<&str, std::collections::BTreeSet<String>> =
            Default::default();
        for link in &fiber.links {
            for hop in &link.hops {
                if let schema::fiber::Hop::Fiber { cable, .. } = hop {
                    m.entry(cable.as_str())
                        .or_default()
                        .insert(link.service_group.to_string());
                }
            }
        }
        m
    };
    for cable in &fiber.cables {
        let (Some(a), Some(b)) = (geo_of(&cable.from), geo_of(&cable.to)) else {
            continue;
        };
        if cable.path.is_empty() && a == b {
            continue; // a jumper inside the hub or on one structure
        }
        let mut coordinates = vec![json!([a.lon_deg, a.lat_deg])];
        coordinates.extend(
            cable
                .path
                .iter()
                .map(|p| json!([p.geo.lon_deg, p.geo.lat_deg])),
        );
        coordinates.push(json!([b.lon_deg, b.lat_deg]));
        features.push(json!({
            "type": "Feature",
            "geometry": {"type": "LineString", "coordinates": coordinates},
            "properties": {
                "id": cable.id, "kind": "FiberCable", "component": cable.component_ref, "len_ft": cable.len_ft,
                "from": cable.from, "to": cable.to, "installation": cable.installation, "support": cable.support,
                "record_status": cable.record_status, "service_groups": carried.get(cable.id.as_str()),
            },
        }));
    }
    for element in &fiber.elements {
        let Some(geo) = element.geo else {
            continue;
        };
        let kind = match &element.kind {
            schema::fiber::FiberElementKind::NodeEnd { .. } => "NodeEnd".to_string(),
            other => format!("{other:?}"),
        };
        features.push(json!({
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": [geo.lon_deg, geo.lat_deg]},
            "properties": {
                "id": element.id, "kind": kind, "component": element.component_ref, "hub": element.hub,
                "structure": element.structure, "record_status": element.record_status,
            },
        }));
    }
    json!({"type": "FeatureCollection", "id": fiber.id, "origin": fiber.origin, "features": features})
}

/// The version 2 designer (task 23): rules given in the controls, and a mode.
fn design_v2_tool(
    twin: &mut Twin,
    a: &DesignArgs,
    controls: schema::region::DesignControls,
) -> Result<Value, TwinError> {
    let map = twin
        .maps
        .get(&a.map_id)
        .cloned()
        .ok_or_else(|| TwinError::Refused(format!("no map {}", a.map_id)))?;
    let profile = twin
        .profiles
        .get(&a.profile)
        .cloned()
        .ok_or_else(|| TwinError::NoProfile(a.profile.to_string()))?;
    let rules_of = |id: &str| {
        twin.design_rules.get(id).cloned().ok_or_else(|| {
            let known: Vec<&String> = twin.design_rules.keys().collect();
            TwinError::Refused(format!("no design rules {id}; the twin has {known:?}"))
        })
    };
    let rules_id = controls.rules_id.clone().unwrap_or_default();
    let rules = rules_of(&rules_id)?;
    let design = match &controls.mode {
        Some(schema::region::DesignMode::LegacyThenUpgrade {
            legacy_rules_id,
            legacy_profile_ref,
        }) => {
            let legacy_rules = rules_of(legacy_rules_id)?;
            let legacy_profile = twin
                .profiles
                .get(legacy_profile_ref)
                .cloned()
                .ok_or_else(|| TwinError::NoProfile(legacy_profile_ref.to_string()))?;
            design::v2::design_upgraded(
                &map,
                &profile,
                &twin.library,
                &rules,
                &legacy_profile,
                &legacy_rules,
                &controls,
            )
        }
        _ => design::v2::design(&map, &profile, &twin.library, &rules, &controls),
    }
    .map_err(|e| TwinError::Refused(e.to_string()))?;
    // A re-solve replaces the map's nodes and fiber plant, so it can't go over anything someone has recorded on them.
    refuse_if_recorded(twin, &a.map_id, "re-solve this map")?;
    for id in nodes_on(twin, &a.map_id) {
        twin.nodes.remove(&id);
    }
    let region_id = format!("region-{}", a.map_id);
    if let Some(old) = twin
        .regions
        .get(&region_id)
        .and_then(|r| r.fiber_ref.clone())
    {
        twin.fiber_plants.remove(&old);
    }
    twin.fiber_faults.remove(&region_id);
    // Node names start at sg-0 on each design, so this one's are prefixed with its map, in the plants, the region and the
    // fiber plant alike.
    let rename = |sg: &ServiceGroupId| ServiceGroupId::new(format!("{}/{}", a.map_id, sg));
    let mut region = design.region.clone();
    for n in &mut region.nodes {
        n.service_group = rename(&n.service_group);
    }
    let mut fiber = design.fiber.clone();
    for link in &mut fiber.links {
        link.service_group = rename(&link.service_group);
    }
    for b in &mut fiber.bindings {
        b.service_group = rename(&b.service_group);
    }
    for e in &mut fiber.elements {
        if let schema::fiber::FiberElementKind::NodeEnd { service_group } = &mut e.kind {
            *service_group = rename(service_group);
        }
    }
    let mut nodes = Vec::new();
    for node in &design.nodes {
        let mut plant = node.plant.clone();
        plant.service_group = rename(&plant.service_group);
        let count =
            |f: fn(&ElementKind) -> bool| plant.elements.iter().filter(|e| f(&e.kind)).count();
        nodes.push(json!({
            "service_group": plant.service_group,
            "homes_passed": node.homes_passed,
            "amps": count(|k| matches!(k, ElementKind::Amp { .. })),
            "taps": count(|k| matches!(k, ElementKind::Tap)),
            "line_equalizers": count(|k| matches!(k, ElementKind::LineEqualizer)),
            "supplies": plant.power_supplies.len(),
            "hardline_ft": plant.edges.iter().filter_map(|e| match e.kind { EdgeKind::Span { len_ft, .. } => Some(len_ft), _ => None }).sum::<f64>(),
            "issues": node.issues,
            "notes": node.notes,
        }));
        twin.add_plant(plant)?;
    }
    twin.add_fiber_plant(fiber.clone())?;
    twin.regions.insert(region.id.clone(), region.clone());
    twin.design_inputs.insert(
        a.map_id.clone(),
        DesignInput {
            profile: a.profile.clone(),
            controls: controls.clone(),
        },
    );
    twin.design_reports.insert(
        a.map_id.clone(),
        crate::state::DesignReport {
            rules_id: rules_id.clone(),
            candidates: design.candidates.clone(),
            over_ceiling: design.over_ceiling.clone(),
            unserved: design.unserved.clone(),
            upgrade: design.upgrade.clone(),
        },
    );
    let over: usize = design.over_ceiling.iter().map(|o| o.homes.len()).sum();
    Ok(json!({
        "origin": region.origin,
        "region": region.id,
        "rules": rules_id,
        "mode": controls.mode,
        "unserved": design.unserved,
        "over_ceiling": {"homes": over, "areas": design.over_ceiling.iter().map(|o| json!({"homes": o.homes.len(), "usd_per_home": o.usd_per_home})).collect::<Vec<_>>()},
        "nodes": nodes,
        "cost": {"total_usd": design.cost.total_usd, "usd_per_home": design.cost.usd_per_home, "basis": if design.cost.all_generic { "Generic" } else { "mixed" }},
        "candidates": design.candidates,
        "fiber": {"id": fiber.id, "cables": fiber.cables.len(), "links": fiber.links.len(), "closures": fiber.elements.iter().filter(|e| matches!(e.kind, schema::fiber::FiberElementKind::SpliceClosure)).count()},
        "upgrade": design.upgrade.as_ref().map(|u| json!({
            "legacy_rules": u.legacy_rules_id, "legacy_profile": u.legacy_profile, "moves": u.moves,
            "changes": u.changes.values().map(Vec::len).sum::<usize>(), "stuck": u.stuck.values().map(Vec::len).sum::<usize>(),
        })),
    }))
}

/// `get_design_cost`: a version 2 design's cost from its records, by the rules it was designed with; never stored.
fn design_cost(twin: &Twin, map_id: &str) -> Result<Value, TwinError> {
    let map = twin
        .maps
        .get(map_id)
        .ok_or_else(|| TwinError::Refused(format!("no map {map_id}")))?;
    let input = twin
        .design_inputs
        .get(map_id)
        .ok_or_else(|| TwinError::Refused(format!("map {map_id} has no design")))?;
    let rules_id = input.controls.rules_id.clone().ok_or_else(|| {
        TwinError::Refused(format!(
            "map {map_id} was designed without rules, by version 1, which prices nothing"
        ))
    })?;
    let rules = twin
        .design_rules
        .get(&rules_id)
        .ok_or_else(|| TwinError::Refused(format!("no design rules {rules_id}")))?;
    let profile = twin
        .profiles
        .get(&input.profile)
        .ok_or_else(|| TwinError::NoProfile(input.profile.to_string()))?;
    let kit = design::v2::kit::Kit::new(rules, &twin.library, profile, None)
        .map_err(|e| TwinError::Refused(e.to_string()))?;
    let structures = design::v2::pipeline::place_structures(&kit, map, &input.controls)
        .map_err(|e| TwinError::Refused(e.to_string()))?;
    let region = twin.regions.get(&format!("region-{map_id}"));
    let fiber = region
        .and_then(|r| r.fiber_ref.as_ref())
        .and_then(|id| twin.fiber_plants.get(id))
        .ok_or_else(|| TwinError::Refused(format!("map {map_id} has no fiber plant")))?;
    let ids = nodes_on(twin, map_id);
    let mut rows = Vec::new();
    for id in &ids {
        let homes = region
            .and_then(|r| r.nodes.iter().find(|n| &n.service_group == id))
            .map_or(0, |n| n.homes_passed);
        rows.push((&twin.node(id)?.plant, homes));
    }
    let report = design::v2::cost::cost_of_records(&kit, &structures, &rows, fiber);
    let extra = twin.design_reports.get(map_id);
    let sourced: Vec<String> = report
        .lines
        .iter()
        .filter(|l| l.basis.as_str() != "Generic")
        .map(|l| format!("{} ({})", l.item, l.basis))
        .collect();
    let basis = if sourced.is_empty() {
        "Every price is Generic: no public price list for coax plant was found".to_string()
    } else {
        format!(
            "Every price is Generic but {}: no public price list for coax plant was found",
            sourced.join(", ")
        )
    };
    Ok(json!({
        "map_id": map_id,
        "rules": rules_id,
        "basis": basis,
        "total_usd": report.total_usd,
        "homes_passed": report.homes_passed,
        "usd_per_home": report.usd_per_home,
        "nodes": report.nodes,
        "lines": report.lines,
        "unpriced": report.unpriced,
        "candidates": extra.map(|r| &r.candidates),
        "over_ceiling": extra.map(|r| &r.over_ceiling),
        "ceiling_usd_per_home": rules.costs.ceiling_usd_per_home.as_ref().map(|c| c.value),
    }))
}

/// `list_design_rules`: each rules file the twin holds, what it covers and how much of it is sourced.
fn list_design_rules(twin: &Twin) -> Value {
    let list: Vec<Value> = twin
        .design_rules
        .values()
        .map(|r| {
            let bases = r.bases();
            let sourced = bases.iter().filter(|(_, b)| matches!(b, schema::design_rules::Basis::Sourced { .. })).count();
            json!({
                "id": r.id,
                "schema_version": r.schema_version,
                "profiles": r.profiles.iter().flatten().map(|p| &p.profile_ref).collect::<Vec<_>>(),
                "tap_port": r.tap_port.iter().map(|t| json!({"profile": t.profile_ref, "at_hi_dbmv": t.at_hi_dbmv.value, "at_lo_dbmv": t.at_lo_dbmv.as_ref().map(|r| r.value), "tolerance_db": t.tolerance_db.value})).collect::<Vec<_>>(),
                "max_homes_per_node": r.node.max_homes.value,
                "max_cascade": r.node.max_cascade.value,
                "ceiling_usd_per_home": r.costs.ceiling_usd_per_home.as_ref().map(|c| c.value),
                "upgrade_moves": r.upgrade.as_ref().map(|u| u.moves.iter().map(|m| m.value).collect::<Vec<_>>()),
                "values": bases.len(),
                "sourced": sourced,
                "generic": bases.len() - sourced,
                "complete": r.for_design().is_ok(),
            })
        })
        .collect();
    json!({"rules": list})
}

/// The map's nodes, by the prefix `design_tool` gives them.
fn nodes_on(twin: &Twin, map_id: &str) -> Vec<ServiceGroupId> {
    let prefix = format!("{map_id}/");
    twin.nodes
        .keys()
        .filter(|id| id.to_string().starts_with(&prefix))
        .cloned()
        .collect()
}

/// A re-solve or a discard throws plant away, so it stops at anything that records more than the design. A `Proposed` order doesn't: every
/// diagnosis can leave one, nobody has acted on it, and it says nothing about a plant that's about to change.
fn refuse_if_recorded(twin: &Twin, map_id: &str, doing: &str) -> Result<(), TwinError> {
    if let Some((id, why)) = recorded_on(twin, map_id)? {
        return Err(TwinError::Refused(format!("can't {doing}: {id} {why}.")));
    }
    Ok(())
}

/// The first node on a map holding something a re-solve would destroy, and what: a fault, an order past `Proposed`,
/// a built element or a migration. `get_map` reports it, so a client can say so before anyone tries.
fn recorded_on(
    twin: &Twin,
    map_id: &str,
) -> Result<Option<(ServiceGroupId, &'static str)>, TwinError> {
    let designed_on = twin.design_inputs.get(map_id).map(|i| &i.profile);
    // The fiber plant is records too (task 23): a fault on it, or any of it built, locks the map as a node's would.
    let region_id = format!("region-{map_id}");
    if twin
        .fiber_faults
        .get(&region_id)
        .is_some_and(|f| !f.is_empty())
        && let Some(id) = nodes_on(twin, map_id).into_iter().next()
    {
        return Ok(Some((
            id,
            "is behind a fiber fault injected on the region. Clear it first",
        )));
    }
    let fiber_built = twin
        .regions
        .get(&region_id)
        .and_then(|r| r.fiber_ref.as_ref())
        .and_then(|id| twin.fiber_plants.get(id))
        .is_some_and(|f| {
            f.elements
                .iter()
                .any(|e| e.record_status != RecordStatus::Designed)
                || f.cables
                    .iter()
                    .any(|c| c.record_status != RecordStatus::Designed)
        });
    if fiber_built && let Some(id) = nodes_on(twin, map_id).into_iter().next() {
        return Ok(Some((id, "is on a fiber plant with built records")));
    }
    for id in nodes_on(twin, map_id) {
        let node = twin.node(&id)?;
        let why = if !node.faults.is_empty() {
            Some("has a fault injected. Clear it first")
        } else if node
            .work_orders
            .iter()
            .any(|o| o.status != WorkOrderStatus::Proposed)
        {
            Some("has a work order that's been acted on")
        } else if node
            .plant
            .elements
            .iter()
            .any(|e| e.record_status != RecordStatus::Designed)
        {
            Some("has built elements")
        } else if designed_on.is_some_and(|p| *p != node.plant.profile_ref) {
            Some("has been migrated")
        } else {
            None
        };
        if let Some(why) = why {
            return Ok(Some((id, why)));
        }
    }
    Ok(None)
}

fn import_osm(twin: &mut Twin, a: ImportArgs) -> Result<Value, TwinError> {
    let Some(text) = a.overpass_json else {
        return twin
            .maps
            .get(&a.map_id)
            .map(|map| map_summary(twin, map))
            .ok_or_else(|| TwinError::Refused(format!("no map {} and no extract", a.map_id)));
    };
    let (mut map, report) = design::osm::read_overpass(
        &a.map_id,
        &text,
        a.installation.unwrap_or(Installation::Aerial),
    )
    .map_err(|e| TwinError::Refused(format!("couldn't read that extract: {e}")))?;
    if let Some(headend) = a.headend {
        map.headend = headend;
    }
    twin.osm_reports.insert(a.map_id.clone(), report);
    twin.maps.insert(a.map_id.clone(), map);
    Ok(map_summary(twin, &twin.maps[&a.map_id]))
}

/// Below this, a place's buildings mostly aren't mapped, and a design would pass off a few houses as the whole place.
const SPARSE_HOMES_PER_STREET_MILE: f64 = 3.0;

fn map_label(map: &schema::region::StreetMap) -> String {
    match &map.source {
        MapSource::Generated { .. } => "Generated streets and homes, not a real place.".to_string(),
        MapSource::OpenStreetMap { attribution } => {
            format!("Real streets and buildings ({attribution}). The plant is invented.")
        }
    }
}

fn map_summary(twin: &Twin, map: &schema::region::StreetMap) -> Value {
    let street_miles: f64 = map
        .streets
        .iter()
        .flat_map(|s| s.points.windows(2))
        .map(|p| design::geo::dist_ft(p[0], p[1]))
        .sum::<f64>()
        / 5280.0;
    let sparse = matches!(map.source, MapSource::OpenStreetMap { .. })
        && (map.homes.len() as f64) < street_miles * SPARSE_HOMES_PER_STREET_MILE;
    json!({
        "map_id": map.id,
        "source": map.source,
        "label": map_label(map),
        "real_geography": matches!(map.source, MapSource::OpenStreetMap { .. }),
        "homes": map.homes.len(),
        "streets": map.streets.len(),
        "street_miles": street_miles,
        "sparse": sparse,
        "report": twin.osm_reports.get(&map.id),
        "nodes": nodes_on(twin, &map.id).len(),
    })
}

fn list_maps(twin: &Twin) -> Value {
    json!({"maps": twin.maps.values().map(|map| map_summary(twin, map)).collect::<Vec<_>>()})
}

/// Where a node's root sits.
pub(crate) fn site_of(plant: &schema::plant::Plant) -> Option<LonLat> {
    plant
        .elements
        .iter()
        .find(|e| matches!(e.kind, ElementKind::Node { .. } | ElementKind::Rpd { .. }))
        .and_then(|e| e.geo)
}

fn fiber_json(twin: &Twin, map: &schema::region::StreetMap) -> Value {
    let sites: Vec<(ServiceGroupId, LonLat)> = nodes_on(twin, &map.id)
        .into_iter()
        .filter_map(|id| {
            let site = twin.nodes.get(&id).and_then(|n| site_of(&n.plant))?;
            Some((id, site))
        })
        .collect();
    let route = design::fiber::fiber_route(map, &sites);
    let features: Vec<Value> = route
        .lines
        .iter()
        .map(|(a, b)| {
            json!({
                "type": "Feature",
                "geometry": {"type": "LineString", "coordinates": [[a.lon_deg, a.lat_deg], [b.lon_deg, b.lat_deg]]},
                "properties": {},
            })
        })
        .collect();
    json!({
        "route_ft": route.route_ft,
        "unreached": route.unreached,
        "lines": {"type": "FeatureCollection", "features": features},
    })
}

fn get_map(twin: &Twin, map_id: &str) -> Result<Value, TwinError> {
    let map = twin
        .maps
        .get(map_id)
        .ok_or_else(|| TwinError::Refused(format!("no map {map_id}")))?;
    let streets: Vec<Value> = map
        .streets
        .iter()
        .map(|s| {
            let coordinates: Vec<[f64; 2]> =
                s.points.iter().map(|p| [p.lon_deg, p.lat_deg]).collect();
            json!({
                "type": "Feature",
                "geometry": {"type": "LineString", "coordinates": coordinates},
                "properties": {"id": s.id, "crossing": s.id.starts_with("crossing-")},
            })
        })
        .collect();
    let homes: Vec<Value> = map
        .homes
        .iter()
        .map(|h| {
            json!({
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [h.geo.lon_deg, h.geo.lat_deg]},
                "properties": {"id": h.id},
            })
        })
        .collect();

    let input = twin.design_inputs.get(map_id);
    let ids = nodes_on(twin, map_id);
    let sites: Vec<Option<LonLat>> = ids
        .iter()
        .map(|id| twin.nodes.get(id).and_then(|n| site_of(&n.plant)))
        .collect();
    // A node's site is the street vertex nearest its pin, so a pin claims the nearest site not yet claimed.
    let mut pin_of: Vec<Option<usize>> = vec![None; ids.len()];
    for (p, pin) in input
        .map(|i| i.controls.pinned_nodes.as_slice())
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let nearest = sites
            .iter()
            .enumerate()
            .filter(|(n, _)| pin_of[*n].is_none())
            .filter_map(|(n, site)| site.map(|s| (design::geo::dist_ft(s, pin.geo), n)))
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some((_, n)) = nearest {
            pin_of[n] = Some(p);
        }
    }
    let homes_passed = |id: &ServiceGroupId| {
        twin.regions
            .get(&format!("region-{map_id}"))
            .and_then(|r| r.nodes.iter().find(|n| n.service_group == *id))
            .map(|n| n.homes_passed)
    };
    let mut nodes = Vec::new();
    let mut spans = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let node = twin.node(id)?;
        let plant = &node.plant;
        let at = |e: &ElementId| plant.element(e).and_then(|x| x.geo);
        let mut hardline_ft = 0.0;
        for edge in &plant.edges {
            if let EdgeKind::Span { len_ft, .. } = edge.kind {
                hardline_ft += len_ft;
                if let (Some(a), Some(b)) = (at(&edge.from.element_id), at(&edge.to.element_id)) {
                    // Along its path where it has one (task 23), straight where it doesn't.
                    let mut coordinates = vec![json!([a.lon_deg, a.lat_deg])];
                    coordinates.extend(
                        edge.kind
                            .path()
                            .iter()
                            .map(|p| json!([p.geo.lon_deg, p.geo.lat_deg])),
                    );
                    coordinates.push(json!([b.lon_deg, b.lat_deg]));
                    spans.push(json!({
                        "type": "Feature",
                        "geometry": {"type": "LineString", "coordinates": coordinates},
                        "properties": {"service_group": id, "node": i},
                    }));
                }
            }
        }
        let amps = plant
            .elements
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Amp { .. }))
            .count();
        nodes.push(json!({
            "service_group": id,
            "site": sites[i],
            "pin": pin_of[i],
            "homes_passed": homes_passed(id),
            "amps": amps,
            "hardline_ft": hardline_ft,
        }));
    }
    let mut out = map_summary(twin, map);
    if let Some(fields) = out.as_object_mut() {
        fields.insert("headend".into(), json!(map.headend));
        fields.insert(
            "street_lines".into(),
            json!({"type": "FeatureCollection", "features": streets}),
        );
        fields.insert(
            "home_points".into(),
            json!({"type": "FeatureCollection", "features": homes}),
        );
        fields.insert("design_input".into(), json!(input));
        fields.insert("node_sites".into(), json!(nodes));
        fields.insert(
            "span_lines".into(),
            json!({"type": "FeatureCollection", "features": spans}),
        );
        fields.insert("fiber".into(), fiber_json(twin, map));
        // A version 2 design's stored fiber plant, drawn: sheaths along their paths, closures and node ends.
        if let Some(fiber) = twin
            .regions
            .get(&format!("region-{map_id}"))
            .and_then(|r| r.fiber_ref.as_ref())
            .and_then(|id| twin.fiber_plants.get(id))
        {
            fields.insert("fiber_plant".into(), fiber_geojson(fiber));
        }
        if let Some(report) = twin.design_reports.get(map_id) {
            fields.insert(
                "over_ceiling_homes".into(),
                json!(
                    report
                        .over_ceiling
                        .iter()
                        .map(|o| o.homes.len())
                        .sum::<usize>()
                ),
            );
        }
        let locked =
            recorded_on(twin, map_id)?.map(|(id, why)| json!({"service_group": id, "reason": why}));
        fields.insert("locked".into(), json!(locked));
    }
    Ok(out)
}

/// A fiber fault must name a region with a fiber plant, and a cable, element or splice in it.
fn check_fiber_fault(
    twin: &Twin,
    region: &str,
    fault: &schema::fiber_fault::FiberFault,
) -> Result<(), TwinError> {
    use schema::fiber_fault::{FiberFaultKind, FiberLocation};
    let found = twin
        .regions
        .get(region)
        .ok_or_else(|| TwinError::InvalidFault(format!("no region {region}")))?;
    let fiber = found
        .fiber_ref
        .as_ref()
        .and_then(|f| twin.fiber_plants.get(f))
        .ok_or_else(|| TwinError::InvalidFault(format!("region {region} has no fiber plant")))?;
    let cable = |id: &schema::common::FiberCableId, pos_ft: f64| {
        let c = fiber
            .cable(id)
            .ok_or_else(|| TwinError::InvalidFault(format!("no cable {id} in {}", fiber.id)))?;
        if pos_ft < 0.0 || pos_ft > c.len_ft {
            return Err(TwinError::InvalidFault(format!(
                "{pos_ft} ft is off cable {id}, which is {} ft",
                c.len_ft
            )));
        }
        Ok(())
    };
    let element = |id: &schema::common::FiberElementId| {
        fiber
            .element(id)
            .map(|_| ())
            .ok_or_else(|| TwinError::InvalidFault(format!("no element {id} in {}", fiber.id)))
    };
    match &fault.kind {
        FiberFaultKind::FiberCut {
            cable: c, pos_ft, ..
        } => cable(c, *pos_ft),
        FiberFaultKind::EventLoss { location, .. } => match location {
            FiberLocation::Cable { cable: c, pos_ft } => cable(c, *pos_ft),
            FiberLocation::Element { element: e } => element(e),
            FiberLocation::Splice {
                element: e,
                strands,
            } => {
                element(e)?;
                for strand in strands {
                    cable(&strand.cable, 0.0)?;
                }
                Ok(())
            }
        },
        FiberFaultKind::TransmitterDrift { element: e, .. } => element(e),
    }
}

/// `get_optical_budget`.
fn optical_budget(twin: &Twin, id: &ServiceGroupId) -> Result<Value, TwinError> {
    let node = twin.node(id)?;
    let link = twin.link(id)?;
    let profile = twin
        .profiles
        .get(&node.plant.profile_ref)
        .ok_or_else(|| TwinError::NoProfile(node.plant.profile_ref.to_string()))?;
    let issues: Vec<String> = link
        .as_ref()
        .map(|l| {
            sim::optics::link_issues(l, &twin.library, &profile.channel_plan)
                .iter()
                .map(|i| format!("{i:?}"))
                .collect()
        })
        .unwrap_or_default();
    let region = twin.region_of(id);
    Ok(json!({
        "origin": node.plant.origin,
        "service_group": id,
        "region": region.map(|r| r.id.clone()),
        "fiber_ref": region.and_then(|r| r.fiber_ref.clone()),
        "link": link,
        "issues": issues,
    }))
}

fn check_fault(twin: &Twin, id: &ServiceGroupId, fault: &Fault) -> Result<(), TwinError> {
    let node = twin.node(id)?;
    let graph = twin.graph(node)?;
    let set = twin.fault_set(id, std::slice::from_ref(fault));
    graph
        .check_faults(&set)
        .map_err(|e| TwinError::InvalidFault(format!("{e:?}")))
}

fn list_groups(twin: &Twin) -> Value {
    let groups: Vec<Value> = twin
        .nodes
        .iter()
        .map(|(id, node)| {
            let is = |f: &dyn Fn(&schema::plant::ElementKind) -> bool| {
                node.plant.elements.iter().filter(|e| f(&e.kind)).count()
            };
            json!({
                "service_group": id,
                "profile": node.plant.profile_ref,
                "origin": node.plant.origin,
                "modems": is(&|k| matches!(k, schema::plant::ElementKind::Modem)),
                "amps": is(&|k| matches!(k, schema::plant::ElementKind::Amp { .. })),
                "faults": node.faults.len(),
            })
        })
        .collect();
    json!({"service_groups": groups})
}

fn plant_geojson(twin: &Twin, id: &ServiceGroupId) -> Result<Value, TwinError> {
    let node = twin.node(id)?;
    let plant = &node.plant;
    let mut features = Vec::new();
    for element in &plant.elements {
        if let Some(geo) = element.geo {
            features.push(json!({
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [geo.lon_deg, geo.lat_deg]},
                "properties": {
                    "id": element.id,
                    "kind": kind_name(&element.kind),
                    "component": element.component_ref,
                    "record_status": element.record_status,
                    "ports": element.ports,
                    "fitted": element.fitted,
                    "structure": element.structure,
                },
            }));
        }
    }
    for supply in &plant.power_supplies {
        if let Some(geo) = supply.geo {
            features.push(json!({
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [geo.lon_deg, geo.lat_deg]},
                "properties": {
                    "id": supply.id,
                    "kind": "PowerSupply",
                    "component": supply.component_ref,
                    "batteries": supply.batteries,
                    "structure": supply.structure,
                    "record_status": supply.record_status,
                },
            }));
        }
    }
    // Version 2: poles, pedestals, vaults and cabinets. No RF, so nothing else reads them.
    for structure in &plant.structures {
        if let Some(geo) = structure.geo {
            features.push(json!({
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [geo.lon_deg, geo.lat_deg]},
                "properties": {
                    "id": structure.id,
                    "kind": "Structure",
                    "structure_kind": structure.kind,
                    "component": structure.component_ref,
                    "record_status": structure.record_status,
                },
            }));
        }
    }
    for edge in &plant.edges {
        let at = |id: &ElementId| plant.element(id).and_then(|e| e.geo);
        if let (Some(a), Some(b)) = (at(&edge.from.element_id), at(&edge.to.element_id)) {
            let (kind, len_ft) = match edge.kind {
                EdgeKind::Span { len_ft, .. } => ("Span", len_ft),
                EdgeKind::Drop { len_ft, .. } => ("Drop", len_ft),
                EdgeKind::House { len_ft, .. } => ("House", len_ft),
                EdgeKind::Direct => ("Direct", 0.0),
            };
            let mut coordinates = vec![json!([a.lon_deg, a.lat_deg])];
            coordinates.extend(
                edge.kind
                    .path()
                    .iter()
                    .map(|p| json!([p.geo.lon_deg, p.geo.lat_deg])),
            );
            coordinates.push(json!([b.lon_deg, b.lat_deg]));
            let (support, installation) = match &edge.kind {
                EdgeKind::Span {
                    support,
                    installation,
                    ..
                } => (support.clone(), Some(*installation)),
                EdgeKind::Drop { installation, .. } => (None, *installation),
                _ => (None, None),
            };
            features.push(json!({
                "type": "Feature",
                "geometry": {"type": "LineString", "coordinates": coordinates},
                "properties": {
                    "id": edge.id,
                    "kind": kind,
                    "len_ft": len_ft,
                    "from": edge.from.element_id,
                    "to": edge.to.element_id,
                    "cable": edge.kind.cable().map(|(cable_ref, _)| cable_ref),
                    "fittings": edge.kind.fittings(),
                    "support": support,
                    "installation": installation,
                    "record_status": edge.record_status,
                },
            }));
        }
    }
    Ok(json!({
        "type": "FeatureCollection",
        "origin": plant.origin,
        "service_group": plant.service_group,
        "profile": plant.profile_ref,
        "features": features,
    }))
}

fn fiber_plant_json(twin: &Twin, id: &str) -> Result<Value, TwinError> {
    let fiber = twin.fiber_plants.get(id).ok_or_else(|| {
        let known: Vec<&String> = twin.fiber_plants.keys().collect();
        TwinError::Refused(format!("no fiber plant {id}; the twin has {known:?}"))
    })?;
    let graph = plant_graph::FiberGraph::new(fiber, &twin.library)
        .map_err(|e| TwinError::InvalidPlant(format!("{e:?}")))?;
    let mut regions = Vec::new();
    for region in twin.regions.values() {
        if region.fiber_ref.as_deref() != Some(id) {
            continue;
        }
        let plants: Vec<&schema::plant::Plant> = region
            .nodes
            .iter()
            .filter_map(|n| twin.nodes.get(&n.service_group).map(|node| &node.plant))
            .collect();
        let issues: Vec<String> = plant_graph::check_region(region, &graph, &plants)
            .iter()
            .map(ToString::to_string)
            .collect();
        regions.push(json!({"region": region.id, "issues": issues}));
    }
    Ok(json!({
        "origin": fiber.origin,
        "fiber_plant": fiber,
        "parts": fiber.parts_list(),
        "splices": fiber.splices(),
        "regions": regions,
    }))
}

fn kind_name(kind: &schema::plant::ElementKind) -> &'static str {
    use schema::plant::ElementKind::*;
    match kind {
        Node { .. } => "Node",
        Rpd { .. } => "Rpd",
        Amp { .. } => "Amp",
        Tap => "Tap",
        Splitter => "Splitter",
        DirectionalCoupler => "DirectionalCoupler",
        PowerInserter => "PowerInserter",
        Terminator => "Terminator",
        Modem => "Modem",
        LineEqualizer => "LineEqualizer",
        GroundBlock => "GroundBlock",
        DropAmp => "DropAmp",
        Filter => "Filter",
        Cpe => "Cpe",
    }
}

fn sample_at(twin: &Twin, temp_c: Option<f64>) -> EnvironmentSample {
    let mut sample = twin.sample.clone();
    if let Some(temp_c) = temp_c {
        sample.temp_c = temp_c;
        sample.ground_temp_c = temp_c;
    }
    sample
}

fn finite(value: f64) -> Value {
    if value.is_finite() {
        json!(value)
    } else {
        Value::Null
    }
}

fn budget(twin: &Twin, a: &BudgetArgs) -> Result<Value, TwinError> {
    let node = twin.node(&a.service_group)?;
    let graph = twin.graph(node)?;
    let sample = sample_at(twin, a.temp_c);
    let overlay = Overlay::build(
        &graph,
        &twin.fault_set(&a.service_group, &node.faults),
        &sample,
    )?;
    let budget = level_budget(&graph, &sample, &overlay)?;
    let (ds_lo_mhz, ds_hi_mhz) = sim::walk::ds_band(graph.profile());
    // §13: what the home costs, from its tap port to the modem, at the band's two edges.
    let premises = |id: &schema::common::ElementId| -> Option<Value> {
        let home = graph.premises_of(id).ok()??;
        let tap_port = schema::fault::Location::Port {
            element_id: home.drop.from.element_id.clone(),
            port: home.drop.from.port.clone(),
        };
        let modem = graph.element(id).ok()?;
        let at = schema::fault::Location::Port {
            element_id: id.clone(),
            port: modem.ports.first()?.id.clone(),
        };
        let loss = |f: f64| {
            sim::transfer::transfer(&graph, &sample, &tap_port, &at, f)
                .ok()
                .map(|t| t.loss_db)
        };
        Some(json!({
            "tap": home.tap.id,
            "tap_port": home.drop.from.port,
            "drop": home.drop.id,
            "ground_block": home.ground_block.map(|g| g.id.clone()),
            "devices": home.devices.iter().map(|d| d.id.clone()).collect::<Vec<_>>(),
            "loss_from_tap_port_db": {
                "lo_mhz": ds_lo_mhz,
                "at_lo": loss(ds_lo_mhz).map_or(Value::Null, finite),
                "hi_mhz": ds_hi_mhz,
                "at_hi": loss(ds_hi_mhz).map_or(Value::Null, finite),
            },
        }))
    };
    let modems: Vec<Value> = budget
        .modems
        .iter()
        .map(|(id, m)| {
            json!({
                "modem": id,
                "premises": premises(id),
                "level_min_dbmv": finite(m.level_min_dbmv),
                "level_max_dbmv": finite(m.level_max_dbmv),
                "spread_db": finite(m.spread_db),
                "total_input_dbmv": finite(m.total_input_dbmv),
                "tx_min_dbmv_per_1p6mhz": finite(m.tx_min_dbmv_per_1p6mhz),
                "tx_max_dbmv_per_1p6mhz": finite(m.tx_max_dbmv_per_1p6mhz),
                "tx_limit_dbmv_per_1p6mhz": finite(m.tx_limit_dbmv_per_1p6mhz),
            })
        })
        .collect();
    Ok(json!({"origin": Origin::Synthetic, "temp_c": sample.temp_c, "modems": modems}))
}

fn design_issues(twin: &Twin, a: &BudgetArgs) -> Result<Value, TwinError> {
    let node = twin.node(&a.service_group)?;
    let graph = twin.graph(node)?;
    let samples: Vec<EnvironmentSample> = match a.temp_c {
        Some(_) => vec![sample_at(twin, a.temp_c)],
        None => [-20.0, 20.0, 45.0]
            .iter()
            .map(|t| sample_at(twin, Some(*t)))
            .collect(),
    };
    let topology: Vec<String> = graph
        .check_design()
        .iter()
        .map(|i| format!("{i:?}"))
        .collect();
    let physics: Vec<String> = check_physics(&graph, &samples, &Overlay::default())?
        .iter()
        .map(|i| format!("{i:?}"))
        .collect();
    Ok(json!({"origin": node.plant.origin, "topology": topology, "physics": physics}))
}

fn modem_telemetry(twin: &Twin, a: &ModemArgs) -> Result<Value, TwinError> {
    let node = twin.node(&a.service_group)?;
    let graph = twin.graph(node)?;
    let faults = a.assume_faults.as_ref().unwrap_or(&node.faults);
    let set = twin.fault_set(&a.service_group, faults);
    let environment = Environment {
        schema_version: schema::environment::SCHEMA_VERSION.to_string(),
        samples: vec![twin.sample.clone()],
        ambient_sources: Vec::new(),
    };
    let link = twin.link(&a.service_group)?;
    let steps = simulate(&Scenario {
        graph: &graph,
        environment: &environment,
        faults: &set,
        seed: twin.group_seed(&a.service_group),
        options: SimOptions {
            detail: if a.full {
                Detail::Full
            } else {
                Detail::Summary {
                    stride: features::reference::SUMMARY_STRIDE,
                }
            },
            randomize: Some(twin.group_seed(&a.service_group)),
            measurement_noise_db: 0.25,
            rxmer_scatter_db: 0.0,
        },
        link: link.as_ref(),
    })?;
    let step = steps
        .first()
        .ok_or_else(|| TwinError::Refused("no samples".into()))?;
    let modem = step
        .modems
        .iter()
        .find(|m| m.element_id == a.modem)
        .ok_or_else(|| TwinError::Refused(format!("no modem {}", a.modem)))?;
    serde_json::to_value(modem).map_err(TwinError::from)
}

fn status_map(
    twin: &Twin,
    id: &ServiceGroupId,
    features: &features::StepFeatures,
) -> Result<Value, TwinError> {
    let who = twin.who(id, features)?;
    let modems: Vec<Value> = features
        .modems
        .iter()
        .map(|m| {
            json!({
                "modem": m.element_id,
                "affected": who.affected.contains(&m.element_id),
                "operational": m.operational,
                "is_d31": m.is_d31,
                "level_drift_db": m.level_drift_db,
                "us_tx_drift_db": m.us_tx_drift_db,
                "mer_margin_mean_db": m.mer_margin_mean_db,
                "mer_margin_p2_db": m.mer_margin_p2_db,
                "preeq_nmter_db": m.preeq_nmter_db,
                "preeq_echo_ft": m.preeq_echo_ft,
                "uncorrectable_rate": m.uncorrectable_rate,
                "t3_delta": m.t3_delta,
                "t4_delta": m.t4_delta,
            })
        })
        .collect();
    Ok(json!({
        "origin": Origin::Synthetic,
        "environment": twin.sample,
        "who": who,
        "modems": modems,
        "service_group": features.service_group,
        "actives": features.actives,
    }))
}

fn hypothesis_json(answer: &diagnosis::Diagnosis) -> Vec<Value> {
    answer
        .hypotheses
        .iter()
        .map(|h| {
            json!({
                "class": h.class,
                "components": h.components,
                "faults": h.faults,
                "posterior": h.posterior,
                "chi2_per_dof": h.chi2 / h.dof as f64,
                "affected_modems": h.affected_modems,
                "predicted": status_of_predicted(&h.predicted),
            })
        })
        .collect()
}

/// Below even odds, with no modem reading worse than this node does when healthy, there's nothing to send a truck for.
const QUIET_ORDER_CONFIDENCE: f64 = 0.5;

/// Whether a diagnosis may raise a work order. Someone reading worse than normal is enough. Otherwise it takes a calibrated confidence: the
/// floor's posterior isn't one, and it names faults on healthy nodes at 0.99.
fn order_warranted(who: &diagnosis::who::Who, confidence: Option<f64>) -> bool {
    !who.affected.is_empty()
        || who.out_of_service > 0
        || who.upstream_degraded
        || who.rpd_abnormal
        || confidence.is_some_and(|c| c >= QUIET_ORDER_CONFIDENCE)
}

/// Why a diagnosis raises no work order, or `None` when it raises one. An order is a truck roll. Randomization alone pushes a healthy
/// node's top answer to a named fault, so one nobody can feel has to earn it ("quiet"). And when healthy is the most likely answer there
/// is no fault to send a truck to, whether nobody reads worse ("healthy") or modems do and nothing the twin tried explains them
/// ("unexplained").
fn order_withheld(
    who: &diagnosis::who::Who,
    healthy_leads: bool,
    confidence: Option<f64>,
) -> Option<&'static str> {
    match (healthy_leads, who.affected.is_empty()) {
        (true, false) => Some("unexplained"),
        (true, true) => Some("healthy"),
        (false, _) if order_warranted(who, confidence) => None,
        (false, _) => Some("quiet"),
    }
}

fn run_diagnosis(twin: &mut Twin, id: &ServiceGroupId) -> Result<Value, TwinError> {
    let floor = twin.diagnose(id)?;
    let with_model = twin.diagnose_with_model(id)?;
    let model_withheld = twin.model_withheld(id)?;
    let t = twin.sample.t;
    let observed = twin.features(id)?;
    let who = twin.who(id, &observed)?;
    let affected = who.affected.len() as u32;
    // The work order comes from the model's verified answer when there is one, since the sim has checked it. The no-ML answer stays
    // beside it, because every recommendation is a truck roll (`docs/DESIGN.md`, "Risks").
    let fit_ranked = with_model.as_ref().map_or(&floor, |(_, answer)| answer);
    let confidence = twin
        .model_calibrated(id)?
        .then_some(())
        .and(with_model.as_ref())
        .and_then(|(_, answer)| answer.hypotheses.first())
        .map(|top| top.posterior);
    // Calibration replaces the fit's best answer's posterior with a measured confidence, which can drop it below another answer. The
    // answer led with, and ordered for, is the most likely one after that, so the reply and the order agree. A stable sort keeps the
    // fit's order between equals.
    let mut chosen = fit_ranked.clone();
    chosen
        .hypotheses
        .sort_by(|a, b| b.posterior.total_cmp(&a.posterior));
    let still_fit_top = match (chosen.hypotheses.first(), fit_ranked.hypotheses.first()) {
        (Some(a), Some(b)) => a.class == b.class && a.components == b.components,
        _ => false,
    };
    // Only the fit's best answer has a calibrated confidence. Once another answer leads, its posterior is a rescaled raw one, and can't
    // earn a quiet node an order.
    let confidence = confidence.filter(|_| still_fit_top);
    let healthy_leads = chosen
        .hypotheses
        .first()
        .is_some_and(|h| h.faults.is_empty());
    let withheld = order_withheld(&who, healthy_leads, confidence);
    let warranted = withheld.is_none();
    let withheld_reason = json!(withheld);
    let chosen = &chosen;
    let orders = if warranted {
        let node = twin.node(id)?;
        let graph = twin.graph(node)?;
        chosen
            .hypotheses
            .first()
            .map(|top| diagnosis::work_order(&graph, top, affected, &twin.policy, t))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let node = twin.node_mut(id)?;
    for order in &orders {
        if !node.work_orders.iter().any(|o| o.id == order.id) {
            node.work_orders.push(order.clone());
            node.evidence.insert(
                order.id,
                crate::lifecycle::Evidence {
                    affected_at_issue: who.affected.clone(),
                    upstream_degraded_at_issue: floor.who.upstream_degraded,
                    rpd_abnormal_at_issue: floor.who.rpd_abnormal,
                    verdict: None,
                },
            );
        }
    }
    let (proposals, model_answer) = match &with_model {
        Some((proposals, answer)) => (
            json!(proposals.iter().take(5).collect::<Vec<_>>()),
            json!({"hypotheses": hypothesis_json(chosen), "simulations": answer.simulations}),
        ),
        None => (Value::Null, Value::Null),
    };
    Ok(json!({
        "origin": Origin::Synthetic,
        "who": who,
        "hypotheses": hypothesis_json(chosen),
        "no_ml": {"hypotheses": hypothesis_json(&floor), "simulations": floor.simulations},
        "model": {"proposals": proposals, "verified": model_answer, "withheld_reason": model_withheld},
        "work_orders": orders,
        "order_withheld": !warranted,
        "order_withheld_reason": withheld_reason,
        "simulations": chosen.simulations,
    }))
}

/// Simulated telemetry under a hypothesis, for setting beside what was seen. The explanation.
fn status_of_predicted(predicted: &features::StepFeatures) -> Value {
    let modems: Vec<Value> = predicted
        .modems
        .iter()
        .map(|m| {
            json!({
                "modem": m.element_id,
                "operational": m.operational,
                "level_drift_db": m.level_drift_db,
                "us_tx_drift_db": m.us_tx_drift_db,
                "mer_margin_mean_db": m.mer_margin_mean_db,
                "preeq_nmter_db": m.preeq_nmter_db,
            })
        })
        .collect();
    json!({"modems": modems})
}

#[cfg(test)]
mod tests {
    use super::order_withheld;
    use diagnosis::who::Who;
    use schema::common::ElementId;

    fn who(affected: usize, upstream_degraded: bool) -> Who {
        Who {
            affected: (0..affected)
                .map(|i| ElementId::new(format!("modem-{i}")))
                .collect(),
            out_of_service: 0,
            upstream_degraded,
            actives: Vec::new(),
            rpd_abnormal: false,
            scope: None,
        }
    }

    #[test]
    fn no_order_when_healthy_leads() {
        // Modems read worse, but healthy is the most likely answer: nothing to send a truck to.
        assert_eq!(
            order_withheld(&who(8, false), true, Some(0.9)),
            Some("unexplained")
        );
        assert_eq!(
            order_withheld(&who(0, false), true, Some(0.9)),
            Some("healthy")
        );
    }

    #[test]
    fn a_named_fault_needs_someone_to_feel_it_or_even_odds() {
        assert_eq!(order_withheld(&who(3, false), false, Some(0.07)), None);
        assert_eq!(order_withheld(&who(0, true), false, None), None);
        assert_eq!(order_withheld(&who(0, false), false, Some(0.6)), None);
        assert_eq!(
            order_withheld(&who(0, false), false, Some(0.2)),
            Some("quiet")
        );
        assert_eq!(order_withheld(&who(0, false), false, None), Some("quiet"));
    }
}

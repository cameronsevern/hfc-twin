//! The world: its inputs, its state, and the one-minute step.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use design::route::StreetGraph;
use diagnosis::fiber::LinkReading;
use diagnosis::outage::{LostWay, OutageVerdict, read_outage};
use features::sampler::{DamageKind, sample_damage_event, sample_fault, sample_fiber_fault};
use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId, LonLat, Origin, ServiceGroupId, Timestamp};
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{CutEnd, Fault, FaultKind, FaultSet, Location, Onset};
use schema::fiber::FiberPlant;
use schema::fiber_fault::{FiberFault, FiberFaultKind};
use schema::observation::{ChannelUsability, ModemObservation, RegState};
use schema::ops::{
    Call, CallReason, Counters, Crew, FeedItem, Job, JobSource, JobTarget, OpsRates, OpsRecords,
    Outcome, Ticket, TicketStatus, Truck, TruckState,
};
use schema::plant::{EdgeKind, ElementKind, Installation, Plant};
use schema::profile::Profile;
use schema::region::{Home, Street};
use schema::work_order::CauseClass;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sim::faults::{dist_ft, event_elapsed_s};
use sim::optics::{NodeLink, region_links};
use sim::rng::{Rng, hash_str};
use sim::state::ModemState;
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate_from};

use crate::field::{drive_s, position, route};
use crate::hazards::{HAZARDS, Hazard, chance_per_minute};

/// The step, seconds of world time.
pub const STEP_S: i64 = 60;
/// World time 0 is a Monday at this hour.
pub const START_HOUR: i64 = 6;
/// How often a world samples its outage for a chart, seconds of world time.
pub const SAMPLE_S: i64 = 600;
/// A node with nothing changing and every modem up is stepped this often, for temperature and slow wear.
const QUIET_STEP_S: i64 = 3600;
/// A modem is a home's: a step reads every eighth subcarrier, which is enough to say whether it works.
const DETAIL: Detail = Detail::Summary { stride: 32 };
/// Feet to a mile.
const FT_PER_MILE: f64 = 5280.0;
/// The twin's reading acts on a node with at least this many modems not operational.
const READING_MODEMS: usize = 3;
/// Every rate the world reads; a rates file missing one is refused.
const REQUIRED: [&str; 33] = [
    "callback_hours",
    "redial_min",
    "utility_outages_per_home_year",
    "utility_outage_hours",
    "storm_outage_hours",
    "dig_ins_per_underground_mile_year",
    "aerial_damage_per_mile_year",
    "wind_damage_exponent",
    "vehicle_hits_per_structure_year",
    "fiber_cuts_per_mile_year",
    "connector_wear_per_span_year",
    "amp_faults_per_amp_year",
    "water_per_span_year",
    "water_per_device_year",
    "rain_factor_per_mm_hr",
    "tap_corrosion_per_tap_year",
    "call_share_out",
    "call_share_slow",
    "call_delay_min",
    "handle_min",
    "patience_min",
    "agents_per_10000_homes",
    "service_trucks_per_10000_homes",
    "line_trucks_per_10000_homes",
    "splicers",
    "drive_mph",
    "repair_quick_min",
    "repair_cable_min",
    "repair_aerial_min",
    "repair_active_min",
    "repair_fiber_min",
    "no_trouble_found_min",
    "twin_reading_delay_min",
];

/// Why a world can't be made or run.
#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    /// The rates file is missing a rate.
    #[error("the rates file has no `{0}`")]
    Rate(String),
    /// A plant doesn't build.
    #[error("plant: {0}")]
    Plant(String),
    /// The sim refused.
    #[error(transparent)]
    Physics(#[from] sim::PhysicsError),
}

/// One node of the region.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeInput {
    /// Its plant.
    pub plant: Plant,
    /// Its profile.
    pub profile: Profile,
}

/// Everything a world is made from. Both worlds of a pair share it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldInputs {
    /// The parts.
    pub library: ComponentLibrary,
    /// The nodes.
    pub nodes: Vec<NodeInput>,
    /// The region's fiber, if it has any.
    pub fiber: Option<FiberPlant>,
    /// The streets the design was built on, which trucks drive.
    pub streets: Vec<Street>,
    /// The homes on them.
    pub homes: Vec<Home>,
    /// Where the trucks and the hub are.
    pub hub: LonLat,
    /// The rates.
    pub rates: OpsRates,
    /// The seed every draw derives from.
    pub seed: u64,
    /// How many times the rates file's rates the plant's hazards run at, so a demo sees a week's trouble in an hour. The page
    /// says it.
    pub intensity: f64,
}

/// How dispatch works.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// From calls: a truck to the first caller's home, a line crew once calls on a node pile up.
    Reactive,
    /// With the twin: its reading of telemetry sends the truck, and calls join the job it's on.
    Twin,
}

/// Something the user does, at a world time.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Action {
    /// Wind and rain over the region.
    Storm {
        /// How long.
        hours: f64,
        /// Wind, m/s.
        wind_m_per_s: f64,
        /// Rain, mm/h.
        rain_mm_per_hr: f64,
    },
    /// A heat wave: the air this many degrees above normal.
    HeatWave {
        /// How long.
        hours: f64,
        /// Degrees above normal.
        delta_c: f64,
    },
    /// The power company loses a circle.
    PowerCut {
        /// Its centre.
        center: LonLat,
        /// Its radius, feet.
        radius_ft: f64,
        /// How long until power is back.
        hours: f64,
    },
    /// An excavator through the buried span nearest a point, or a random one.
    DigIn {
        /// Near here.
        near: Option<LonLat>,
    },
    /// A car into the pole or pedestal nearest a point, or a random one.
    CarHit {
        /// Near here.
        near: Option<LonLat>,
    },
    /// A fiber cable cut.
    FiberCut,
}

/// A home's service, as its household lives it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Working.
    Up,
    /// Working badly: a channel down.
    Slow,
    /// Not working.
    Out,
    /// The house has no power.
    Off,
}

#[derive(Debug, Clone, PartialEq)]
struct ModemFacts {
    id: ElementId,
    geo: LonLat,
    tap: Option<ElementId>,
    drop: Option<EdgeId>,
    tap_input: Option<EdgeId>,
    premises: Vec<ElementId>,
}

#[derive(Debug, Clone, PartialEq)]
struct NodeFacts {
    sg: ServiceGroupId,
    root: ElementId,
    root_geo: LonLat,
    modems: Vec<ModemFacts>,
    geo: BTreeMap<ElementId, LonLat>,
    parent: BTreeMap<ElementId, (ElementId, EdgeId)>,
    child_spans: BTreeMap<ElementId, Vec<EdgeId>>,
    span_install: BTreeMap<EdgeId, Installation>,
    aerial: Vec<(EdgeId, f64)>,
    underground: Vec<(EdgeId, f64)>,
    spans: usize,
    amps: usize,
    taps: Vec<ElementId>,
    devices: usize,
    structures: Vec<ElementId>,
    supplies: Vec<(ElementId, LonLat, BTreeSet<EdgeId>)>,
}

#[derive(Debug, Clone)]
struct Prepared {
    streets: StreetGraph,
    cables: Vec<BTreeSet<String>>,
    facts: Vec<NodeFacts>,
    rates: BTreeMap<String, f64>,
    homes_per_outage: f64,
    homes: usize,
    fiber_miles: f64,
}

#[derive(Debug, Clone)]
struct Trouble {
    id: u64,
    /// Its place among the plant's troubles, the same in both worlds of a pair: hazards are drawn from the seed and the time
    /// alone, so troubles come in the same order whatever the trucks do.
    seq: u64,
    node: Option<usize>,
    fault: Option<Fault>,
    fiber: Option<FiberFault>,
    began_s: i64,
    label: String,
    first_call: Option<i64>,
    first_dispatch: Option<i64>,
    fixed: bool,
    ends_s: Option<i64>,
}

#[derive(Debug, Clone)]
struct NodeRun {
    states: BTreeMap<ElementId, ModemState>,
    last: Option<StepObservation>,
    last_t: Option<i64>,
    dirty: bool,
    standing: BTreeMap<ElementId, Standing>,
    /// What an operator sees of each home: its modem's state alone. A home with no house power reads as out, as it does to
    /// any CMTS; only the household, and a crew at its door, know it's the power.
    seen: BTreeMap<ElementId, Standing>,
}

#[derive(Debug, Clone)]
struct Household {
    node: usize,
    geo: LonLat,
    since: Option<(i64, Standing)>,
    call_at: Option<(i64, CallReason)>,
}

#[derive(Debug, Clone)]
struct Waiting {
    call: u64,
    gives_up_s: i64,
}

/// One world.
#[derive(Debug, Clone)]
pub struct World {
    /// How it dispatches.
    pub policy: Policy,
    /// World time, seconds.
    pub t_s: i64,
    inputs: Arc<WorldInputs>,
    prep: Arc<Prepared>,
    log: Vec<(i64, Action)>,
    applied: usize,
    storms: Vec<(i64, i64, f64, f64)>,
    heat: Vec<(i64, i64, f64)>,
    nodes: Vec<NodeRun>,
    troubles: Vec<Trouble>,
    links: BTreeMap<ServiceGroupId, NodeLink>,
    healthy: BTreeMap<ServiceGroupId, NodeLink>,
    households: BTreeMap<ElementId, Household>,
    calls: Vec<Call>,
    tickets: Vec<Ticket>,
    jobs: Vec<Job>,
    trucks: Vec<Truck>,
    waiting: VecDeque<Waiting>,
    agents_free_at: Vec<i64>,
    readings: Vec<(i64, usize, diagnosis::outage::OutageReading)>,
    escalated: BTreeMap<u64, JobTarget>,
    counters: Counters,
    feed: Vec<FeedItem>,
    next_id: u64,
    next_trouble: u64,
    series: Vec<Sample>,
    trips: Vec<Trip>,
}

/// One drive, as it was set off: for a page to draw every drive, however far apart it reads the world. Output only: nothing in
/// the world reads it. A drive cut short (a job called off, a truck turned round) is followed by the truck's next one.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Trip {
    /// The truck.
    pub truck: u64,
    /// The job it drives to, or none going back to the hub.
    pub job: Option<u64>,
    /// When it left, world seconds.
    pub depart_t_s: i64,
    /// When it gets there, world seconds.
    pub arrive_t_s: i64,
    /// Its route.
    pub path: Vec<LonLat>,
}

/// A world's outage at one time, every `SAMPLE_S`, for a chart. Output only: nothing in the world reads it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// World time, seconds.
    pub t_s: i64,
    /// Homes with no service, house power aside.
    pub homes_out: u64,
    /// Trucks not at the hub.
    pub trucks_out: u64,
    /// Calls waiting for an agent.
    pub calls_waiting: u64,
}

/// The weather and the hour, as the world's environment has them now: what a page needs to draw the sky.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Sky {
    /// Hours since midnight, with the minutes as a fraction.
    pub hour: f64,
    /// Days since the Monday the world started on.
    pub day: i64,
    /// The air, °C.
    pub temp_c: f64,
    /// Wind, m/s.
    pub wind_m_per_s: f64,
    /// Rain, mm/h.
    pub rain_mm_per_hr: f64,
    /// When the storm on now ends, if one is.
    pub storm_until_s: Option<i64>,
    /// Degrees a heat wave adds now.
    pub heat_delta_c: f64,
    /// When the heat wave on now ends, if one is.
    pub heat_until_s: Option<i64>,
}

/// One of the plant's open troubles, as the audience sees it and the dispatcher never does.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TruthItem {
    /// What it is, in words.
    pub label: String,
    /// Its fault's kind, as the fault schema names it, or `FiberCut` and so on for a fiber fault.
    pub kind: String,
    /// Where it is, if it has a place.
    pub geo: Option<LonLat>,
    /// A power cut's radius, feet.
    pub radius_ft: Option<f64>,
    /// When it began, world seconds.
    pub began_s: i64,
    /// The node it's on, if it's on one.
    pub service_group: Option<ServiceGroupId>,
}

fn rate(map: &BTreeMap<String, f64>, key: &str) -> f64 {
    map.get(key).copied().unwrap_or(0.0)
}

fn exp_draw(rng: &mut Rng, mean: f64) -> f64 {
    -mean * (1.0 - rng.uniform()).max(1e-12).ln()
}

fn hour_of(t_s: i64) -> i64 {
    (START_HOUR + t_s.div_euclid(3600)).rem_euclid(24)
}

fn weekday_of(t_s: i64) -> i64 {
    (t_s + START_HOUR * 3600).div_euclid(86_400).rem_euclid(7)
}

/// When a household awake at `t_s` would call, `delay_s` later, moved past the night: nobody calls from 23:00 to 07:00.
fn awake(t_s: i64) -> i64 {
    let h = hour_of(t_s);
    if (7..23).contains(&h) {
        return t_s;
    }
    let to_seven = (7 - h).rem_euclid(24) * 3600 - t_s.rem_euclid(3600);
    t_s + to_seven
}

fn facts_of(input: &NodeInput, library: &ComponentLibrary) -> Result<NodeFacts, WorldError> {
    let graph = PlantGraph::new(&input.plant, library, &input.profile)
        .map_err(|e| WorldError::Plant(format!("{e:?}")))?;
    let plant = &input.plant;
    let geo: BTreeMap<ElementId, LonLat> = plant
        .elements
        .iter()
        .filter_map(|e| e.geo.map(|g| (e.id.clone(), g)))
        .collect();
    let mut parent = BTreeMap::new();
    let mut child_spans: BTreeMap<ElementId, Vec<EdgeId>> = BTreeMap::new();
    let mut span_install = BTreeMap::new();
    let (mut aerial, mut underground) = (Vec::new(), Vec::new());
    let mut spans = 0;
    for e in &plant.edges {
        parent.insert(
            e.to.element_id.clone(),
            (e.from.element_id.clone(), e.id.clone()),
        );
        child_spans
            .entry(e.from.element_id.clone())
            .or_default()
            .push(e.id.clone());
        if let EdgeKind::Span {
            len_ft,
            installation,
            ..
        } = &e.kind
        {
            spans += 1;
            span_install.insert(e.id.clone(), *installation);
            match installation {
                Installation::Aerial => aerial.push((e.id.clone(), *len_ft)),
                _ => underground.push((e.id.clone(), *len_ft)),
            }
        }
    }
    let root = graph.root();
    let root_geo = root.geo.unwrap_or(LonLat {
        lon_deg: 0.0,
        lat_deg: 0.0,
    });
    let mut modems = Vec::new();
    for m in graph.modems() {
        let mut premises = Vec::new();
        let (mut tap, mut drop, mut tap_input) = (None, None, None);
        let mut at = m.id.clone();
        let mut via: Option<EdgeId> = None;
        while let Some((up, edge)) = parent.get(&at) {
            let kind = plant.elements.iter().find(|e| &e.id == up).map(|e| &e.kind);
            if matches!(kind, Some(ElementKind::Tap)) {
                tap = Some(up.clone());
                drop = Some(edge.clone());
                tap_input = parent.get(up).map(|(_, e)| e.clone());
                break;
            }
            premises.push(up.clone());
            via = Some(edge.clone());
            at = up.clone();
            if premises.len() > 8 {
                break;
            }
        }
        let _ = via;
        modems.push(ModemFacts {
            id: m.id.clone(),
            geo: m
                .geo
                .or_else(|| geo.get(&m.id).copied())
                .unwrap_or(root_geo),
            tap,
            drop,
            tap_input,
            premises,
        });
    }
    let kinds =
        |pred: fn(&ElementKind) -> bool| plant.elements.iter().filter(|e| pred(&e.kind)).count();
    let taps: Vec<ElementId> = plant
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Tap)
        .map(|e| e.id.clone())
        .collect();
    let structures: Vec<ElementId> = plant
        .elements
        .iter()
        .filter(|e| {
            e.structure.is_some() && matches!(e.kind, ElementKind::Tap | ElementKind::Amp { .. })
        })
        .map(|e| e.id.clone())
        .collect();
    // A supply's run is every span its current can reach: a short on a leg with no active beyond it still trips it (§20.4).
    let supplies = plant
        .power_supplies
        .iter()
        .map(|supply| {
            let at = supply
                .geo
                .or_else(|| geo.get(&supply.feeds).copied())
                .unwrap_or(root_geo);
            let run = plant
                .edges
                .iter()
                .filter(|e| {
                    matches!(
                        e.kind,
                        EdgeKind::Span {
                            power_passing: true,
                            ..
                        }
                    )
                })
                .filter(|e| graph.power_path(supply, &e.id).is_some())
                .map(|e| e.id.clone())
                .collect();
            (supply.id.clone(), at, run)
        })
        .collect();
    Ok(NodeFacts {
        sg: plant.service_group.clone(),
        root: root.id.clone(),
        root_geo,
        modems,
        geo,
        parent,
        child_spans,
        span_install,
        aerial,
        underground,
        spans,
        amps: kinds(|k| matches!(k, ElementKind::Amp { .. })),
        devices: kinds(|k| {
            matches!(
                k,
                ElementKind::Tap | ElementKind::Amp { .. } | ElementKind::Node { .. }
            )
        }),
        taps,
        structures,
        supplies,
    })
}

impl World {
    /// A world at time 0, every home working, the trucks at the hub.
    pub fn new(inputs: Arc<WorldInputs>, policy: Policy) -> Result<World, WorldError> {
        let prep = Arc::new(prepare(&inputs)?);
        World::with(inputs, prep, policy)
    }

    fn with(
        inputs: Arc<WorldInputs>,
        prep: Arc<Prepared>,
        policy: Policy,
    ) -> Result<World, WorldError> {
        let homes = prep.homes as f64;
        let per = |key: &str| (rate(&prep.rates, key) * homes / 10_000.0).round().max(1.0) as u64;
        let mut trucks = Vec::new();
        let mut id = 0;
        for (crew, n) in [
            (Crew::Service, per("service_trucks_per_10000_homes")),
            (Crew::Line, per("line_trucks_per_10000_homes")),
            (
                Crew::Splicer,
                rate(&prep.rates, "splicers").round().max(1.0) as u64,
            ),
        ] {
            for _ in 0..n {
                id += 1;
                trucks.push(Truck {
                    id,
                    crew,
                    geo: inputs.hub,
                    state: TruckState::Idle,
                    on_shift: true,
                });
            }
        }
        let agents = (per("agents_per_10000_homes") as usize).max(2);
        let mut households = BTreeMap::new();
        for (n, facts) in prep.facts.iter().enumerate() {
            for m in &facts.modems {
                households.insert(
                    m.id.clone(),
                    Household {
                        node: n,
                        geo: m.geo,
                        since: None,
                        call_at: None,
                    },
                );
            }
        }
        let pairs: Vec<(&Plant, &schema::profile::ChannelPlan)> = inputs
            .nodes
            .iter()
            .map(|n| (&n.plant, &n.profile.channel_plan))
            .collect();
        let healthy = match &inputs.fiber {
            Some(fiber) => region_links(fiber, &pairs, &inputs.library, &[], Timestamp(0))?,
            None => BTreeMap::new(),
        };
        let nodes = prep
            .facts
            .iter()
            .map(|f| NodeRun {
                states: BTreeMap::new(),
                last: None,
                last_t: None,
                dirty: true,
                standing: f
                    .modems
                    .iter()
                    .map(|m| (m.id.clone(), Standing::Up))
                    .collect(),
                seen: f
                    .modems
                    .iter()
                    .map(|m| (m.id.clone(), Standing::Up))
                    .collect(),
            })
            .collect();
        Ok(World {
            policy,
            t_s: 0,
            inputs,
            prep,
            log: Vec::new(),
            applied: 0,
            storms: Vec::new(),
            heat: Vec::new(),
            nodes,
            troubles: Vec::new(),
            links: healthy.clone(),
            healthy,
            households,
            calls: Vec::new(),
            tickets: Vec::new(),
            jobs: Vec::new(),
            trucks,
            waiting: VecDeque::new(),
            agents_free_at: vec![0; agents],
            readings: Vec::new(),
            escalated: BTreeMap::new(),
            counters: Counters::default(),
            feed: Vec::new(),
            series: vec![Sample {
                t_s: 0,
                homes_out: 0,
                trucks_out: 0,
                calls_waiting: 0,
            }],
            trips: Vec::new(),
            next_id: 0,
            next_trouble: 0,
        })
    }

    /// A fault placed by hand on a node, as a trouble from now: for tests, and for a tool that wants a particular fault.
    pub fn place(&mut self, node: usize, mut fault: Fault, label: &str) {
        if matches!(fault.onset, Onset::Instant) {
            fault.onset = Onset::Ramp {
                start: Timestamp(self.t_s),
                end: Timestamp(self.t_s),
            };
        }
        self.add_trouble(Some(node), fault, label);
    }

    /// Logs an action to take effect at the current time, applied at the next step.
    pub fn act(&mut self, action: Action) {
        self.act_at(self.t_s + STEP_S, action);
    }

    /// Logs an action to take effect at world time `at_s`, or at the next step if that's sooner: an action written ahead, as
    /// a scenario writes its day. The log stays in time order, an action at a time already logged going after those there, so
    /// the world it makes is the world made by acting at `at_s` itself.
    pub fn act_at(&mut self, at_s: i64, action: Action) {
        let at_s = at_s.max(self.t_s + STEP_S);
        let i = self.log.partition_point(|(t, _)| *t <= at_s);
        self.log.insert(i, (at_s, action));
    }

    /// Steps until world time reaches `to_s`.
    pub fn advance(&mut self, to_s: i64) -> Result<(), WorldError> {
        while self.t_s + STEP_S <= to_s {
            self.step()?;
        }
        Ok(())
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn say(&mut self, text: String, geo: Option<LonLat>, truth: bool) {
        self.feed.push(FeedItem {
            t_s: self.t_s,
            text,
            geo,
            truth,
        });
    }

    fn storm_at(&self, t: i64) -> Option<(f64, f64)> {
        self.storms
            .iter()
            .filter(|(a, b, _, _)| *a <= t && t < *b)
            .map(|(_, _, w, r)| (*w, *r))
            .fold(None, |acc: Option<(f64, f64)>, (w, r)| {
                Some(acc.map_or((w, r), |(aw, ar)| (aw.max(w), ar.max(r))))
            })
    }

    fn environment(&self, t: i64) -> EnvironmentSample {
        // The hour's air: a day's swing about 14 °C, warmest mid-afternoon (approx), held for the hour.
        let h = hour_of(t) as f64;
        let hot: f64 = self
            .heat
            .iter()
            .filter(|(a, b, _)| *a <= t && t < *b)
            .map(|(_, _, d)| *d)
            .sum();
        let temp_c = 14.0 + 8.0 * ((h - 9.0) / 24.0 * std::f64::consts::TAU).sin() + hot;
        let (wind, rain) = self.storm_at(t).unwrap_or((3.0, 0.0));
        EnvironmentSample {
            t: Timestamp(t),
            temp_c,
            ground_temp_c: 10.0 + (temp_c - 10.0) * 0.3,
            rain_mm_per_hr: rain,
            solar_w_per_m2: None,
            wind_m_per_s: Some(wind),
        }
    }

    /// One minute.
    pub fn step(&mut self) -> Result<(), WorldError> {
        self.t_s += STEP_S;
        let t = self.t_s;
        self.apply_actions()?;
        self.hazards()?;
        self.end_timed();
        self.sim_nodes()?;
        self.households_live();
        self.call_centre();
        if self.policy == Policy::Twin {
            self.twin_reads();
        }
        self.reactive_escalation();
        self.field()?;
        self.close_tickets();
        if t % SAMPLE_S == 0 {
            self.series.push(self.sample());
        }
        Ok(())
    }

    fn sample(&self) -> Sample {
        Sample {
            t_s: self.t_s,
            homes_out: self.counters.homes_out_now,
            trucks_out: self
                .trucks
                .iter()
                .filter(|k| k.state != TruckState::Idle)
                .count() as u64,
            calls_waiting: self.waiting.len() as u64,
        }
    }

    fn apply_actions(&mut self) -> Result<(), WorldError> {
        while self.applied < self.log.len() && self.log[self.applied].0 <= self.t_s {
            let action = self.log[self.applied].1.clone();
            self.applied += 1;
            let t = self.t_s;
            match action {
                Action::Storm {
                    hours,
                    wind_m_per_s,
                    rain_mm_per_hr,
                } => {
                    self.storms.push((
                        t,
                        t + (hours * 3600.0) as i64,
                        wind_m_per_s,
                        rain_mm_per_hr,
                    ));
                    self.say(
                        format!("A storm: {wind_m_per_s:.0} m/s wind and {rain_mm_per_hr:.0} mm/h of rain for {hours:.0} hours"),
                        None,
                        true,
                    );
                    for n in &mut self.nodes {
                        n.dirty = true;
                    }
                }
                Action::HeatWave { hours, delta_c } => {
                    self.heat.push((t, t + (hours * 3600.0) as i64, delta_c));
                    self.say(
                        format!("A heat wave: {delta_c:.0} °C above normal for {hours:.0} hours"),
                        None,
                        true,
                    );
                    for n in &mut self.nodes {
                        n.dirty = true;
                    }
                }
                Action::PowerCut {
                    center,
                    radius_ft,
                    hours,
                } => {
                    self.utility_outage(
                        center,
                        radius_ft,
                        hours * 3600.0,
                        "The power company loses a neighbourhood",
                    );
                }
                Action::DigIn { near } => {
                    let mut rng = Rng::derive(self.inputs.seed, &[t as u64, 7001]);
                    self.dig_in(near, &mut rng)?;
                }
                Action::CarHit { near } => {
                    let mut rng = Rng::derive(self.inputs.seed, &[t as u64, 7002]);
                    self.car_hit(near, &mut rng)?;
                }
                Action::FiberCut => {
                    let mut rng = Rng::derive(self.inputs.seed, &[t as u64, 7003]);
                    self.fiber_cut(&mut rng);
                }
            }
        }
        Ok(())
    }

    /// The plant's processes, drawn a minute at a time from the seed and the time alone, so two worlds on one seed see the
    /// same hazards whatever their trucks do.
    fn hazards(&mut self) -> Result<(), WorldError> {
        let t = self.t_s;
        let prep = Arc::clone(&self.prep);
        let r = |k: &str| rate(&prep.rates, k);
        let intensity = self.inputs.intensity;
        let (wind, rain) = self.storm_at(t).unwrap_or((3.0, 0.0));
        let wet = 1.0 + r("rain_factor_per_mm_hr") * rain;
        let windy = 1.0 + (wind / 8.0).powf(r("wind_damage_exponent"));
        let daytime = (8..17).contains(&hour_of(t)) && weekday_of(t) < 5;
        let sum = |f: &dyn Fn(&NodeFacts) -> f64| prep.facts.iter().map(f).sum::<f64>();
        for hazard in HAZARDS {
            let units = match hazard {
                Hazard::ConnectorWear | Hazard::WaterInSpan => sum(&|f| f.spans as f64),
                Hazard::AmpFault => sum(&|f| f.amps as f64),
                Hazard::WaterInDevice => sum(&|f| f.devices as f64),
                Hazard::TapCorrosion => sum(&|f| f.taps.len() as f64),
                Hazard::AerialDamage => {
                    sum(&|f| f.aerial.iter().map(|a| a.1).sum::<f64>()) / FT_PER_MILE
                }
                Hazard::DigIn => {
                    sum(&|f| f.underground.iter().map(|a| a.1).sum::<f64>()) / FT_PER_MILE
                }
                Hazard::VehicleHit => sum(&|f| f.structures.len() as f64),
                Hazard::UtilityOutage => prep.homes as f64 / prep.homes_per_outage.max(1.0),
                Hazard::FiberCut => prep.fiber_miles,
            };
            let modulated = match hazard {
                Hazard::WaterInSpan | Hazard::WaterInDevice => wet,
                Hazard::AerialDamage => windy,
                // The power company's rate is sourced and isn't the demo's to raise: only weather raises it.
                Hazard::UtilityOutage => windy,
                Hazard::DigIn => {
                    if daytime {
                        // Dug only by day on weekdays: the year's rate squeezed into those hours.
                        168.0 / 45.0
                    } else {
                        0.0
                    }
                }
                _ => 1.0,
            };
            let scale = if hazard == Hazard::UtilityOutage {
                1.0
            } else {
                intensity
            };
            let p = chance_per_minute(r(hazard.rate_key()) * units * modulated * scale);
            let mut rng = Rng::derive(self.inputs.seed, &[t as u64, hash_str(hazard.rate_key())]);
            if rng.uniform() >= p {
                continue;
            }
            self.fire(hazard, &mut rng)?;
        }
        Ok(())
    }

    fn pick_node(&self, rng: &mut Rng, weight: &dyn Fn(&NodeFacts) -> f64) -> Option<usize> {
        let total: f64 = self.prep.facts.iter().map(weight).sum();
        if total <= 0.0 {
            return None;
        }
        let mut at = rng.uniform() * total;
        for (i, f) in self.prep.facts.iter().enumerate() {
            at -= weight(f);
            if at < 0.0 {
                return Some(i);
            }
        }
        Some(self.prep.facts.len() - 1)
    }

    fn graph_of(&self, n: usize) -> Result<PlantGraph<'_>, WorldError> {
        let input = &self.inputs.nodes[n];
        PlantGraph::new(&input.plant, &self.inputs.library, &input.profile)
            .map_err(|e| WorldError::Plant(format!("{e:?}")))
    }

    fn fire(&mut self, hazard: Hazard, rng: &mut Rng) -> Result<(), WorldError> {
        let t = self.t_s;
        let at_t = Onset::Ramp {
            start: Timestamp(t),
            end: Timestamp(t),
        };
        match hazard {
            Hazard::UtilityOutage => {
                let homes: Vec<LonLat> = self.households.values().map(|h| h.geo).collect();
                if homes.is_empty() {
                    return Ok(());
                }
                let center = homes[rng.index(homes.len())];
                let storm = self.storm_at(t).is_some();
                let hours = exp_draw(
                    rng,
                    rate(
                        &self.prep.rates,
                        if storm {
                            "storm_outage_hours"
                        } else {
                            "utility_outage_hours"
                        },
                    ),
                )
                .clamp(0.25, 48.0);
                self.utility_outage(
                    center,
                    rng.range(100.0, 800.0),
                    hours * 3600.0,
                    "The power company loses a neighbourhood",
                );
                return Ok(());
            }
            Hazard::FiberCut => {
                self.fiber_cut(rng);
                return Ok(());
            }
            Hazard::DigIn => return self.dig_in(None, rng),
            Hazard::VehicleHit => return self.car_hit(None, rng),
            _ => {}
        }
        let weight: &dyn Fn(&NodeFacts) -> f64 = match hazard {
            Hazard::AmpFault => &|f| f.amps as f64,
            Hazard::TapCorrosion => &|f| f.taps.len() as f64,
            Hazard::AerialDamage => &|f| f.aerial.iter().map(|a| a.1).sum(),
            Hazard::WaterInDevice => &|f| f.devices as f64,
            _ => &|f| f.spans as f64,
        };
        let Some(n) = self.pick_node(rng, weight) else {
            return Ok(());
        };
        let fault = {
            let graph = self.graph_of(n)?;
            match hazard {
                Hazard::ConnectorWear => sample_fault(&graph, CauseClass::ConnectorReflection, rng)
                    .map(|mut f| {
                        // Worse over days, as a connector loosens or corrodes.
                        let days = rng.range(1.0, 10.0);
                        f.onset = Onset::Ramp {
                            start: Timestamp(t),
                            end: Timestamp(t + (days * 86_400.0) as i64),
                        };
                        f
                    }),
                Hazard::AmpFault => sample_fault(&graph, CauseClass::AmpDrift, rng),
                Hazard::WaterInSpan => sample_fault(&graph, CauseClass::WaterIngress, rng),
                Hazard::WaterInDevice => {
                    sample_damage_event(&graph, None, DamageKind::Water, rng).and_then(|e| e.fault)
                }
                Hazard::TapCorrosion => {
                    let facts = &self.prep.facts[n];
                    (!facts.taps.is_empty()).then(|| Fault {
                        onset: Onset::Ramp {
                            start: Timestamp(t),
                            end: Timestamp(t + 7 * 86_400),
                        },
                        kind: FaultKind::DamagedDevice {
                            element_id: facts.taps[rng.index(facts.taps.len())].clone(),
                            through_loss_db: rng.range(0.0, 3.0),
                            return_loss_db: rng.range(8.0, 16.0),
                            ports_lost: Vec::new(),
                            ingress_power_dbmv: None,
                            cpd_second_order_dbmv_per_mhz: Some(rng.range(-60.0, -40.0)),
                        },
                    })
                }
                Hazard::AerialDamage => {
                    let spans = &self.prep.facts[n].aerial;
                    by_length(spans, rng).map(|(span, len)| Fault {
                        onset: at_t.clone(),
                        kind: FaultKind::SpanCut {
                            span_id: span.clone(),
                            pos_ft: rng.uniform() * len,
                            end: if rng.uniform() < 0.5 {
                                CutEnd::Open
                            } else {
                                CutEnd::Shorted
                            },
                            repair_s: None,
                        },
                    })
                }
                _ => None,
            }
        };
        if let Some(mut fault) = fault {
            if matches!(fault.onset, Onset::Instant) {
                fault.onset = at_t;
            }
            self.add_trouble(Some(n), fault, hazard.label());
        }
        Ok(())
    }

    fn add_trouble(&mut self, node: Option<usize>, fault: Fault, label: &str) {
        let id = self.id();
        let geo = node.and_then(|n| self.fault_geo(n, &fault.kind));
        let place = node
            .map(|n| self.prep.facts[n].sg.to_string())
            .unwrap_or_default();
        self.next_trouble += 1;
        self.troubles.push(Trouble {
            id,
            seq: self.next_trouble,
            node,
            fault: Some(fault),
            fiber: None,
            began_s: self.t_s,
            label: label.to_string(),
            first_call: None,
            first_dispatch: None,
            fixed: false,
            ends_s: None,
        });
        if let Some(n) = node {
            self.nodes[n].dirty = true;
        }
        self.say(format!("{} on {place}", capital(label)), geo, true);
    }

    fn utility_outage(&mut self, center: LonLat, radius_ft: f64, repair_s: f64, text: &str) {
        let t = self.t_s;
        let id = self.id();
        self.next_trouble += 1;
        self.troubles.push(Trouble {
            id,
            seq: self.next_trouble,
            node: None,
            fault: Some(Fault {
                onset: Onset::Ramp {
                    start: Timestamp(t),
                    end: Timestamp(t),
                },
                kind: FaultKind::UtilityOutage {
                    center,
                    radius_ft,
                    repair_s: Some(repair_s),
                },
            }),
            fiber: None,
            began_s: t,
            label: "a power cut".into(),
            first_call: None,
            first_dispatch: None,
            fixed: false,
            ends_s: Some(t + repair_s as i64),
        });
        for n in &mut self.nodes {
            n.dirty = true;
        }
        self.say(
            format!(
                "{text}: {radius_ft:.0} ft around, for {:.1} hours",
                repair_s / 3600.0
            ),
            Some(center),
            true,
        );
    }

    fn dig_in(&mut self, near: Option<LonLat>, rng: &mut Rng) -> Result<(), WorldError> {
        let pick = self.near_or_random(near, rng, |f| &f.underground);
        if let Some((n, span, len)) = pick {
            let fault = Fault {
                onset: Onset::Ramp {
                    start: Timestamp(self.t_s),
                    end: Timestamp(self.t_s),
                },
                kind: FaultKind::SpanCut {
                    span_id: span,
                    pos_ft: rng.uniform() * len,
                    end: if rng.uniform() < 0.5 {
                        CutEnd::Open
                    } else {
                        CutEnd::Shorted
                    },
                    repair_s: None,
                },
            };
            self.add_trouble(Some(n), fault, Hazard::DigIn.label());
        } else {
            // A disaster with nothing to hit says so, rather than leaving the user to wonder where it went.
            self.say(
                "An excavator digs, but none of this region's cable is buried, so it hits nothing"
                    .into(),
                near,
                true,
            );
        }
        Ok(())
    }

    fn car_hit(&mut self, near: Option<LonLat>, rng: &mut Rng) -> Result<(), WorldError> {
        let candidates: Vec<(usize, ElementId, LonLat)> = self
            .prep
            .facts
            .iter()
            .enumerate()
            .flat_map(|(n, f)| {
                f.structures
                    .iter()
                    .filter_map(move |e| f.geo.get(e).map(|g| (n, e.clone(), *g)))
            })
            .collect();
        if candidates.is_empty() {
            self.say(
                "A car leaves the road, but there's no pole or pedestal in this region to hit"
                    .into(),
                near,
                true,
            );
            return Ok(());
        }
        let (n, element, _) = match near {
            Some(p) => candidates
                .iter()
                .min_by(|a, b| dist_ft(a.2, p).total_cmp(&dist_ft(b.2, p)))
                .cloned()
                .unwrap_or_else(|| candidates[0].clone()),
            None => candidates[rng.index(candidates.len())].clone(),
        };
        let severity = rng.range(0.5, 1.0);
        let graph = self.graph_of(n)?;
        let ports_lost = graph
            .child_edges(&element)
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Drop { .. }))
            .filter(|_| rng.uniform() < severity)
            .map(|e| e.from.port.clone())
            .collect();
        let is_tap = graph
            .element(&element)
            .is_ok_and(|e| e.kind == ElementKind::Tap);
        drop(graph);
        let kind = if is_tap {
            FaultKind::DamagedDevice {
                element_id: element,
                through_loss_db: 30.0 * severity,
                return_loss_db: 20.0 - 18.0 * severity,
                ports_lost,
                ingress_power_dbmv: Some(-40.0 + 40.0 * severity),
                cpd_second_order_dbmv_per_mhz: None,
            }
        } else {
            FaultKind::AmpDrift {
                amp_id: element,
                state: schema::fault::AmpState::Failed,
            }
        };
        self.add_trouble(
            Some(n),
            Fault {
                onset: Onset::Ramp {
                    start: Timestamp(self.t_s),
                    end: Timestamp(self.t_s),
                },
                kind,
            },
            Hazard::VehicleHit.label(),
        );
        Ok(())
    }

    fn fiber_cut(&mut self, rng: &mut Rng) {
        let none = |w: &mut World| {
            w.say(
                "A fiber cable would be cut, but this region has no fiber plant to cut".into(),
                None,
                true,
            )
        };
        let Some(fiber) = self.inputs.fiber.clone() else {
            return none(self);
        };
        if self.prep.facts.is_empty() {
            return none(self);
        }
        let n = rng.index(self.prep.facts.len());
        let sg = self.prep.facts[n].sg.clone();
        let Some(mut cut) = sample_fiber_fault(&fiber, &sg, CauseClass::FiberCut, rng) else {
            return none(self);
        };
        if let FiberFaultKind::FiberCut { repair_s, .. } = &mut cut.kind {
            *repair_s = None;
        }
        let id = self.id();
        let place = match &cut.kind {
            FiberFaultKind::FiberCut { cable, .. } => cable.to_string(),
            _ => String::new(),
        };
        self.next_trouble += 1;
        self.troubles.push(Trouble {
            id,
            seq: self.next_trouble,
            node: None,
            fault: None,
            fiber: Some(cut),
            began_s: self.t_s,
            label: Hazard::FiberCut.label().into(),
            first_call: None,
            first_dispatch: None,
            fixed: false,
            ends_s: None,
        });
        self.relink();
        self.say(format!("A fiber cut on {place}"), None, true);
    }

    fn relink(&mut self) {
        let Some(fiber) = &self.inputs.fiber else {
            return;
        };
        let faults: Vec<FiberFault> = self
            .troubles
            .iter()
            .filter(|t| !t.fixed)
            .filter_map(|t| t.fiber.clone())
            .collect();
        let pairs: Vec<(&Plant, &schema::profile::ChannelPlan)> = self
            .inputs
            .nodes
            .iter()
            .map(|n| (&n.plant, &n.profile.channel_plan))
            .collect();
        if let Ok(links) = region_links(
            fiber,
            &pairs,
            &self.inputs.library,
            &faults,
            Timestamp(self.t_s),
        ) {
            for (i, f) in self.prep.facts.iter().enumerate() {
                if links.get(&f.sg) != self.links.get(&f.sg) {
                    self.nodes[i].dirty = true;
                }
            }
            self.links = links;
        }
    }

    fn near_or_random(
        &self,
        near: Option<LonLat>,
        rng: &mut Rng,
        spans: fn(&NodeFacts) -> &Vec<(EdgeId, f64)>,
    ) -> Option<(usize, EdgeId, f64)> {
        match near {
            Some(p) => {
                let mut best: Option<(f64, usize, EdgeId, f64)> = None;
                for (n, f) in self.prep.facts.iter().enumerate() {
                    for (span, len) in spans(f) {
                        let Some(ends) = self.span_geo(n, span) else {
                            continue;
                        };
                        let d = dist_ft(ends, p);
                        if best.as_ref().is_none_or(|b| d < b.0) {
                            best = Some((d, n, span.clone(), *len));
                        }
                    }
                }
                best.map(|(_, n, s, l)| (n, s, l))
            }
            None => {
                let n = self.pick_node(rng, &|f| spans(f).iter().map(|a| a.1).sum())?;
                by_length(spans(&self.prep.facts[n]), rng).map(|(s, l)| (n, s.clone(), l))
            }
        }
    }

    fn span_geo(&self, n: usize, span: &EdgeId) -> Option<LonLat> {
        let f = &self.prep.facts[n];
        let edge = self.inputs.nodes[n]
            .plant
            .edges
            .iter()
            .find(|e| &e.id == span)?;
        let (a, b) = (
            f.geo.get(&edge.from.element_id)?,
            f.geo.get(&edge.to.element_id)?,
        );
        Some(LonLat {
            lon_deg: (a.lon_deg + b.lon_deg) / 2.0,
            lat_deg: (a.lat_deg + b.lat_deg) / 2.0,
        })
    }

    fn fault_geo(&self, n: usize, kind: &FaultKind) -> Option<LonLat> {
        let f = &self.prep.facts[n];
        match place_of(kind) {
            Place::Span(s) => self.span_geo(n, &s),
            Place::Element(e) | Place::Modem(e) => f.geo.get(&e).copied(),
            Place::Area(c) => Some(c),
            Place::None => None,
        }
    }

    /// Timed ends: the power company's restorations.
    fn end_timed(&mut self) {
        let t = self.t_s;
        let mut ended = Vec::new();
        for tr in &mut self.troubles {
            if !tr.fixed && tr.ends_s.is_some_and(|e| e <= t) {
                tr.fixed = true;
                ended.push(tr.label.clone());
            }
        }
        if !ended.is_empty() {
            for n in &mut self.nodes {
                n.dirty = true;
            }
            self.say(
                "The power company restores a neighbourhood".into(),
                None,
                true,
            );
        }
    }

    fn faults_of(&self, n: usize) -> Vec<Fault> {
        self.troubles
            .iter()
            .filter(|t| !t.fixed && (t.node == Some(n) || t.node.is_none()))
            .filter_map(|t| t.fault.clone())
            .collect()
    }

    fn sim_nodes(&mut self) -> Result<(), WorldError> {
        let t = self.t_s;
        let storm = self.storm_at(t).is_some();
        let sample = self.environment(t);
        // Rain moves only water faults, so a storm steps only the nodes that have one.
        let wet: BTreeSet<usize> = self
            .troubles
            .iter()
            .filter(|tr| !tr.fixed)
            .filter(|tr| {
                matches!(
                    tr.fault.as_ref().map(|f| &f.kind),
                    Some(FaultKind::WaterIngress { .. } | FaultKind::WaterInDevice { .. })
                )
            })
            .filter_map(|tr| tr.node)
            .collect();
        let due: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| {
                let n = &self.nodes[i];
                n.dirty
                    || (storm && wet.contains(&i))
                    || n.last_t.is_none_or(|lt| t - lt >= QUIET_STEP_S)
                    || n.last.as_ref().is_some_and(|s| {
                        s.modems
                            .iter()
                            .any(|m| m.reg_state != RegState::Operational)
                    })
            })
            .collect();
        let jobs: Vec<(usize, Vec<Fault>, Option<NodeLink>)> = due
            .iter()
            .map(|&i| {
                (
                    i,
                    self.faults_of(i),
                    self.links.get(&self.prep.facts[i].sg).cloned(),
                )
            })
            .collect();
        let inputs = Arc::clone(&self.inputs);
        let seed = inputs.seed;
        let mut taken: Vec<(usize, NodeRun)> =
            due.iter().map(|&i| (i, self.nodes[i].clone())).collect();
        let workers = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .min(taken.len().max(1));
        let chunk = taken.len().div_ceil(workers).max(1);
        let results: Vec<Result<(), WorldError>> = std::thread::scope(|scope| {
            let handles: Vec<_> = taken
                .chunks_mut(chunk)
                .map(|part| {
                    let inputs = &inputs;
                    let jobs = &jobs;
                    let sample = &sample;
                    scope.spawn(move || -> Result<(), WorldError> {
                        for (i, run) in part.iter_mut() {
                            let (_, faults, link) = jobs
                                .iter()
                                .find(|j| j.0 == *i)
                                .ok_or_else(|| WorldError::Plant("lost node".into()))?;
                            let input = &inputs.nodes[*i];
                            let graph =
                                PlantGraph::new(&input.plant, &inputs.library, &input.profile)
                                    .map_err(|e| WorldError::Plant(format!("{e:?}")))?;
                            let environment = Environment {
                                schema_version: schema::environment::SCHEMA_VERSION.to_string(),
                                samples: vec![sample.clone()],
                                ambient_sources: Vec::new(),
                            };
                            let set = FaultSet {
                                schema_version: schema::fault::SCHEMA_VERSION.to_string(),
                                seed: seed ^ hash_str(input.plant.service_group.as_str()),
                                faults: faults.clone(),
                            };
                            let mut steps = simulate_from(
                                &Scenario {
                                    graph: &graph,
                                    environment: &environment,
                                    faults: &set,
                                    seed: seed ^ hash_str(input.plant.service_group.as_str()),
                                    options: SimOptions {
                                        detail: DETAIL,
                                        ..SimOptions::default()
                                    },
                                    link: link.as_ref(),
                                },
                                &mut run.states,
                                run.last_t.map(Timestamp),
                            )?;
                            run.last = steps.pop();
                            run.last_t = Some(sample.t.0);
                            run.dirty = false;
                        }
                        Ok(())
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(WorldError::Plant("a worker panicked".into())))
                })
                .collect()
        });
        for r in results {
            r?;
        }
        for (i, run) in taken {
            self.nodes[i] = run;
            if self.policy == Policy::Twin
                && let Some(step) = &self.nodes[i].last
            {
                let sg = &self.prep.facts[i].sg;
                let graph = self.graph_of(i)?;
                let design = self.healthy.get(sg).map(LinkReading::of);
                let reading = read_outage(
                    &graph,
                    &step.modems,
                    step.service_group.optics.as_ref(),
                    design.as_ref(),
                );
                let delay = (rate(&self.prep.rates, "twin_reading_delay_min") * 60.0) as i64;
                self.readings.push((t + delay, i, reading));
            }
        }
        // Standings: what each home lives, from its modem; house power is the household's own to know.
        let outages: Vec<(LonLat, f64)> =
            self.troubles
                .iter()
                .filter(|tr| !tr.fixed)
                .filter_map(|tr| match &tr.fault {
                    Some(Fault {
                        kind:
                            FaultKind::UtilityOutage {
                                center,
                                radius_ft,
                                repair_s,
                            },
                        onset,
                    }) => event_elapsed_s(onset, Timestamp(t), *repair_s)
                        .map(|_| (*center, *radius_ft)),
                    _ => None,
                })
                .collect();
        for i in &due {
            let Some(step) = self.nodes[*i].last.clone() else {
                continue;
            };
            for m in &step.modems {
                let geo = self.households.get(&m.element_id).map(|h| h.geo);
                let off = geo.is_some_and(|g| outages.iter().any(|(c, r)| dist_ft(*c, g) <= *r));
                let standing = if off { Standing::Off } else { standing_of(m) };
                self.nodes[*i]
                    .standing
                    .insert(m.element_id.clone(), standing);
                self.nodes[*i]
                    .seen
                    .insert(m.element_id.clone(), standing_of(m));
            }
        }
        let out = self
            .nodes
            .iter()
            .flat_map(|n| n.standing.values())
            .filter(|s| **s == Standing::Out)
            .count() as u64;
        self.counters.homes_out_now = out;
        self.counters.customer_minutes_out += out * (STEP_S as u64 / 60);
        Ok(())
    }

    fn households_live(&mut self) {
        let t = self.t_s;
        let rates = Arc::clone(&self.prep);
        let share_out = rate(&rates.rates, "call_share_out");
        let share_slow = rate(&rates.rates, "call_share_slow");
        let delay_min = rate(&rates.rates, "call_delay_min");
        let callback_h = rate(&rates.rates, "callback_hours");
        let mut calls: Vec<(ElementId, usize, CallReason)> = Vec::new();
        for (id, h) in self.households.iter_mut() {
            let now = self.nodes[h.node]
                .standing
                .get(id)
                .copied()
                .unwrap_or(Standing::Up);
            let bad = matches!(now, Standing::Out | Standing::Slow);
            match (&h.since, bad) {
                (None, true) => {
                    // Whether this household calls about this trouble, and when, drawn once as it starts.
                    let mut rng =
                        Rng::derive(self.inputs.seed, &[hash_str(id.as_str()), t as u64, 11]);
                    let share = if now == Standing::Out {
                        share_out
                    } else {
                        share_slow
                    };
                    h.since = Some((t, now));
                    if rng.uniform() < share {
                        let at = awake(t + (exp_draw(&mut rng, delay_min) * 60.0) as i64);
                        let reason = if now == Standing::Out {
                            CallReason::NoService
                        } else {
                            CallReason::Slow
                        };
                        h.call_at = Some((at, reason));
                    }
                }
                (Some(_), false) => {
                    h.since = None;
                    h.call_at = None;
                }
                _ => {}
            }
            if let Some((at, reason)) = h.call_at
                && at <= t
            {
                // Still out later, they'll call back.
                let mut rng = Rng::derive(self.inputs.seed, &[hash_str(id.as_str()), t as u64, 15]);
                h.call_at = Some((
                    awake(t + (exp_draw(&mut rng, callback_h) * 3600.0) as i64),
                    reason,
                ));
                calls.push((id.clone(), h.node, reason));
            }
        }
        let patience = rate(&self.prep.rates, "patience_min");
        for (home, node, reason) in calls {
            let id = self.id();
            let sg = self.prep.facts[node].sg.clone();
            let known = self.known_outage(node);
            let geo = self.households.get(&home).map(|h| h.geo);
            self.calls.push(Call {
                id,
                t_s: t,
                home: home.clone(),
                service_group: sg.clone(),
                reason,
                answered_t_s: None,
                abandoned_t_s: None,
                ticket: None,
            });
            self.counters.calls += 1;
            if known {
                // The phone line says the outage is known and a truck is on it: no agent, and the call joins its ticket.
                if let Some(c) = self.calls.last_mut() {
                    c.answered_t_s = Some(t);
                }
                self.counters.calls_told_of_outage += 1;
                if let Some(k) = self
                    .tickets
                    .iter_mut()
                    .find(|k| k.service_group == sg && k.status != TicketStatus::Closed)
                {
                    k.calls.push(id);
                    let ticket = k.id;
                    if let Some(c) = self.calls.last_mut() {
                        c.ticket = Some(ticket);
                    }
                }
            } else {
                let mut rng =
                    Rng::derive(self.inputs.seed, &[hash_str(home.as_str()), t as u64, 12]);
                self.waiting.push_back(Waiting {
                    call: id,
                    gives_up_s: t + (exp_draw(&mut rng, patience) * 60.0) as i64,
                });
            }
            for tr in &mut self.troubles {
                if !tr.fixed
                    && (tr.node == Some(node) || tr.node.is_none())
                    && tr.first_call.is_none()
                {
                    tr.first_call = Some(t);
                    self.counters.onset_to_first_call.0 += ((t - tr.began_s) / 60) as u64;
                    self.counters.onset_to_first_call.1 += 1;
                }
            }
            let says = if reason == CallReason::NoService {
                "no service"
            } else {
                "slow service"
            };
            self.say(format!("A call from a home on {sg}: {says}"), geo, false);
        }
    }

    fn call_centre(&mut self) {
        let t = self.t_s;
        let h = hour_of(t);
        let on = if (7..21).contains(&h) {
            self.agents_free_at.len()
        } else {
            1
        };
        // Callers who've waited past their patience hang up.
        let redial = rate(&self.prep.rates, "redial_min");
        let mut still = VecDeque::new();
        while let Some(w) = self.waiting.pop_front() {
            if w.gives_up_s <= t {
                let mut home = None;
                if let Some(c) = self.calls.iter_mut().find(|c| c.id == w.call) {
                    c.abandoned_t_s = Some(t);
                    home = Some((c.home.clone(), c.reason));
                }
                self.counters.calls_abandoned += 1;
                // Still out, they'll try again.
                if let Some((home, reason)) = home
                    && let Some(h) = self.households.get_mut(&home)
                    && h.since.is_some()
                {
                    let mut rng = Rng::derive(self.inputs.seed, &[w.call, 14]);
                    h.call_at = Some((
                        awake(t + (exp_draw(&mut rng, redial) * 60.0) as i64),
                        reason,
                    ));
                }
            } else {
                still.push_back(w);
            }
        }
        self.waiting = still;
        let handle = rate(&self.prep.rates, "handle_min");
        for agent in 0..on {
            if self.agents_free_at[agent] > t {
                continue;
            }
            let Some(w) = self.waiting.pop_front() else {
                break;
            };
            let mut rng = Rng::derive(self.inputs.seed, &[w.call, 13]);
            self.agents_free_at[agent] = t + (exp_draw(&mut rng, handle) * 60.0) as i64;
            self.counters.calls_answered += 1;
            self.answer(w.call);
        }
    }

    fn answer(&mut self, call_id: u64) {
        let t = self.t_s;
        let Some(ci) = self.calls.iter().position(|c| c.id == call_id) else {
            return;
        };
        self.calls[ci].answered_t_s = Some(t);
        let (sg, home) = (
            self.calls[ci].service_group.clone(),
            self.calls[ci].home.clone(),
        );
        let open = self
            .tickets
            .iter()
            .position(|k| k.service_group == sg && k.status != TicketStatus::Closed);
        let ticket = match open {
            Some(k) => {
                self.tickets[k].calls.push(call_id);
                self.tickets[k].id
            }
            None => {
                let id = self.id();
                self.tickets.push(Ticket {
                    id,
                    service_group: sg.clone(),
                    opened_t_s: t,
                    first_home: home.clone(),
                    calls: vec![call_id],
                    status: TicketStatus::Open,
                    jobs: Vec::new(),
                    closed_t_s: None,
                });
                self.counters.tickets += 1;
                let node = self.node_index(&sg);
                // With the twin, a ticket on a node it's already sent a truck to joins that job.
                if self.policy == Policy::Twin
                    && let Some(n) = node
                    && let Some(job) = self.open_job_covering(n, &home)
                {
                    if let Some(k) = self.tickets.last_mut() {
                        k.jobs.push(job);
                        k.status = TicketStatus::Dispatched;
                    }
                } else {
                    let geo = self
                        .households
                        .get(&home)
                        .map(|h| h.geo)
                        .unwrap_or(self.inputs.hub);
                    let job = self.new_job(
                        &sg,
                        JobTarget::Home {
                            modem: home.clone(),
                        },
                        geo,
                        JobSource::Ticket(id),
                        Crew::Service,
                    );
                    if let Some(k) = self.tickets.last_mut() {
                        k.jobs.push(job);
                        k.status = TicketStatus::Dispatched;
                    }
                }
                id
            }
        };
        self.calls[ci].ticket = Some(ticket);
    }

    /// Whether the phone line knows of an outage on a node: with the twin, once it has a truck on the node from telemetry;
    /// from calls, once the node's ticket has piled up enough calls for dispatch to call it an outage.
    fn known_outage(&self, n: usize) -> bool {
        let sg = &self.prep.facts[n].sg;
        let after = rate(&self.prep.rates, "escalate_after_tickets").max(1.0) as usize;
        let by_calls = self.tickets.iter().any(|k| {
            &k.service_group == sg && k.status != TicketStatus::Closed && k.calls.len() >= after
        });
        let by_twin = self.policy == Policy::Twin
            && (self.jobs.iter().any(|j| {
                &j.service_group == sg && j.source == JobSource::Telemetry && j.outcome.is_none()
            }) || self.fiber_job_covering(n).is_some());
        by_calls || by_twin
    }

    /// An open fiber job from telemetry on a node sharing fiber with `n`, while `n` is dark: one splicing crew covers every
    /// node a cut took out.
    fn fiber_job_covering(&self, n: usize) -> Option<u64> {
        let dark = self.nodes[n].seen.values().any(|s| *s == Standing::Out);
        if !dark {
            return None;
        }
        self.jobs
            .iter()
            .filter(|j| {
                j.source == JobSource::Telemetry
                    && j.outcome.is_none()
                    && matches!(j.target, JobTarget::Fiber { .. })
            })
            .find(|j| {
                self.node_index(&j.service_group).is_some_and(|m| {
                    m == n || !self.prep.cables[m].is_disjoint(&self.prep.cables[n])
                })
            })
            .map(|j| j.id)
    }

    fn node_index(&self, sg: &ServiceGroupId) -> Option<usize> {
        self.prep.facts.iter().position(|f| &f.sg == sg)
    }

    fn open_job_covering(&self, n: usize, home: &ElementId) -> Option<u64> {
        if let Some(job) = self.fiber_job_covering(n) {
            return Some(job);
        }
        let facts = &self.prep.facts[n];
        self.jobs
            .iter()
            .filter(|j| {
                j.service_group == facts.sg
                    && j.outcome.is_none()
                    && j.source == JobSource::Telemetry
            })
            .find(|j| match &j.target {
                JobTarget::Plant { element } => {
                    element == &facts.root || is_under(facts, home, element)
                }
                JobTarget::Supply { .. } | JobTarget::Fiber { .. } => true,
                JobTarget::Home { modem } => modem == home,
            })
            .map(|j| j.id)
    }

    fn new_job(
        &mut self,
        sg: &ServiceGroupId,
        target: JobTarget,
        geo: LonLat,
        source: JobSource,
        crew: Crew,
    ) -> u64 {
        let id = self.id();
        let t = self.t_s;
        self.jobs.push(Job {
            id,
            created_t_s: t,
            service_group: sg.clone(),
            target,
            geo,
            source,
            crew,
            truck: None,
            arrived_t_s: None,
            done_t_s: None,
            outcome: None,
        });
        // A truck sent to a node that one came back from with nothing, the same day, is a repeat roll.
        if self.jobs.iter().any(|j| {
            &j.service_group == sg
                && j.id != id
                && j.outcome == Some(Outcome::NoTroubleFound)
                && j.done_t_s.is_some_and(|d| t - d < 86_400)
        }) {
            self.counters.repeat_rolls += 1;
        }
        if let Some(n) = self.node_index(sg) {
            for tr in &mut self.troubles {
                let here = tr.node == Some(n) || tr.node.is_none();
                if !tr.fixed && here && tr.first_dispatch.is_none() {
                    tr.first_dispatch = Some(t);
                    self.counters.onset_to_dispatch.0 += ((t - tr.began_s) / 60) as u64;
                    self.counters.onset_to_dispatch.1 += 1;
                }
            }
        }
        let crew_word = match crew {
            Crew::Service => "a service tech",
            Crew::Line => "a line crew",
            Crew::Splicer => "a splicing crew",
        };
        let why = match source {
            JobSource::Ticket(_) => "for a ticket",
            JobSource::Telemetry => "from the twin's reading",
            JobSource::Escalation(_) => "after a truck found nothing",
        };
        self.say(
            format!("Job for {crew_word} on {sg}, {why}"),
            Some(geo),
            false,
        );
        id
    }

    /// The next place to look once `from` was looked at and found clean: up the tree to the node, then each supply's run,
    /// then the fiber. `None` when every rung has been tried.
    fn ladder(
        &self,
        n: usize,
        from: &JobTarget,
        tried: &dyn Fn(&JobTarget) -> bool,
    ) -> Option<(JobTarget, Crew, LonLat)> {
        let facts = &self.prep.facts[n];
        let mut rungs: Vec<JobTarget> = Vec::new();
        if let JobTarget::Plant { element } = from {
            let mut at = element.clone();
            while let Some((up, _)) = facts.parent.get(&at) {
                rungs.push(JobTarget::Plant {
                    element: up.clone(),
                });
                at = up.clone();
            }
        }
        if matches!(from, JobTarget::Plant { .. }) {
            rungs.extend(facts.supplies.iter().map(|s| JobTarget::Supply {
                supply: s.0.clone(),
            }));
        }
        if matches!(from, JobTarget::Plant { .. } | JobTarget::Supply { .. }) {
            rungs.push(JobTarget::Fiber {
                service_group: facts.sg.clone(),
            });
        }
        let rung = rungs.into_iter().find(|r| r != from && !tried(r))?;
        Some(self.dispatch_for(n, rung))
    }

    fn dispatch_for(&self, n: usize, target: JobTarget) -> (JobTarget, Crew, LonLat) {
        let facts = &self.prep.facts[n];
        match &target {
            JobTarget::Plant { element } => {
                let geo = facts.geo.get(element).copied().unwrap_or(facts.root_geo);
                (target, Crew::Line, geo)
            }
            JobTarget::Supply { supply } => {
                let geo = facts
                    .supplies
                    .iter()
                    .find(|s| &s.0 == supply)
                    .map_or(facts.root_geo, |s| s.1);
                (target, Crew::Line, geo)
            }
            JobTarget::Fiber { .. } => (target, Crew::Splicer, facts.root_geo),
            JobTarget::Home { modem } => {
                let geo = self.households.get(modem).map_or(facts.root_geo, |h| h.geo);
                (target, Crew::Service, geo)
            }
        }
    }

    /// Whether a job to `target` on this node found nothing in the last day.
    fn clean_lately(&self, sg: &ServiceGroupId, target: &JobTarget) -> bool {
        let t = self.t_s;
        self.jobs.iter().any(|j| {
            &j.service_group == sg
                && &j.target == target
                && j.outcome == Some(Outcome::NoTroubleFound)
                && j.done_t_s.is_some_and(|d| t - d < 86_400)
        })
    }

    /// Nodes wholly dark at once that share a fiber cable: the twin reads that as the fiber, across the region, before any
    /// one node's reading sends a truck to a node.
    fn dark_together(&self) -> BTreeSet<usize> {
        if self.inputs.fiber.is_none() {
            return BTreeSet::new();
        }
        let dark: Vec<usize> = (0..self.nodes.len())
            .filter(|&n| {
                let st = &self.nodes[n].seen;
                !st.is_empty() && st.values().all(|s| *s == Standing::Out)
            })
            .collect();
        let cables = |n: usize| &self.prep.cables[n];
        let mut out = BTreeSet::new();
        for (i, a) in dark.iter().enumerate() {
            for b in &dark[i + 1..] {
                if !cables(*a).is_disjoint(cables(*b)) {
                    out.insert(*a);
                    out.insert(*b);
                }
            }
        }
        out
    }

    /// The twin's readings, each acted on once its delay is up: a break it can name gets a truck before most people call.
    fn twin_reads(&mut self) {
        let t = self.t_s;
        let (ready, later): (Vec<_>, Vec<_>) = self.readings.drain(..).partition(|r| r.0 <= t);
        self.readings = later;
        let together = self.dark_together();
        for (_, n, reading) in ready {
            if reading.vanished < READING_MODEMS {
                continue;
            }
            // Acted on only while the node is still out, and not while it comes back from a fix: modems take minutes to
            // re-register (§7.4).
            let out_now = self.nodes[n]
                .seen
                .values()
                .filter(|s| **s == Standing::Out)
                .count();
            let sg = self.prep.facts[n].sg.clone();
            let just_fixed = self.jobs.iter().any(|j| {
                j.service_group == sg
                    && matches!(j.outcome, Some(Outcome::Fixed { .. }))
                    && j.done_t_s.is_some_and(|d| t - d < 1800)
            });
            if out_now < READING_MODEMS || just_fixed {
                continue;
            }
            let facts = self.prep.facts[n].clone();
            if self.jobs.iter().any(|j| {
                j.service_group == facts.sg
                    && j.outcome.is_none()
                    && j.source == JobSource::Telemetry
            }) {
                continue;
            }
            // Dark with others on the same fiber: one splicing crew for all of them, if none is on it yet. Not where splicers found
            // this node's fiber clean today: then something else darkened them all (house power, or a break at each), and the
            // node's own reading goes up its ladder below. Without this, splicers went back to a clean fiber all night (task 29).
            let fiber_clean = self.clean_lately(
                &facts.sg,
                &JobTarget::Fiber {
                    service_group: facts.sg.clone(),
                },
            );
            if together.contains(&n) && !fiber_clean {
                let on_it = self.jobs.iter().any(|j| {
                    matches!(j.target, JobTarget::Fiber { .. })
                        && j.outcome.is_none()
                        && self
                            .node_index(&j.service_group)
                            .is_some_and(|m| together.contains(&m))
                });
                if !on_it {
                    self.new_job(
                        &facts.sg,
                        JobTarget::Fiber {
                            service_group: facts.sg.clone(),
                        },
                        facts.root_geo,
                        JobSource::Telemetry,
                        Crew::Splicer,
                    );
                }
                continue;
            }
            let named = match &reading.verdict {
                OutageVerdict::PlantBreak { element, lost, .. } => {
                    if element == &facts.root
                        && matches!(
                            lost,
                            LostWay::ForwardOnly | LostWay::ReturnOnly | LostWay::Both
                        )
                    {
                        JobTarget::Fiber {
                            service_group: facts.sg.clone(),
                        }
                    } else {
                        JobTarget::Plant {
                            element: element.clone(),
                        }
                    }
                }
                OutageVerdict::SupplyRun { supply } => JobTarget::Supply {
                    supply: supply.clone(),
                },
                _ => continue,
            };
            // Where a truck it sent found nothing, the next goes to the next rung; with every rung tried, it waits.
            let sg = facts.sg.clone();
            let pick = if self.clean_lately(&sg, &named) {
                self.ladder(n, &named, &|r| self.clean_lately(&sg, r))
            } else {
                Some(self.dispatch_for(n, named))
            };
            let Some((target, crew, geo)) = pick else {
                continue;
            };
            self.new_job(&facts.sg, target, geo, JobSource::Telemetry, crew);
        }
    }

    /// Dispatch from calls alone: once a node's ticket has piled up calls and its first truck found nothing that fixed it,
    /// a line crew goes to what the calling homes share, and up from there.
    fn reactive_escalation(&mut self) {
        let after = rate(&self.prep.rates, "escalate_after_tickets").max(1.0) as usize;
        let mut sends = Vec::new();
        for k in &self.tickets {
            if k.status == TicketStatus::Closed || k.calls.len() < after {
                continue;
            }
            let busy = k
                .jobs
                .iter()
                .any(|j| self.jobs.iter().any(|x| x.id == *j && x.outcome.is_none()));
            if busy {
                continue;
            }
            let Some(n) = self.node_index(&k.service_group) else {
                continue;
            };
            // With the twin working this node from telemetry, calls ride along on its jobs.
            if self.policy == Policy::Twin
                && (self.fiber_job_covering(n).is_some()
                    || self.jobs.iter().any(|j| {
                        j.service_group == k.service_group
                            && j.source == JobSource::Telemetry
                            && (j.outcome.is_none()
                                || j.done_t_s.is_some_and(|d| self.t_s - d < 86_400))
                    }))
            {
                continue;
            }
            let facts = &self.prep.facts[n];
            let homes: Vec<ElementId> = k
                .calls
                .iter()
                .filter_map(|c| {
                    self.calls
                        .iter()
                        .find(|x| x.id == *c)
                        .map(|x| x.home.clone())
                })
                .collect();
            let sg = facts.sg.clone();
            let next = match self.escalated.get(&k.id) {
                None => {
                    let shared = common_ancestor(facts, &homes);
                    Some(self.dispatch_for(n, JobTarget::Plant { element: shared }))
                }
                Some(last) => self.ladder(n, last, &|r| self.clean_lately(&sg, r)),
            };
            if let Some((target, crew, geo)) = next {
                sends.push((k.id, target, crew, geo, n));
            }
        }
        for (ticket, target, crew, geo, n) in sends {
            self.escalated.insert(ticket, target.clone());
            let sg = self.prep.facts[n].sg.clone();
            let job = self.new_job(&sg, target, geo, JobSource::Escalation(ticket), crew);
            if let Some(k) = self.tickets.iter_mut().find(|k| k.id == ticket) {
                k.jobs.push(job);
                k.status = TicketStatus::Dispatched;
            }
        }
    }

    fn field(&mut self) -> Result<(), WorldError> {
        let t = self.t_s;
        let h = hour_of(t);
        let day = (7..19).contains(&h);
        // Shifts: every truck by day; at night one of each crew on call, and splicers always.
        let mut seen: BTreeMap<Crew, usize> = BTreeMap::new();
        for truck in &mut self.trucks {
            let k = seen.entry(truck.crew).or_insert(0);
            truck.on_shift = day || *k == 0 || truck.crew == Crew::Splicer;
            *k += 1;
        }
        // Arrivals and finished work.
        for i in 0..self.trucks.len() {
            match self.trucks[i].state.clone() {
                TruckState::Driving {
                    job,
                    path,
                    arrive_t_s,
                    ..
                } if arrive_t_s <= t => {
                    self.trucks[i].geo = *path.last().unwrap_or(&self.trucks[i].geo);
                    let work = self.arrive(job);
                    self.trucks[i].state = TruckState::Working {
                        job,
                        until_t_s: t + work,
                    };
                }
                TruckState::Working { job, until_t_s } if until_t_s <= t => {
                    self.finish(job)?;
                    self.trucks[i].state = TruckState::Idle;
                    let hub = self.inputs.hub;
                    if dist_ft(self.trucks[i].geo, hub) > 50.0 {
                        let (path, len) = route(&self.prep.streets, self.trucks[i].geo, hub);
                        let back = drive_s(len, rate(&self.prep.rates, "drive_mph"));
                        self.trucks[i].state = TruckState::Returning {
                            path,
                            depart_t_s: t,
                            arrive_t_s: t + back,
                        };
                        self.log_trip(i);
                    }
                }
                TruckState::Returning {
                    path, arrive_t_s, ..
                } if arrive_t_s <= t => {
                    self.trucks[i].geo = *path.last().unwrap_or(&self.trucks[i].geo);
                    self.trucks[i].state = TruckState::Idle;
                }
                _ => {}
            }
        }
        // A job whose node has no home out any more is called off, waiting or on the way; a truck on the way turns back.
        let back: Vec<u64> = self
            .jobs
            .iter()
            .filter(|j| j.outcome.is_none() && j.arrived_t_s.is_none() && t - j.created_t_s >= 600)
            .filter(|j| {
                self.node_index(&j.service_group)
                    .is_some_and(|n| !self.nodes[n].seen.values().any(|s| *s == Standing::Out))
            })
            .map(|j| j.id)
            .collect();
        for job_id in back {
            if let Some(j) = self.jobs.iter_mut().find(|j| j.id == job_id) {
                j.outcome = Some(Outcome::Cancelled);
                j.done_t_s = Some(t);
            }
            for i in 0..self.trucks.len() {
                if matches!(self.trucks[i].state, TruckState::Driving { job, .. } if job == job_id)
                {
                    let here = self.truck_geo(&self.trucks[i], t);
                    let (path, len) = route(&self.prep.streets, here, self.inputs.hub);
                    let back_s = drive_s(len, rate(&self.prep.rates, "drive_mph"));
                    self.trucks[i].geo = here;
                    self.trucks[i].state = TruckState::Returning {
                        path,
                        depart_t_s: t,
                        arrive_t_s: t + back_s,
                    };
                    self.log_trip(i);
                }
            }
        }
        // Waiting jobs to free trucks of their crew, nearest first; a returning truck can be turned round.
        let mut waiting: Vec<u64> = self
            .jobs
            .iter()
            .filter(|j| j.truck.is_none() && j.outcome.is_none())
            .map(|j| j.id)
            .collect();
        waiting.sort_unstable();
        for job_id in waiting {
            let Some(job) = self.jobs.iter().find(|j| j.id == job_id).cloned() else {
                continue;
            };
            let free = self
                .trucks
                .iter()
                .enumerate()
                .filter(|(_, k)| k.crew == job.crew && k.on_shift)
                .filter(|(_, k)| matches!(k.state, TruckState::Idle | TruckState::Returning { .. }))
                .map(|(i, k)| (i, self.truck_geo(k, t)))
                .min_by(|a, b| {
                    dist_ft(a.1, job.geo)
                        .total_cmp(&dist_ft(b.1, job.geo))
                        .then(a.0.cmp(&b.0))
                });
            let Some((i, from)) = free else { continue };
            let (path, len) = route(&self.prep.streets, from, job.geo);
            let arrive = t + drive_s(len, rate(&self.prep.rates, "drive_mph"));
            self.trucks[i].geo = from;
            self.trucks[i].state = TruckState::Driving {
                job: job_id,
                path,
                depart_t_s: t,
                arrive_t_s: arrive,
            };
            self.log_trip(i);
            let truck_id = self.trucks[i].id;
            if let Some(j) = self.jobs.iter_mut().find(|j| j.id == job_id) {
                j.truck = Some(truck_id);
            }
            self.counters.truck_rolls += 1;
            self.say(
                format!("Truck {truck_id} rolls to {}", job.service_group),
                Some(job.geo),
                false,
            );
        }
        Ok(())
    }

    fn log_trip(&mut self, i: usize) {
        let k = &self.trucks[i];
        let (job, path, depart_t_s, arrive_t_s) = match &k.state {
            TruckState::Driving {
                job,
                path,
                depart_t_s,
                arrive_t_s,
            } => (Some(*job), path, *depart_t_s, *arrive_t_s),
            TruckState::Returning {
                path,
                depart_t_s,
                arrive_t_s,
            } => (None, path, *depart_t_s, *arrive_t_s),
            _ => return,
        };
        let trip = Trip {
            truck: k.id,
            job,
            depart_t_s,
            arrive_t_s,
            path: path.clone(),
        };
        self.trips.push(trip);
    }

    fn truck_geo(&self, k: &Truck, t: i64) -> LonLat {
        match &k.state {
            TruckState::Driving {
                path,
                depart_t_s,
                arrive_t_s,
                ..
            }
            | TruckState::Returning {
                path,
                depart_t_s,
                arrive_t_s,
            } => position(path, *depart_t_s, *arrive_t_s, t),
            _ => k.geo,
        }
    }

    /// On site: what the crew finds within reach, from the plant as it is, and how long the work takes. The one place the
    /// world's truth meets its operators, as it does when a tech opens a housing.
    fn arrive(&mut self, job_id: u64) -> i64 {
        let t = self.t_s;
        let Some(job) = self.jobs.iter().find(|j| j.id == job_id).cloned() else {
            return 0;
        };
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            j.arrived_t_s = Some(t);
        }
        let r = |k: &str| rate(&self.prep.rates, k);
        let found = self.within_reach(&job);
        let minutes = match &found {
            Some(Found::Fault(_, kind, installation)) => match kind.as_ref() {
                FaultKind::SpanCut { .. }
                | FaultKind::PowerShort { .. }
                | FaultKind::WaterIngress { .. } => {
                    if *installation == Some(Installation::Aerial) {
                        r("repair_aerial_min")
                    } else {
                        r("repair_cable_min")
                    }
                }
                FaultKind::AmpDrift { .. } | FaultKind::AnalogNodeFailure { .. } => {
                    r("repair_active_min")
                }
                _ => r("repair_quick_min"),
            },
            Some(Found::Fiber(_)) => r("repair_fiber_min"),
            Some(Found::PowerCompany) | None => r("no_trouble_found_min"),
        };
        // A fault takes the same time to fix whoever finds it, so the two worlds differ in dispatch, not in luck.
        let key = match &found {
            Some(Found::Fault(id, _, _)) | Some(Found::Fiber(id)) => self
                .troubles
                .iter()
                .find(|tr| tr.id == *id)
                .map_or(job_id | (1 << 62), |tr| tr.seq),
            _ => job_id | (1 << 62),
        };
        let mut rng = Rng::derive(self.inputs.seed, &[key, 17]);
        (minutes * rng.range(0.7, 1.3) * 60.0) as i64
    }

    fn within_reach(&self, job: &Job) -> Option<Found> {
        let n = self.node_index(&job.service_group)?;
        let facts = &self.prep.facts[n];
        let (elements, spans): (BTreeSet<ElementId>, BTreeSet<EdgeId>) = match &job.target {
            JobTarget::Home { modem } => {
                let m = facts.modems.iter().find(|m| &m.id == modem)?;
                let mut el: BTreeSet<ElementId> = m.premises.iter().cloned().collect();
                el.insert(m.id.clone());
                el.extend(m.tap.clone());
                let sp: BTreeSet<EdgeId> =
                    m.drop.iter().chain(m.tap_input.iter()).cloned().collect();
                (el, sp)
            }
            JobTarget::Plant { element } => {
                let mut sp: BTreeSet<EdgeId> = facts
                    .child_spans
                    .get(element)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect();
                sp.extend(facts.parent.get(element).map(|(_, e)| e.clone()));
                ([element.clone()].into(), sp)
            }
            JobTarget::Supply { supply } => {
                let run = facts
                    .supplies
                    .iter()
                    .find(|s| &s.0 == supply)
                    .map(|s| s.2.clone())
                    .unwrap_or_default();
                (BTreeSet::new(), run)
            }
            JobTarget::Fiber { .. } => {
                return self
                    .troubles
                    .iter()
                    .find(|tr| !tr.fixed && tr.fiber.is_some() && self.cuts_node(tr, &facts.sg))
                    .map(|tr| Found::Fiber(tr.id));
            }
        };
        for tr in self
            .troubles
            .iter()
            .filter(|tr| !tr.fixed && tr.node == Some(n))
        {
            let Some(fault) = &tr.fault else { continue };
            let hit = match place_of(&fault.kind) {
                Place::Span(s) => {
                    let shorted_on_run = matches!(job.target, JobTarget::Supply { .. })
                        && matches!(
                            fault.kind,
                            FaultKind::PowerShort { .. }
                                | FaultKind::SpanCut {
                                    end: CutEnd::Shorted,
                                    ..
                                }
                        );
                    spans.contains(&s)
                        && (shorted_on_run || !matches!(job.target, JobTarget::Supply { .. }))
                }
                Place::Element(e) | Place::Modem(e) => elements.contains(&e),
                _ => false,
            };
            if hit {
                let install = match place_of(&fault.kind) {
                    Place::Span(s) => facts.span_install.get(&s).copied(),
                    _ => None,
                };
                return Some(Found::Fault(tr.id, Box::new(fault.kind.clone()), install));
            }
        }
        // A dark home with no house power, or a node with no light: nothing here to fix.
        if let JobTarget::Home { modem } = &job.target
            && self.nodes[n].standing.get(modem) == Some(&Standing::Off)
        {
            return Some(Found::PowerCompany);
        }
        None
    }

    fn cuts_node(&self, tr: &Trouble, sg: &ServiceGroupId) -> bool {
        tr.fiber.is_some() && self.links.get(sg) != self.healthy.get(sg)
    }

    fn finish(&mut self, job_id: u64) -> Result<(), WorldError> {
        let t = self.t_s;
        let Some(job) = self.jobs.iter().find(|j| j.id == job_id).cloned() else {
            return Ok(());
        };
        let n = self.node_index(&job.service_group);
        let found = self.within_reach(&job);
        let mut fixed = None;
        let outcome = match found {
            Some(Found::Fault(id, _, _)) | Some(Found::Fiber(id)) => {
                fixed = Some(id);
                let label = self
                    .troubles
                    .iter()
                    .find(|tr| tr.id == id)
                    .map(|tr| tr.label.clone())
                    .unwrap_or_default();
                Outcome::Fixed { what: label }
            }
            Some(Found::PowerCompany) => Outcome::PowerCompany,
            None => Outcome::NoTroubleFound,
        };
        if let Some(id) = fixed {
            let mut fiber = false;
            if let Some(tr) = self.troubles.iter_mut().find(|tr| tr.id == id) {
                tr.fixed = true;
                fiber = tr.fiber.is_some();
                self.counters.fixes += 1;
                self.counters.onset_to_fix.0 += ((t - tr.began_s) / 60) as u64;
                self.counters.onset_to_fix.1 += 1;
            }
            if fiber {
                self.relink();
            }
            if let Some(n) = n {
                self.nodes[n].dirty = true;
            }
        }
        let text = match &outcome {
            Outcome::Fixed { what } => format!("Fixed on {}: {what}", job.service_group),
            Outcome::NoTroubleFound => format!("No trouble found on {}", job.service_group),
            Outcome::PowerCompany => format!(
                "No house power on {}: the power company's",
                job.service_group
            ),
            Outcome::Cancelled => {
                format!("Called off on {}: the homes are back", job.service_group)
            }
        };
        self.say(text, Some(job.geo), false);
        if outcome == Outcome::NoTroubleFound {
            self.counters.no_trouble_found += 1;
        }
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            j.done_t_s = Some(t);
            j.outcome = Some(outcome.clone());
        }
        // A ticket whose job found nothing is open again, for the next truck.
        if !matches!(outcome, Outcome::Fixed { .. }) {
            for k in &mut self.tickets {
                if k.jobs.contains(&job_id) && k.status != TicketStatus::Closed {
                    k.status = TicketStatus::Open;
                }
            }
        }
        Ok(())
    }

    /// A ticket closes when telemetry shows every calling home back.
    fn close_tickets(&mut self) {
        let t = self.t_s;
        let mut closed = Vec::new();
        for k in &mut self.tickets {
            if k.status == TicketStatus::Closed {
                continue;
            }
            let back = k.calls.iter().all(|c| {
                self.calls.iter().find(|x| x.id == *c).is_none_or(|x| {
                    let node = self.households.get(&x.home).map(|h| h.node);
                    node.and_then(|n| self.nodes[n].seen.get(&x.home))
                        .is_none_or(|s| *s == Standing::Up)
                })
            });
            if back {
                k.status = TicketStatus::Closed;
                k.closed_t_s = Some(t);
                closed.push(k.service_group.clone());
            }
        }
        for sg in closed {
            self.say(
                format!("Ticket on {sg} closed: telemetry shows the homes back"),
                None,
                false,
            );
        }
    }
}

enum Found {
    Fault(u64, Box<FaultKind>, Option<Installation>),
    Fiber(u64),
    PowerCompany,
}

enum Place {
    Span(EdgeId),
    Element(ElementId),
    Modem(ElementId),
    Area(LonLat),
    None,
}

fn place_of(kind: &FaultKind) -> Place {
    match kind {
        FaultKind::SpanCut { span_id, .. }
        | FaultKind::PowerShort { span_id, .. }
        | FaultKind::WaterIngress { span_id, .. } => Place::Span(span_id.clone()),
        FaultKind::ConnectorReflection { location, .. } => match location {
            Location::Span { span_id, .. } => Place::Span(span_id.clone()),
            Location::Port { element_id, .. } => Place::Element(element_id.clone()),
        },
        FaultKind::AmpDrift { amp_id, .. } => Place::Element(amp_id.clone()),
        FaultKind::WaterInDevice { element_id, .. }
        | FaultKind::DamagedDevice { element_id, .. } => Place::Element(element_id.clone()),
        FaultKind::AnalogNodeFailure { node_id, .. } => Place::Element(node_id.clone()),
        FaultKind::BadDrop { modem_id, .. } => Place::Modem(modem_id.clone()),
        FaultKind::UtilityOutage { center, .. } => Place::Area(*center),
        _ => Place::None,
    }
}

fn standing_of(m: &ModemObservation) -> Standing {
    if m.reg_state != RegState::Operational {
        return Standing::Out;
    }
    let down = m
        .ds_channel_state
        .iter()
        .chain(&m.us_channel_state)
        .any(|c| c.state == ChannelUsability::Unusable);
    if down { Standing::Slow } else { Standing::Up }
}

fn is_under(facts: &NodeFacts, modem: &ElementId, element: &ElementId) -> bool {
    let mut at = modem.clone();
    while let Some((up, _)) = facts.parent.get(&at) {
        if up == element {
            return true;
        }
        at = up.clone();
    }
    false
}

fn common_ancestor(facts: &NodeFacts, homes: &[ElementId]) -> ElementId {
    let chain = |m: &ElementId| {
        let mut out = vec![m.clone()];
        let mut at = m.clone();
        while let Some((up, _)) = facts.parent.get(&at) {
            out.push(up.clone());
            at = up.clone();
        }
        out
    };
    let Some(first) = homes.first() else {
        return facts.root.clone();
    };
    let mut shared = chain(first);
    for h in &homes[1..] {
        let c: BTreeSet<ElementId> = chain(h).into_iter().collect();
        shared.retain(|e| c.contains(e));
    }
    // The lowest shared point that's plant, not a home.
    shared
        .into_iter()
        .find(|e| {
            facts
                .modems
                .iter()
                .all(|m| &m.id != e && !m.premises.contains(e))
        })
        .unwrap_or_else(|| facts.root.clone())
}

fn by_length<'a>(spans: &'a [(EdgeId, f64)], rng: &mut Rng) -> Option<(&'a EdgeId, f64)> {
    let total: f64 = spans.iter().map(|s| s.1).sum();
    if total <= 0.0 {
        return None;
    }
    let mut at = rng.uniform() * total;
    for (s, l) in spans {
        at -= l;
        if at < 0.0 {
            return Some((s, *l));
        }
    }
    spans.last().map(|(s, l)| (s, *l))
}

fn capital(text: &str) -> String {
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn prepare(inputs: &WorldInputs) -> Result<Prepared, WorldError> {
    let mut rates = BTreeMap::new();
    for (k, v) in &inputs.rates.rates {
        rates.insert(k.clone(), v.value);
    }
    for key in REQUIRED {
        if !rates.contains_key(key) {
            return Err(WorldError::Rate(key.into()));
        }
    }
    let facts: Vec<NodeFacts> = inputs
        .nodes
        .iter()
        .map(|n| facts_of(n, &inputs.library))
        .collect::<Result<_, _>>()?;
    let homes: usize = facts.iter().map(|f| f.modems.len()).sum();
    // How many homes an outage takes, on average, at the radii drawn: sampled once over the region's own homes.
    let geos: Vec<LonLat> = facts
        .iter()
        .flat_map(|f| f.modems.iter().map(|m| m.geo))
        .collect();
    let mut rng = Rng::derive(inputs.seed, &[404]);
    let mut total = 0.0;
    let samples = 40;
    for _ in 0..samples {
        if geos.is_empty() {
            break;
        }
        let c = geos[rng.index(geos.len())];
        let r = rng.range(100.0, 800.0);
        total += geos.iter().filter(|g| dist_ft(c, **g) <= r).count() as f64;
    }
    let homes_per_outage = (total / samples as f64).max(1.0);
    let fiber_miles = inputs.fiber.as_ref().map_or(0.0, |f| {
        f.cables.iter().map(|c| c.len_ft).sum::<f64>() / FT_PER_MILE
    });
    let streets = StreetGraph::build(&inputs.streets, &inputs.homes, 400.0);
    // The fiber cables each node's links run through, for reading nodes dark together.
    let cables = facts
        .iter()
        .map(|f| {
            inputs.fiber.as_ref().map_or_else(BTreeSet::new, |fiber| {
                fiber
                    .links
                    .iter()
                    .filter(|l| l.service_group == f.sg)
                    .flat_map(|l| l.hops.iter())
                    .filter_map(|h| match h {
                        schema::fiber::Hop::Fiber { cable, .. } => Some(cable.to_string()),
                        _ => None,
                    })
                    .collect()
            })
        })
        .collect();
    Ok(Prepared {
        streets,
        cables,
        facts,
        rates,
        homes_per_outage,
        homes,
        fiber_miles,
    })
}

/// Two worlds on one seed, one log: dispatch from calls, and dispatch with the twin.
#[derive(Debug, Clone)]
pub struct Pair {
    /// From calls.
    pub reactive: World,
    /// With the twin.
    pub twin: World,
}

impl Pair {
    /// Both worlds at time 0.
    pub fn new(inputs: WorldInputs) -> Result<Pair, WorldError> {
        let inputs = Arc::new(inputs);
        let prep = Arc::new(prepare(&inputs)?);
        Ok(Pair {
            reactive: World::with(Arc::clone(&inputs), Arc::clone(&prep), Policy::Reactive)?,
            twin: World::with(inputs, prep, Policy::Twin)?,
        })
    }

    /// An action, in both.
    pub fn act(&mut self, action: Action) {
        self.reactive.act(action.clone());
        self.twin.act(action);
    }

    /// An action at world time `at_s`, in both.
    pub fn act_at(&mut self, at_s: i64, action: Action) {
        self.reactive.act_at(at_s, action.clone());
        self.twin.act_at(at_s, action);
    }

    /// Both, to `to_s`, each on its own thread. They share only what neither writes (the inputs and what's prepared from
    /// them), so this is the same fold as stepping one and then the other.
    pub fn advance(&mut self, to_s: i64) -> Result<(), WorldError> {
        let (reactive, twin) = (&mut self.reactive, &mut self.twin);
        let (a, b) = std::thread::scope(|scope| {
            let other = scope.spawn(move || reactive.advance(to_s));
            let here = twin.advance(to_s);
            let there = other
                .join()
                .unwrap_or_else(|_| Err(WorldError::Plant("a world's thread panicked".into())));
            (there, here)
        });
        a.and(b)
    }

    /// Both, to `to_s`, one after the other on this thread: what `advance` must match.
    pub fn advance_in_turn(&mut self, to_s: i64) -> Result<(), WorldError> {
        self.reactive.advance(to_s)?;
        self.twin.advance(to_s)
    }
}

impl World {
    /// The calls, tickets, jobs, trucks and counts, as `schema::ops` has them.
    pub fn records(&self) -> OpsRecords {
        OpsRecords {
            schema_version: schema::ops::SCHEMA_VERSION.to_string(),
            origin: Origin::Synthetic,
            calls: self.calls.clone(),
            tickets: self.tickets.clone(),
            jobs: self.jobs.clone(),
            trucks: self.trucks.clone(),
            counters: self.counters.clone(),
        }
    }

    /// The counts.
    pub fn counters(&self) -> &Counters {
        &self.counters
    }

    /// The story so far, from `since_s` on.
    pub fn feed_since(&self, since_s: i64) -> Vec<FeedItem> {
        self.feed
            .iter()
            .filter(|f| f.t_s >= since_s)
            .cloned()
            .collect()
    }

    /// Every home that isn't working, with where it is.
    pub fn homes_not_up(&self) -> Vec<(ElementId, LonLat, Standing)> {
        let mut out = Vec::new();
        for (id, h) in &self.households {
            let s = self.nodes[h.node]
                .standing
                .get(id)
                .copied()
                .unwrap_or(Standing::Up);
            if s != Standing::Up {
                out.push((id.clone(), h.geo, s));
            }
        }
        out
    }

    /// Every truck and where it is now.
    pub fn trucks_now(&self) -> Vec<(Truck, LonLat)> {
        self.trucks
            .iter()
            .map(|k| (k.clone(), self.truck_geo(k, self.t_s)))
            .collect()
    }

    /// What's wrong with the plant now: the audience's view, never the dispatcher's.
    pub fn truth(&self) -> Vec<TruthItem> {
        self.troubles
            .iter()
            .filter(|t| !t.fixed)
            .map(|t| {
                let geo = match (t.node, &t.fault) {
                    (Some(n), Some(f)) => self.fault_geo(n, &f.kind),
                    (
                        _,
                        Some(Fault {
                            kind: FaultKind::UtilityOutage { center, .. },
                            ..
                        }),
                    ) => Some(*center),
                    _ => None,
                };
                let radius_ft = match &t.fault {
                    Some(Fault {
                        kind: FaultKind::UtilityOutage { radius_ft, .. },
                        ..
                    }) => Some(*radius_ft),
                    _ => None,
                };
                let kind = match (&t.fault, &t.fiber) {
                    (Some(f), _) => variant_name(&f.kind),
                    (None, Some(f)) => variant_name(&f.kind),
                    _ => "Unknown".to_string(),
                };
                TruthItem {
                    label: t.label.clone(),
                    kind,
                    geo,
                    radius_ft,
                    began_s: t.began_s,
                    service_group: t.node.map(|n| self.prep.facts[n].sg.clone()),
                }
            })
            .collect()
    }

    /// The weather and the hour now.
    pub fn sky(&self) -> Sky {
        let t = self.t_s;
        let env = self.environment(t);
        let minutes = (START_HOUR * 60 + t.div_euclid(60)).rem_euclid(24 * 60);
        let storm_until_s = self
            .storms
            .iter()
            .filter(|(a, b, _, _)| *a <= t && t < *b)
            .map(|(_, b, _, _)| *b)
            .max();
        let heat: Vec<&(i64, i64, f64)> = self
            .heat
            .iter()
            .filter(|(a, b, _)| *a <= t && t < *b)
            .collect();
        Sky {
            hour: minutes as f64 / 60.0,
            day: (t + START_HOUR * 3600).div_euclid(86_400),
            temp_c: env.temp_c,
            wind_m_per_s: env.wind_m_per_s.unwrap_or(0.0),
            rain_mm_per_hr: env.rain_mm_per_hr,
            storm_until_s,
            heat_delta_c: heat.iter().map(|(_, _, d)| *d).sum(),
            heat_until_s: heat.iter().map(|(_, b, _)| *b).max(),
        }
    }

    /// The outage every `SAMPLE_S` since the start.
    pub fn series(&self) -> &[Sample] {
        &self.series
    }

    /// Every drive still going at `since_s` or set off after it.
    pub fn trips_since(&self, since_s: i64) -> Vec<&Trip> {
        self.trips
            .iter()
            .filter(|k| k.arrive_t_s >= since_s || k.depart_t_s >= since_s)
            .collect()
    }

    /// The actions taken so far, with the world time each took effect. Those written ahead and not yet reached aren't here.
    pub fn actions_taken(&self) -> &[(i64, Action)] {
        &self.log[..self.applied]
    }

    /// Every served home: its modem, where it is, and its node.
    pub fn homes(&self) -> Vec<(ElementId, LonLat, ServiceGroupId)> {
        self.households
            .iter()
            .map(|(id, h)| (id.clone(), h.geo, self.prep.facts[h.node].sg.clone()))
            .collect()
    }

    /// What the world was made from.
    pub fn inputs(&self) -> &WorldInputs {
        &self.inputs
    }

    /// Calls waiting for an agent now.
    pub fn calls_waiting(&self) -> usize {
        self.waiting.len()
    }
}

/// A fault kind's name as its schema writes it: the variant's tag.
fn variant_name<T: Serialize>(kind: &T) -> String {
    match serde_json::to_value(kind) {
        Ok(Value::Object(o)) => o.keys().next().cloned().unwrap_or_default(),
        Ok(Value::String(s)) => s,
        _ => "Unknown".to_string(),
    }
}

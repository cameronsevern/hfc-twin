//! The region run day to day (task 26), through the twin's tools: start a pair of worlds on the region designed last, run them,
//! act on them, read them. Task 28 adds what a client needs to show them as a game: the region's layout, the sky, a chart's
//! series, trucks' times so they can move smoothly, and scenarios. Everything they show is `Synthetic` (invariant 11).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use ops::{Action, NodeInput, Pair, Scenario, Scenarios, Standing, World, WorldInputs};
use schema::common::LonLat;
use schema::ops::{JobTarget, OpsRates, TruckState};
use schema::plant::EdgeKind;
use schema::region::MapSource;

use crate::state::{Twin, TwinError};

/// The rates the twin's worlds run on, as `fixtures/ops/rates.json` has them.
const RATES: &str = include_str!("../../../fixtures/ops/rates.json");
/// The scenarios a world can start on, as `fixtures/ops/scenarios.json` has them.
const SCENARIOS: &str = include_str!("../../../fixtures/ops/scenarios.json");

/// A running pair of worlds and what it was started from.
#[derive(Debug, Clone)]
pub struct OpsRun {
    /// The two worlds.
    pub pair: Pair,
    /// The region it runs.
    pub region_id: String,
    /// The map the region was designed on.
    pub map_id: String,
    /// Whether the map's streets are a real place's.
    pub real_geography: bool,
    /// The scenario it started on, if any.
    pub scenario: Option<Scenario>,
    /// Its seed.
    pub seed: u64,
    /// Its hazard intensity.
    pub intensity: f64,
}

/// `ops_start`.
#[derive(Deserialize, JsonSchema)]
pub struct StartArgs {
    /// The region, or the last by id whose map is loaded.
    #[serde(default)]
    pub region_id: Option<String>,
    /// The seed, or the scenario's, or the twin's. Two starts on one seed and one set of actions run the same.
    #[serde(default)]
    pub seed: Option<u64>,
    /// How many times the rates file's hazard rates to run at, so a demo sees weeks of trouble in hours. Default the
    /// scenario's, or 50.
    #[serde(default)]
    pub intensity: Option<f64>,
    /// A scenario from `ops_scenarios`, whose actions are written into both worlds' logs at their times.
    #[serde(default)]
    pub scenario: Option<String>,
}

/// `ops_run`.
#[derive(Deserialize, JsonSchema)]
pub struct RunArgs {
    /// World minutes to run both worlds on, at most a week.
    pub minutes: u32,
}

/// `ops_state`.
#[derive(Deserialize, JsonSchema)]
pub struct StateArgs {
    /// Which world: `reactive` (dispatch from calls), `twin` (dispatch with the twin's reading), or `both`. Default `both`.
    #[serde(default)]
    pub world: Option<String>,
    /// The story and the calls from this world time on, seconds. Default the last hour.
    #[serde(default)]
    pub since_s: Option<i64>,
    /// The chart's samples from this world time on, seconds. Default 0, every sample.
    #[serde(default)]
    pub series_since_s: Option<i64>,
}

/// `ops_act`.
#[derive(Deserialize, JsonSchema)]
pub struct ActArgs {
    /// What happens, in both worlds: `{"Storm": {"hours": 6, "wind_m_per_s": 25, "rain_mm_per_hr": 20}}`,
    /// `{"HeatWave": {"hours": 8, "delta_c": 12}}`, `{"PowerCut": {"center": {...}, "radius_ft": 600, "hours": 3}}`,
    /// `{"DigIn": {"near": null}}`, `{"CarHit": {"near": {...}}}`, or `"FiberCut"`.
    pub action: Value,
    /// The world time it happens, seconds. Default, and at the soonest, the next minute.
    #[serde(default)]
    pub at_s: Option<i64>,
}

fn running(twin: &mut Twin) -> Result<&mut OpsRun, TwinError> {
    twin.ops
        .as_mut()
        .ok_or_else(|| TwinError::Refused("no world is running: call ops_start first".into()))
}

fn scenarios_file() -> Result<Scenarios, TwinError> {
    Ok(serde_json::from_str(SCENARIOS)?)
}

/// Starts a pair of worlds on a designed region.
pub fn start(twin: &mut Twin, a: &StartArgs) -> Result<Value, TwinError> {
    let scenario = match &a.scenario {
        Some(id) => Some(
            scenarios_file()?
                .get(id)
                .cloned()
                .ok_or_else(|| TwinError::Refused(format!("no scenario {id}")))?,
        ),
        None => None,
    };
    // A region runs on its map's streets, so one whose map isn't loaded (a fixture's) can't run. Without an id, the last region
    // by id that can.
    let region = match &a.region_id {
        Some(id) => twin
            .regions
            .get(id)
            .cloned()
            .ok_or_else(|| TwinError::Refused(format!("no region {id}")))?,
        None => twin
            .regions
            .values()
            .rfind(|r| twin.maps.contains_key(&r.map_id))
            .cloned()
            .ok_or_else(|| {
                TwinError::Refused("no region is designed: design an area first".into())
            })?,
    };
    let map = twin
        .maps
        .get(&region.map_id)
        .cloned()
        .ok_or_else(|| TwinError::Refused(format!("no map {}", region.map_id)))?;
    let mut nodes = Vec::new();
    for rn in &region.nodes {
        let node = twin.node(&rn.service_group)?;
        let profile = twin
            .profiles
            .get(&node.plant.profile_ref)
            .cloned()
            .ok_or_else(|| TwinError::Refused(format!("no profile {}", node.plant.profile_ref)))?;
        nodes.push(NodeInput {
            plant: node.plant.clone(),
            profile,
        });
    }
    let fiber = region
        .fiber_ref
        .as_ref()
        .and_then(|f| twin.fiber_plants.get(f))
        .cloned();
    let rates: OpsRates = serde_json::from_str(RATES)?;
    let intensity = a
        .intensity
        .or(scenario.as_ref().map(|s| s.intensity))
        .unwrap_or(50.0)
        .clamp(0.0, 10_000.0);
    let seed = a
        .seed
        .or(scenario.as_ref().map(|s| s.seed))
        .unwrap_or(twin.seed);
    let inputs = WorldInputs {
        library: twin.library.clone(),
        nodes,
        fiber,
        streets: map.streets.clone(),
        homes: map.homes.clone(),
        hub: map.headend,
        rates,
        seed,
        intensity,
    };
    let mut pair = Pair::new(inputs).map_err(|e| TwinError::Refused(e.to_string()))?;
    if let Some(s) = &scenario {
        s.apply(&mut pair);
    }
    let homes = pair.twin.homes().len();
    let summary = json!({
        "origin": "Synthetic",
        "region_id": region.id,
        "map_id": region.map_id,
        "nodes": region.nodes.len(),
        "homes": homes,
        "trucks": pair.twin.records().trucks.len(),
        "seed": seed,
        "intensity": intensity,
        "scenario": scenario.as_ref().map(|s| json!({"id": s.id, "title": s.title, "blurb": s.blurb})),
        "note": "Two worlds on one seed: one dispatches from calls, one with the twin. Hazards run at the rates file's rates times the intensity.",
    });
    twin.ops = Some(OpsRun {
        pair,
        region_id: region.id.clone(),
        map_id: region.map_id.clone(),
        real_geography: matches!(map.source, MapSource::OpenStreetMap { .. }),
        scenario,
        seed,
        intensity,
    });
    Ok(summary)
}

/// Runs both worlds on.
pub fn run(twin: &mut Twin, a: &RunArgs) -> Result<Value, TwinError> {
    let p = &mut running(twin)?.pair;
    let to = p.twin.t_s + i64::from(a.minutes.min(7 * 24 * 60)) * 60;
    p.advance(to)
        .map_err(|e| TwinError::Refused(e.to_string()))?;
    Ok(json!({"t_s": p.twin.t_s, "clock": clock(p.twin.t_s)}))
}

/// An action, in both worlds.
pub fn act(twin: &mut Twin, a: &ActArgs) -> Result<Value, TwinError> {
    let action: Action = serde_json::from_value(a.action.clone())?;
    let p = &mut running(twin)?.pair;
    let soonest = p.twin.t_s + ops::world::STEP_S;
    let at = a.at_s.unwrap_or(soonest).max(soonest);
    p.act_at(at, action);
    Ok(json!({"t_s": p.twin.t_s, "applies_at_s": at}))
}

/// The rates, with where each comes from.
pub fn rates() -> Result<Value, TwinError> {
    Ok(serde_json::from_str(RATES)?)
}

/// The scenarios.
pub fn scenarios() -> Result<Value, TwinError> {
    Ok(serde_json::to_value(scenarios_file()?)?)
}

fn clock(t_s: i64) -> String {
    let minutes = t_s / 60 + ops::world::START_HOUR * 60;
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    format!(
        "{} {:02}:{:02}",
        days[((minutes / 1440) % 7) as usize],
        (minutes / 60) % 24,
        minutes % 60
    )
}

fn point(g: LonLat) -> [f64; 2] {
    [g.lon_deg, g.lat_deg]
}

fn lines(features: Vec<Value>) -> Value {
    json!({"type": "FeatureCollection", "features": features})
}

/// What doesn't change while a pair runs, so any client can draw the region: its homes, hub, streets, plant and fiber.
pub fn layout(twin: &mut Twin) -> Result<Value, TwinError> {
    let run = running(twin)?;
    let w = &run.pair.twin;
    let inputs = w.inputs();
    let homes: Vec<Value> = w
        .homes()
        .into_iter()
        .map(|(id, g, sg)| json!({"home": id, "lon": g.lon_deg, "lat": g.lat_deg, "service_group": sg}))
        .collect();
    let streets: Vec<Value> = inputs
        .streets
        .iter()
        .map(|s| {
            let coordinates: Vec<[f64; 2]> = s.points.iter().map(|p| point(*p)).collect();
            json!({
                "type": "Feature",
                "geometry": {"type": "LineString", "coordinates": coordinates},
                "properties": {"id": s.id, "crossing": s.id.starts_with("crossing-")},
            })
        })
        .collect();
    let mut spans = Vec::new();
    let mut nodes = Vec::new();
    for n in &inputs.nodes {
        let plant = &n.plant;
        if let Some(site) = crate::tools::site_of(plant) {
            nodes.push(json!({"service_group": plant.service_group, "lon": site.lon_deg, "lat": site.lat_deg}));
        }
        let at = |e: &schema::common::ElementId| plant.element(e).and_then(|x| x.geo);
        for edge in &plant.edges {
            let EdgeKind::Span { installation, .. } = &edge.kind else {
                continue;
            };
            let (Some(a), Some(b)) = (at(&edge.from.element_id), at(&edge.to.element_id)) else {
                continue;
            };
            let mut coordinates = vec![point(a)];
            coordinates.extend(edge.kind.path().iter().map(|p| point(p.geo)));
            coordinates.push(point(b));
            spans.push(json!({
                "type": "Feature",
                "geometry": {"type": "LineString", "coordinates": coordinates},
                "properties": {"id": edge.id, "service_group": plant.service_group, "installation": installation},
            }));
        }
    }
    let all = inputs
        .streets
        .iter()
        .flat_map(|s| s.points.iter().copied())
        .chain(inputs.homes.iter().map(|h| h.geo))
        .chain(std::iter::once(inputs.hub));
    let (mut lo, mut hi) = ([f64::MAX, f64::MAX], [f64::MIN, f64::MIN]);
    for g in all {
        lo = [lo[0].min(g.lon_deg), lo[1].min(g.lat_deg)];
        hi = [hi[0].max(g.lon_deg), hi[1].max(g.lat_deg)];
    }
    Ok(json!({
        "origin": "Synthetic",
        "region_id": run.region_id,
        "map_id": run.map_id,
        "real_geography": run.real_geography,
        "seed": run.seed,
        "intensity": run.intensity,
        "scenario": run.scenario.as_ref().map(|s| json!({"id": s.id, "title": s.title, "blurb": s.blurb})),
        "hub": point(inputs.hub),
        "bounds": [lo, hi],
        "homes": homes,
        "nodes": nodes,
        "streets": lines(streets),
        "spans": lines(spans),
        "fiber": inputs.fiber.as_ref().map(crate::tools::fiber_geojson),
    }))
}

fn world_json(w: &World, since_s: i64) -> Value {
    let r = w.records();
    let homes: Vec<Value> = w
        .homes_not_up()
        .into_iter()
        .map(|(id, g, s)| {
            let status = match s {
                Standing::Up => "up",
                Standing::Slow => "slow",
                Standing::Out => "out",
                Standing::Off => "off",
            };
            json!({"home": id, "lon": g.lon_deg, "lat": g.lat_deg, "status": status})
        })
        .collect();
    // A truck's times, so a client can move it smoothly between states: when it left and gets there, or when its work
    // started and ends.
    let trucks: Vec<Value> = w
        .trucks_now()
        .into_iter()
        .map(|(k, g)| {
            let (doing, job, path, from_s, to_s) = match &k.state {
                TruckState::Idle => ("idle", None, None, None, None),
                TruckState::Driving {
                    job,
                    path,
                    depart_t_s,
                    arrive_t_s,
                } => (
                    "driving",
                    Some(*job),
                    Some(path.clone()),
                    Some(*depart_t_s),
                    Some(*arrive_t_s),
                ),
                TruckState::Working { job, until_t_s } => {
                    let began = r
                        .jobs
                        .iter()
                        .find(|j| j.id == *job)
                        .and_then(|j| j.arrived_t_s);
                    ("working", Some(*job), None, began, Some(*until_t_s))
                }
                TruckState::Returning {
                    path,
                    depart_t_s,
                    arrive_t_s,
                } => (
                    "returning",
                    None,
                    Some(path.clone()),
                    Some(*depart_t_s),
                    Some(*arrive_t_s),
                ),
            };
            json!({
                "id": k.id, "crew": k.crew, "lon": g.lon_deg, "lat": g.lat_deg, "doing": doing, "job": job,
                "on_shift": k.on_shift, "from_t_s": from_s, "to_t_s": to_s,
                "path": path.map(|p| p.iter().map(|x| point(*x)).collect::<Vec<_>>()),
            })
        })
        .collect();
    let jobs: Vec<Value> = r
        .jobs
        .iter()
        .filter(|j| j.outcome.is_none() || j.done_t_s.is_some_and(|d| d >= since_s))
        .map(|j| {
            let target = match &j.target {
                JobTarget::Home { modem } => format!("home {modem}"),
                JobTarget::Plant { element } => format!("plant {element}"),
                JobTarget::Supply { supply } => format!("supply {supply}"),
                JobTarget::Fiber { service_group } => format!("fiber to {service_group}"),
            };
            json!({
                "id": j.id, "service_group": j.service_group, "target": target, "crew": j.crew, "source": j.source,
                "lon": j.geo.lon_deg, "lat": j.geo.lat_deg, "truck": j.truck, "created_t_s": j.created_t_s,
                "arrived_t_s": j.arrived_t_s, "done_t_s": j.done_t_s, "outcome": j.outcome,
            })
        })
        .collect();
    let tickets: Vec<Value> = r
        .tickets
        .iter()
        .filter(|k| k.status != schema::ops::TicketStatus::Closed || k.closed_t_s.is_some_and(|c| c >= since_s))
        .map(|k| json!({"id": k.id, "service_group": k.service_group, "opened_t_s": k.opened_t_s, "first_home": k.first_home, "calls": k.calls.len(), "status": k.status, "jobs": k.jobs}))
        .collect();
    let calls: Vec<Value> = r
        .calls
        .iter()
        .filter(|c| c.t_s >= since_s)
        .map(|c| json!({"id": c.id, "t_s": c.t_s, "home": c.home, "service_group": c.service_group, "reason": c.reason, "answered": c.answered_t_s.is_some(), "abandoned": c.abandoned_t_s.is_some(), "ticket": c.ticket}))
        .collect();
    json!({
        "policy": w.policy,
        "homes_not_up": homes,
        "trucks": trucks,
        "jobs": jobs,
        "tickets": tickets,
        "calls": calls,
        "calls_waiting": w.calls_waiting(),
        "feed": w.feed_since(since_s),
        "trips": w.trips_since(since_s).into_iter().map(|k| json!({
            "truck": k.truck, "job": k.job, "depart_t_s": k.depart_t_s, "arrive_t_s": k.arrive_t_s,
            "path": k.path.iter().map(|x| point(*x)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "truth": w.truth(),
        "counters": r.counters,
    })
}

/// The worlds now: each one's homes, trucks, jobs, tickets, calls and story, and what both share.
pub fn state(twin: &mut Twin, a: &StateArgs) -> Result<Value, TwinError> {
    let run = running(twin)?;
    let p = &run.pair;
    let t = p.twin.t_s;
    let since = a.since_s.unwrap_or(t - 3600);
    let series_since = a.series_since_s.unwrap_or(0);
    let mut worlds = serde_json::Map::new();
    let which = a.world.as_deref().unwrap_or("both");
    if which != "twin" {
        worlds.insert("reactive".into(), world_json(&p.reactive, since));
    }
    if which != "reactive" {
        worlds.insert("twin".into(), world_json(&p.twin, since));
    }
    let series = |w: &World| -> Vec<Value> {
        w.series()
            .iter()
            .filter(|s| s.t_s >= series_since)
            .map(|s| json!([s.t_s, s.homes_out, s.trucks_out, s.calls_waiting]))
            .collect()
    };
    let actions: Vec<Value> = p
        .twin
        .actions_taken()
        .iter()
        .map(|(at, action)| json!({"at_s": at, "action": action}))
        .collect();
    Ok(json!({
        "origin": "Synthetic",
        "t_s": t,
        "clock": clock(t),
        "sky": p.twin.sky(),
        "worlds": worlds,
        "counters_reactive": p.reactive.counters(),
        "counters_twin": p.twin.counters(),
        "series_columns": ["t_s", "homes_out", "trucks_out", "calls_waiting"],
        "series_reactive": series(&p.reactive),
        "series_twin": series(&p.twin),
        "actions": actions,
    }))
}

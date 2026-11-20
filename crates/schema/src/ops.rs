//! Running a region day to day (task 26): the rates a world runs on, and what its call centre and field crews keep. A ticket
//! isn't something the plant emits, so none of this is in the observation schema (invariant 2); it's shaped so an operator's
//! real ticketing and dispatch feeds can map into it later. Everything a world writes is `Synthetic` (invariant 11).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common::{ElementId, LonLat, Origin, ServiceGroupId};

/// Version of this schema.
pub const SCHEMA_VERSION: &str = "1";

/// What a rate rests on (invariant 9, for numbers that aren't parts).
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// A published figure: a statistic, a report, a paper.
    Published,
    /// Worked out from published figures, and the working says how.
    Inferred,
    /// Judgement, with nothing published behind it.
    Generic,
}

/// One rate.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    /// Its value.
    pub value: f64,
    /// Its unit, in words, such as `per span per year` or `minutes`.
    pub unit: String,
    /// Where it comes from, if anywhere.
    pub source_url: Option<String>,
    /// What it rests on.
    pub confidence: Confidence,
    /// How it was read or worked out.
    pub note: String,
}

/// The rates a world runs on, by name. `ops::World` refuses a file missing one it reads.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpsRates {
    /// Version of this schema.
    pub schema_version: String,
    /// Stable identifier.
    pub id: String,
    /// Every rate, by name.
    pub rates: BTreeMap<String, Rate>,
}

/// What a caller says. It isn't a diagnosis.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallReason {
    /// Nothing works.
    NoService,
    /// It works, badly: slow, or dropping in and out.
    Slow,
}

/// One call to the call centre.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Call {
    /// Its number in the world.
    pub id: u64,
    /// When it came in, seconds of world time.
    pub t_s: i64,
    /// The home, by its modem's plant location (invariant 10).
    pub home: ElementId,
    /// The node the home is on.
    pub service_group: ServiceGroupId,
    /// What the caller says.
    pub reason: CallReason,
    /// When an agent answered it.
    pub answered_t_s: Option<i64>,
    /// When the caller gave up waiting, if they did.
    pub abandoned_t_s: Option<i64>,
    /// The ticket it opened or joined.
    pub ticket: Option<u64>,
}

/// Where a ticket stands.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketStatus {
    /// Waiting for a truck.
    Open,
    /// A truck has the job.
    Dispatched,
    /// Telemetry says the homes on it are back.
    Closed,
}

/// A trouble ticket: one or more calls about one node.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    /// Its number in the world.
    pub id: u64,
    /// The node.
    pub service_group: ServiceGroupId,
    /// When it opened, seconds of world time.
    pub opened_t_s: i64,
    /// The first caller's home.
    pub first_home: ElementId,
    /// Its calls.
    pub calls: Vec<u64>,
    /// Where it stands.
    pub status: TicketStatus,
    /// The jobs it has sent.
    pub jobs: Vec<u64>,
    /// When it closed.
    pub closed_t_s: Option<i64>,
}

/// Who a job needs.
#[derive(
    Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Crew {
    /// A service tech: homes, drops, taps on the ground.
    Service,
    /// A line crew with a bucket truck: aerial plant, actives, hardline.
    Line,
    /// A fiber splicing crew.
    Splicer,
}

/// Where a job sends a truck.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum JobTarget {
    /// A home, by its modem.
    Home {
        /// The modem.
        modem: ElementId,
    },
    /// A point in the plant.
    Plant {
        /// The element.
        element: ElementId,
    },
    /// A plant supply.
    Supply {
        /// The supply.
        supply: ElementId,
    },
    /// The fiber to a node.
    Fiber {
        /// The node.
        service_group: ServiceGroupId,
    },
}

/// Why a job exists.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobSource {
    /// A ticket, as dispatch works from calls.
    Ticket(u64),
    /// The twin's reading of telemetry, before or without a call.
    Telemetry,
    /// An earlier job that found nothing, sent on.
    Escalation(u64),
}

/// What a job came to.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Found and fixed, with what it was in words.
    Fixed {
        /// What was found.
        what: String,
    },
    /// Nothing wrong within reach.
    NoTroubleFound,
    /// The homes have no house power: the power company's to fix.
    PowerCompany,
    /// Called off before the work: the homes it was for came back.
    Cancelled,
}

/// One job.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Job {
    /// Its number in the world.
    pub id: u64,
    /// When it was made, seconds of world time.
    pub created_t_s: i64,
    /// The node.
    pub service_group: ServiceGroupId,
    /// Where it goes.
    pub target: JobTarget,
    /// Where it stands, on the map.
    pub geo: LonLat,
    /// Why.
    pub source: JobSource,
    /// Who it needs.
    pub crew: Crew,
    /// The truck that has it.
    pub truck: Option<u64>,
    /// When that truck got there.
    pub arrived_t_s: Option<i64>,
    /// When the work ended.
    pub done_t_s: Option<i64>,
    /// What it came to.
    pub outcome: Option<Outcome>,
}

/// What a truck is doing.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum TruckState {
    /// At the hub, free.
    Idle,
    /// On the way to a job, along `path`, leaving and arriving at these times.
    Driving {
        /// The job.
        job: u64,
        /// The route, street by street.
        path: Vec<LonLat>,
        /// When it left.
        depart_t_s: i64,
        /// When it gets there.
        arrive_t_s: i64,
    },
    /// On site.
    Working {
        /// The job.
        job: u64,
        /// When the work ends.
        until_t_s: i64,
    },
    /// Going back to the hub.
    Returning {
        /// The route.
        path: Vec<LonLat>,
        /// When it left.
        depart_t_s: i64,
        /// When it gets there.
        arrive_t_s: i64,
    },
}

/// One truck and its crew.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Truck {
    /// Its number.
    pub id: u64,
    /// Its crew.
    pub crew: Crew,
    /// Where it is when last placed.
    pub geo: LonLat,
    /// What it's doing.
    pub state: TruckState,
    /// Whether its shift is on.
    pub on_shift: bool,
}

/// What a world has counted.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Counters {
    /// Minutes summed over every home with no service, house power aside.
    pub customer_minutes_out: u64,
    /// Homes with no service now.
    pub homes_out_now: u64,
    /// Calls.
    pub calls: u64,
    /// Of those, answered.
    pub calls_answered: u64,
    /// Of those, abandoned.
    pub calls_abandoned: u64,
    /// Of those, told by the phone line that the outage is known, with no agent needed.
    pub calls_told_of_outage: u64,
    /// Tickets opened.
    pub tickets: u64,
    /// Trucks sent.
    pub truck_rolls: u64,
    /// Trucks sent again to a node that a truck had already been sent to for the same trouble.
    pub repeat_rolls: u64,
    /// Jobs that found nothing.
    pub no_trouble_found: u64,
    /// Faults fixed.
    pub fixes: u64,
    /// From a fault's start to its node's first call, minutes, summed, and how many.
    pub onset_to_first_call: (u64, u64),
    /// From a fault's start to the first truck sent to it, minutes, summed, and how many.
    pub onset_to_dispatch: (u64, u64),
    /// From a fault's start to its fix, minutes, summed, and how many.
    pub onset_to_fix: (u64, u64),
}

/// One line of a world's running story, for the page. Lines marked `truth` say what happened to the plant: the audience
/// sees them, and the dispatcher never does.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FeedItem {
    /// When, seconds of world time.
    pub t_s: i64,
    /// What, in words.
    pub text: String,
    /// Where, if anywhere.
    pub geo: Option<LonLat>,
    /// Whether it's the plant's truth rather than anything an operator could know.
    pub truth: bool,
}

/// What a world's call centre and field crews keep, for a reader or a feed.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpsRecords {
    /// Version of this schema.
    pub schema_version: String,
    /// Always `Synthetic` from a world (invariant 11).
    pub origin: Origin,
    /// Calls.
    pub calls: Vec<Call>,
    /// Tickets.
    pub tickets: Vec<Ticket>,
    /// Jobs.
    pub jobs: Vec<Job>,
    /// Trucks.
    pub trucks: Vec<Truck>,
    /// The counts.
    pub counters: Counters,
}

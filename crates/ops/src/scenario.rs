//! Scenarios (task 28): a demo day written ahead as the user's own actions at world times, so a presenter can press one button
//! and talk. A scenario is never scripted physics (invariant 5): each action makes faults through the same draws a button press
//! does. The file is `fixtures/ops/scenarios.json`.

use serde::{Deserialize, Serialize};

use crate::world::{Action, Pair};

/// Version of the scenarios file.
pub const SCHEMA_VERSION: &str = "1";

/// An action at a world time.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Planned {
    /// World time it takes effect, seconds from the start (Monday 06:00).
    pub at_s: i64,
    /// What happens.
    pub action: Action,
}

/// One scenario.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Stable id.
    pub id: String,
    /// Its name on the page.
    pub title: String,
    /// What happens, in a sentence, for the presenter.
    pub blurb: String,
    /// The seed it's meant to run on.
    pub seed: u64,
    /// The hazard intensity it's meant to run at.
    pub intensity: f64,
    /// Its actions, in any order.
    pub actions: Vec<Planned>,
}

impl Scenario {
    /// Writes its actions into both worlds' logs.
    pub fn apply(&self, pair: &mut Pair) {
        for p in &self.actions {
            pair.act_at(p.at_s, p.action.clone());
        }
    }
}

/// The scenarios file.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Scenarios {
    /// Version of this file's shape.
    pub schema_version: String,
    /// Every scenario.
    pub scenarios: Vec<Scenario>,
}

impl Scenarios {
    /// The scenario with this id.
    pub fn get(&self, id: &str) -> Option<&Scenario> {
        self.scenarios.iter().find(|s| s.id == id)
    }
}

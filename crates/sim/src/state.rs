//! §7.4: modem state carried between time steps. The timers and rules are normative [MULPI Annex B,
//! §10.3]; the loss rates are ours (`docs/reference/mac-operations.md`).

use std::collections::{BTreeMap, BTreeSet};

use schema::common::ChannelId;
use schema::observation::RegState;

use crate::rng::Rng;

/// §7.4: consecutive misses that exhaust retries.
pub const RETRIES: u32 = 17;
/// §7.4: T3's period, seconds. At most one expiry per channel per period.
pub const T3_S: f64 = 0.2;
/// §7.4: T4's period, seconds.
pub const T4_S: f64 = 30.0;
/// §7.4: tries at registering: the first REG-REQ and T6's 3 retries [MULPI Annex B].
pub const T6_ATTEMPTS: i32 = 4;

/// §7.4: the chance of exhausting retries in one sequence.
pub fn exhaustion_probability(p_loss: f64) -> f64 {
    p_loss.powi(RETRIES as i32)
}

/// §7.4: the most T3 expiries a step can hold on one channel.
pub fn t3_cap(step_s: f64) -> u64 {
    (step_s / T3_S).round() as u64
}

/// What one modem carries between steps.
#[derive(Debug, Clone, PartialEq)]
pub struct ModemState {
    /// Registration state.
    pub reg_state: RegState,
    /// Cumulative T3 expiries per upstream channel.
    pub t3: BTreeMap<ChannelId, u64>,
    /// Cumulative T4 expiries per upstream channel.
    pub t4: BTreeMap<ChannelId, u64>,
    /// Consecutive misses per upstream channel.
    pub misses: BTreeMap<ChannelId, u32>,
    /// Upstream channels out of retries.
    pub us_unusable: BTreeSet<ChannelId>,
    /// Seconds since each upstream channel last had an opportunity.
    pub since_opportunity_s: BTreeMap<ChannelId, f64>,
    /// Seconds still to go before registering again.
    pub reregister_s: f64,
    /// Time accrued toward the next ranging opportunity, seconds.
    pub ranging_phase_s: f64,
}

impl Default for ModemState {
    fn default() -> Self {
        ModemState {
            reg_state: RegState::Operational,
            t3: BTreeMap::new(),
            t4: BTreeMap::new(),
            misses: BTreeMap::new(),
            us_unusable: BTreeSet::new(),
            since_opportunity_s: BTreeMap::new(),
            reregister_s: 0.0,
            ranging_phase_s: 0.0,
        }
    }
}

/// One step's physical conditions for one modem.
#[derive(Debug, Clone, PartialEq)]
pub struct StepInput {
    /// Seconds in this step.
    pub step_s: f64,
    /// §7.4: the chance a MAC message is lost on the modem's best downstream. 1 means nothing has lock.
    pub p_down: f64,
    /// Per upstream channel, the chance a station-maintenance exchange fails once it has its MAP.
    pub p_loss: BTreeMap<ChannelId, f64>,
    /// `ranging_spacing_s` from the profile.
    pub ranging_spacing_s: f64,
    /// `reregistration_s` from the profile.
    pub reregistration_s: f64,
}

impl ModemState {
    /// Advances one step.
    pub fn advance(&mut self, input: &StepInput, rng: &mut Rng) {
        for channel in input.p_loss.keys() {
            self.t3.entry(channel.clone()).or_insert(0);
            self.t4.entry(channel.clone()).or_insert(0);
        }

        if self.reg_state != RegState::Operational {
            self.register(input, rng);
            return;
        }

        self.ranging_phase_s += input.step_s;
        let trials = (self.ranging_phase_s / input.ranging_spacing_s).floor() as u64;
        self.ranging_phase_s -= trials as f64 * input.ranging_spacing_s;
        let cap = t3_cap(input.step_s);
        let mut expiries: BTreeMap<&ChannelId, u64> = BTreeMap::new();
        for _ in 0..trials {
            for (channel, p_loss) in &input.p_loss {
                // §7.4: the MAP carries the opportunity. Lost, nothing is sent and T4 keeps running.
                let since = self
                    .since_opportunity_s
                    .entry(channel.clone())
                    .or_insert(0.0);
                if rng.uniform() < input.p_down {
                    *since += input.ranging_spacing_s;
                    if *since >= T4_S {
                        *since = 0.0;
                        if let Some(count) = self.t4.get_mut(channel) {
                            *count += 1;
                        }
                        self.us_unusable.insert(channel.clone());
                    }
                    continue;
                }
                *since = 0.0;
                let misses = self.misses.entry(channel.clone()).or_insert(0);
                if rng.uniform() < *p_loss {
                    *expiries.entry(channel).or_insert(0) += 1;
                    *misses += 1;
                    if *misses >= RETRIES {
                        self.us_unusable.insert(channel.clone());
                    }
                } else {
                    *misses = 0;
                    self.us_unusable.remove(channel);
                }
            }
            // §7.4: none left on the primary upstream flow reinitializes the modem.
            if !input.p_loss.is_empty() && input.p_loss.keys().all(|c| self.us_unusable.contains(c))
            {
                self.reinitialize(input);
                break;
            }
        }
        for (channel, count) in expiries {
            if let Some(total) = self.t3.get_mut(channel) {
                *total += count.min(cap);
            }
        }
    }

    /// §7.4: coming back. Nothing happens without lock, and an attempt can fail on T6.
    fn register(&mut self, input: &StepInput, rng: &mut Rng) {
        if input.p_down >= 1.0 {
            self.reinitialize(input);
            return;
        }
        self.reregister_s -= input.step_s;
        self.reg_state = progress(self.reregister_s, input.reregistration_s);
        // §16: registering needs ranging, so with every upstream lost (a return link down) it gets no further.
        if !input.p_loss.is_empty()
            && input.p_loss.values().all(|p| *p >= 1.0)
            && self.reg_state != RegState::NotSynchronized
            && self.reg_state != RegState::PhySynchronized
        {
            self.reg_state = RegState::RangingInProgress;
            return;
        }
        if self.reg_state != RegState::Operational {
            return;
        }
        if rng.uniform() < input.p_down.powi(T6_ATTEMPTS) {
            self.reinitialize(input);
            return;
        }
        self.us_unusable.clear();
        self.misses.clear();
        self.since_opportunity_s.clear();
    }

    fn reinitialize(&mut self, input: &StepInput) {
        self.reg_state = RegState::NotSynchronized;
        self.reregister_s = input.reregistration_s;
    }
}

/// How far through registering a modem is, by the share of `reregistration_s` left.
fn progress(left_s: f64, total_s: f64) -> RegState {
    if left_s <= 0.0 {
        return RegState::Operational;
    }
    let share = left_s / total_s.max(1e-9);
    if share > 0.75 {
        RegState::PhySynchronized
    } else if share > 0.5 {
        RegState::RangingInProgress
    } else if share > 0.25 {
        RegState::RangingComplete
    } else {
        RegState::RegistrationInProgress
    }
}

//! An RPD's state from its faults, time and the recovery defaults (`docs/reference/daa-remote-phy.md`). RF on or off is derived here,
//! and modems fall through §7.4 on their own, so no fault here says anything about modem telemetry (invariant 5).

use schema::common::{ElementId, Origin};
use schema::environment::EnvironmentSample;
use schema::fault::{Fault, FaultKind, Onset, RpdFailureMode};
use schema::rpd::{GcpStatus, PtpResult, RpdObservation, RpdState, SCHEMA_VERSION};

/// How long retries take before GCP gives up and soft-resets, seconds. `GcpRecoveryActionRetry` defaults to 3 [R-PHY §7.1.3.3]. The
/// time for each isn't sourced, so 60 s apiece is ours (approx).
pub const GCP_GRACE_S: f64 = 180.0;
/// How long holdover stays in spec, seconds. Holdover limits are in R-DTI, which isn't in our set (approx).
pub const HOLDOVER_IN_SPEC_S: f64 = 300.0;
/// How long out-of-spec holdover runs before a Core holds RF off, seconds (approx: sources don't say).
pub const HOLDOVER_OUT_OF_SPEC_S: f64 = 300.0;

/// Seconds since a fault began at this sample, if it has.
fn elapsed_s(fault: &Fault, sample: &EnvironmentSample) -> Option<f64> {
    let start = match &fault.onset {
        Onset::Instant => 0,
        Onset::Ramp { start, .. } => start.0,
    };
    (sample.t.0 >= start).then(|| (sample.t.0 - start) as f64)
}

/// What an RPD's doing, derived from whatever's happening to it. `link_down` is §16's: a digital link below its optic's
/// sensitivity or above its overload, either way, which cuts the RPD off as a `CinLinkCut` does.
pub fn rpd_state(
    rpd: &ElementId,
    faults: &[Fault],
    sample: &EnvironmentSample,
    browned_out: bool,
    link_down: bool,
) -> RpdObservation {
    let mut state = RpdState::Operational;
    let mut gcp_status = GcpStatus::Connected;
    let mut ptp_result = PtpResult::Synchronized;
    let mut rf_output_up = true;
    let mut drawing_power = true;
    for fault in faults {
        let Some(elapsed) = elapsed_s(fault, sample) else {
            continue;
        };
        match &fault.kind {
            FaultKind::RpdFailure { rpd_id, mode } if rpd_id == rpd => {
                let recovery = match mode {
                    RpdFailureMode::HardReset { recovery_s }
                    | RpdFailureMode::SoftReset { recovery_s } => *recovery_s,
                    RpdFailureMode::Dead => f64::INFINITY,
                };
                if elapsed < recovery {
                    rf_output_up = false;
                    ptp_result = PtpResult::FreeRunning;
                    gcp_status = GcpStatus::Inactive;
                    match mode {
                        RpdFailureMode::Dead => {
                            state = RpdState::Offline;
                            drawing_power = false;
                        }
                        // A soft reset holds frequency and warm-starts PTP in holdover [R-PHY §8.2.3].
                        RpdFailureMode::SoftReset { .. } => {
                            state = RpdState::Initializing;
                            ptp_result = PtpResult::HoldoverWithinSpec;
                        }
                        RpdFailureMode::HardReset { .. } => state = RpdState::Initializing,
                    }
                }
            }
            FaultKind::CinLinkCut { rpd_id, repair_s } if rpd_id == rpd => {
                if repair_s.is_none_or(|r| elapsed < r) {
                    // Alive and powered, unreachable, and with no data to modulate.
                    state = RpdState::Offline;
                    gcp_status = GcpStatus::Reconnecting;
                    ptp_result = PtpResult::FreeRunning;
                    rf_output_up = false;
                }
            }
            FaultKind::GcpLoss { rpd_id, duration_s } if rpd_id == rpd => {
                if elapsed < *duration_s {
                    gcp_status = GcpStatus::Reconnecting;
                    // No RF effect until retries exhaust, then it soft-resets and keeps trying.
                    if elapsed >= GCP_GRACE_S {
                        state = RpdState::Initializing;
                        rf_output_up = false;
                    }
                }
            }
            FaultKind::TimingLoss { rpd_id, duration_s }
                if rpd_id == rpd && elapsed < *duration_s =>
            {
                if elapsed < HOLDOVER_IN_SPEC_S {
                    ptp_result = PtpResult::HoldoverWithinSpec;
                } else {
                    ptp_result = PtpResult::HoldoverOutOfSpec;
                    if elapsed >= HOLDOVER_IN_SPEC_S + HOLDOVER_OUT_OF_SPEC_S {
                        rf_output_up = false;
                    }
                }
            }
            _ => {}
        }
    }
    if link_down {
        state = RpdState::Offline;
        gcp_status = GcpStatus::Reconnecting;
        ptp_result = PtpResult::FreeRunning;
        rf_output_up = false;
    }
    if browned_out {
        state = RpdState::Offline;
        gcp_status = GcpStatus::Inactive;
        ptp_result = PtpResult::FreeRunning;
        rf_output_up = false;
        drawing_power = false;
    }
    RpdObservation {
        schema_version: SCHEMA_VERSION.to_string(),
        t: sample.t,
        origin: Origin::Synthetic,
        element_id: rpd.clone(),
        top_level_state: state,
        gcp_status,
        ptp_result,
        rf_output_up,
        drawing_power,
    }
}

//! Who is affected: computed from telemetry and topology, with no labels (`docs/DESIGN.md`). These
//! thresholds are diagnoser config, not sim facts (§5.3).

use features::{ModemFeatures, StepFeatures};
use plant_graph::PlantGraph;
use schema::common::{EdgeId, ElementId};
use serde::{Deserialize, Serialize};

/// How far a feature must move before a modem counts as affected.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct WhoThresholds {
    /// Level against design, dB.
    pub level_db: f64,
    /// Transmit power against design, dB.
    pub us_tx_db: f64,
    /// RxMER against design, dB.
    pub mer_db: f64,
    /// RxMER margin in any bucket below this, dB.
    pub margin_floor_db: f64,
    /// NMTER above design, dB.
    pub nmter_db: f64,
    /// Channel estimate ripple, RMS, dB, over the design's own healthy prediction where it has one.
    pub ripple_db: f64,
    /// Uncorrectable rate.
    pub uncorrectable_rate: f64,
    /// A pre-eq tap above design, dB.
    pub tap_rise_db: f64,
    /// Input voltage below design, volts.
    pub v_drop: f64,
}

impl Default for WhoThresholds {
    fn default() -> Self {
        WhoThresholds {
            level_db: 1.5,
            us_tx_db: 1.5,
            mer_db: 1.5,
            margin_floor_db: -3.0,
            nmter_db: 4.0,
            ripple_db: 0.25,
            uncorrectable_rate: 1e-4,
            tap_rise_db: 6.0,
            v_drop: 4.0,
        }
    }
}

/// Who a step's telemetry says is affected.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Who {
    /// Modems that look wrong, in canonical order.
    pub affected: Vec<ElementId>,
    /// How many of them are out of service.
    pub out_of_service: usize,
    /// Whether the upstream as a whole looks wrong, which modem data can't place (§7.3).
    pub upstream_degraded: bool,
    /// Actives reporting a problem.
    pub actives: Vec<ElementId>,
    /// Whether the RPD reports anything but normal.
    #[serde(default)]
    pub rpd_abnormal: bool,
    /// Where the notes' scope rules put it, from the plant: set by [`scope_of`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
}

/// Where a problem is, by who's affected and who isn't [PNM-HFC §8.8.9.3, impairments.md §12]. Computed from the plant
/// graph, not learnt.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Scope {
    /// One device in a home and not another: inside the home.
    InHome {
        /// The home's drop.
        drop: EdgeId,
    },
    /// One home on a tap and not another: its drop or inside it.
    DropOrHome {
        /// The home's drop.
        drop: EdgeId,
    },
    /// Several homes: a cable or an active.
    Leg,
}

/// The scope rules on a step: each affected modem's home through the premises, then which rule holds. `None` where
/// nothing's affected, or one home is and its tap has no other home reporting to tell it by.
pub fn scope_of(
    graph: &PlantGraph<'_>,
    features: &StepFeatures,
    affected: &[ElementId],
) -> Option<Scope> {
    if affected.is_empty() {
        return None;
    }
    let home_of = |id: &ElementId| -> Option<(EdgeId, ElementId)> {
        let p = graph.premises_of(id).ok().flatten()?;
        Some((p.drop.id.clone(), p.tap.id.clone()))
    };
    let homes: std::collections::BTreeSet<(EdgeId, ElementId)> =
        affected.iter().filter_map(home_of).collect();
    if homes.len() != 1 {
        return Some(Scope::Leg);
    }
    let (drop, tap) = homes.into_iter().next()?;
    let unaffected: Vec<(EdgeId, ElementId)> = features
        .modems
        .iter()
        .filter(|m| !affected.contains(&m.element_id))
        .filter_map(|m| home_of(&m.element_id))
        .collect();
    if unaffected.iter().any(|(d, _)| *d == drop) {
        Some(Scope::InHome { drop })
    } else if unaffected.iter().any(|(_, t)| *t == tap) {
        Some(Scope::DropOrHome { drop })
    } else {
        None
    }
}

/// Whether one modem looks wrong.
pub fn modem_affected(modem: &ModemFeatures, thresholds: &WhoThresholds) -> bool {
    let over = |value: Option<f64>, limit: f64| value.is_some_and(|v| v.abs() > limit);
    !modem.operational
        || over(modem.level_drift_db, thresholds.level_db)
        || over(modem.level_drift_min_db, 2.0 * thresholds.level_db)
        || over(modem.us_tx_drift_db, thresholds.us_tx_db)
        || modem.mer_drift_db.is_some_and(|d| d < -thresholds.mer_db)
        || modem
            .preeq_echo_rise_db
            .is_some_and(|d| d > thresholds.tap_rise_db)
        || modem
            .preeq_nmter_drift_db
            .is_some_and(|d| d > thresholds.nmter_db)
        // Against the design's healthy ripple where it predicts one: a home's splitters ripple a healthy estimate.
        || modem
            .chan_est_ripple_drift_db
            .or(modem.chan_est_ripple_db)
            .is_some_and(|r| r > thresholds.ripple_db)
        || modem
            .uncorrectable_rate
            .is_some_and(|r| r > thresholds.uncorrectable_rate)
        || modem.rxmer_margin_buckets.as_ref().is_some_and(|buckets| {
            buckets
                .iter()
                .any(|margin| *margin < thresholds.margin_floor_db)
        })
        || modem.t4_delta > 0
        || modem.ds_unusable_share > 0.0
        || modem.us_unusable_share > 0.0
}

/// Reads a step's features.
pub fn who_is_affected(features: &StepFeatures, thresholds: &WhoThresholds) -> Who {
    let affected: Vec<ElementId> = features
        .modems
        .iter()
        .filter(|m| modem_affected(m, thresholds))
        .map(|m| m.element_id.clone())
        .collect();
    let out_of_service = features.modems.iter().filter(|m| !m.operational).count();
    let group = &features.service_group;
    let upstream_degraded =
        group.us_snr_drift_min_db.is_some_and(|d| d < -3.0) || group.us_uncorrectable_rate > 1e-4;
    let actives = features
        .actives
        .iter()
        .filter(|a| {
            a.out_level_drift_db
                .is_some_and(|d| d.abs() > thresholds.level_db)
                || a.silent
                || a.v_drift.is_some_and(|d| d < -thresholds.v_drop)
        })
        .map(|a| a.element_id.clone())
        .collect();
    let rpd_abnormal = features.rpd.as_ref().is_some_and(|r| {
        r.top_level_state != schema::rpd::RpdState::Operational
            || r.gcp_status != schema::rpd::GcpStatus::Connected
            || r.ptp_result != schema::rpd::PtpResult::Synchronized
            || !r.rf_output_up
    });
    Who {
        affected,
        out_of_service,
        upstream_degraded,
        actives,
        rpd_abnormal,
        scope: None,
    }
}

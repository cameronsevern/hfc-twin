//! The design checks that need physics (§3.3, §3.4, §6.3, §7.2), and the numbers behind them.

use std::collections::BTreeMap;

use plant_graph::PlantGraph;
use schema::common::ElementId;
use schema::component::{ComponentSpec, ModemGeneration};
use schema::environment::EnvironmentSample;
use schema::plant::{Edge, ElementKind};

use crate::ac::{solve_ac, solve_ac_at};
use crate::error::PhysicsError;
use crate::overlay::Overlay;
use crate::passive::check_rated;
use crate::units::db;
use crate::upstream::{p1p6hi_dbmv, per_1p6_from_per_6p4_db};
use crate::walk::{Band, Grid, ModemView, Walker, ds_band, out_level_dbmv, us_band};

/// A plant that's well-formed but doesn't meet a limit that needs physics.
#[derive(Debug, Clone, PartialEq)]
pub enum PhysicsIssue {
    /// §3.3: above the modem window.
    LevelTooHigh {
        /// The modem.
        modem: ElementId,
        /// Its worst level, dBmV per 6 MHz.
        level_dbmv: f64,
        /// Where, MHz.
        f_mhz: f64,
    },
    /// §3.3: below the floor for the highest modulation in use.
    LevelTooLow {
        /// The modem.
        modem: ElementId,
        /// Its worst level, dBmV per 6 MHz.
        level_dbmv: f64,
        /// Where, MHz.
        f_mhz: f64,
        /// The floor it missed.
        floor_dbmv: f64,
    },
    /// §3.3: tilt at the modem.
    PsdSpread {
        /// The modem.
        modem: ElementId,
        /// How far the highest band sits above the lowest, dB.
        spread_db: f64,
    },
    /// §3.3: total input power.
    TotalInputTooHigh {
        /// The modem.
        modem: ElementId,
        /// Total input, dBmV.
        total_dbmv: f64,
    },
    /// §3.4: more gain than the amp has.
    GainExceeded {
        /// The amp.
        amp: ElementId,
        /// Gain needed, dB.
        gain_db: f64,
    },
    /// §3.4: less input than the amp needs.
    InputTooLow {
        /// The amp.
        amp: ElementId,
        /// Input at the band's top, dBmV.
        level_dbmv: f64,
    },
    /// §3.1: more output than the part can deliver.
    OutputExceeded {
        /// The active.
        active: ElementId,
    },
    /// §6.3: below minimum voltage.
    Brownout {
        /// The active.
        active: ElementId,
        /// Input voltage.
        v_in: f64,
    },
    /// §6.3: above 80 % of rating.
    SupplyOverloaded {
        /// The supply.
        supply: ElementId,
        /// Its total current, amps.
        i_total: f64,
    },
    /// §7.2: the modem can't deliver what the node needs.
    TxLimit {
        /// The modem.
        modem: ElementId,
        /// Required, dBmV per 1.6 MHz.
        required_dbmv_per_1p6mhz: f64,
        /// Its limit.
        limit_dbmv_per_1p6mhz: f64,
    },
    /// §7.2: below what a modem can report or transmit.
    TxBelowFloor {
        /// The modem.
        modem: ElementId,
        /// Required, dBmV per 1.6 MHz.
        required_dbmv_per_1p6mhz: f64,
    },
    /// §6.2, §14: more current out of one of an inserter's ports than the part rates a port for.
    InserterPortOverloaded {
        /// The inserter.
        inserter: ElementId,
        /// The edge out of the port.
        edge: schema::common::EdgeId,
        /// Its current, amps.
        i_a: f64,
        /// The part's per-port rating, amps.
        rated_a: f64,
    },
    /// §15: a fitted plug-in moved by a misalignment (§8) to a value its family doesn't reach. Which family and slot a part
    /// may fill is checked when the plant is read (task 21); this is the value under the faults.
    PlugInOutsideFamily {
        /// The host.
        element: ElementId,
        /// The plug-in fitted.
        part: schema::common::ComponentId,
        /// The value the fault leaves it at, dB.
        value_db: f64,
        /// The family's range for that kind, dB.
        lo_db: f64,
        /// The family's range for that kind, dB.
        hi_db: f64,
    },
    /// §13: a modem's transmit power reaching a set-top or television in the same home, through the splitter's isolation, above
    /// what a tuner takes where the return band reaches the tuner band (54-108 MHz) [TR-MIDSPLIT §6.2].
    TransmitIntoTuner {
        /// The modem.
        modem: ElementId,
        /// The device it reaches.
        device: ElementId,
        /// Its level there, dBmV per 6.4 MHz.
        level_dbmv: f64,
    },
    /// §13: a filter whose stop band covers channels a device below it uses, which a rated band can't show.
    FilterBlocksChannel {
        /// The filter.
        filter: ElementId,
        /// A device below it.
        device: ElementId,
        /// The channel's band, MHz.
        lo_mhz: f64,
        /// The channel's band, MHz.
        hi_mhz: f64,
    },
    /// §16: received power outside the receiver's window, or no light at all.
    OpticalInputOutOfRange {
        /// The link.
        link: schema::common::LinkId,
        /// Received power, dBm, or `None` with no light.
        received_dbm: Option<f64>,
        /// The window's bottom: an analog receiver's lowest input or an optic's sensitivity, dBm.
        lo_dbm: f64,
        /// Its top: highest input or overload, dBm.
        hi_dbm: f64,
    },
    /// §16: a return laser driven past the composite its part says it clips at.
    LaserOverdriven {
        /// The link.
        link: schema::common::LinkId,
        /// Its total RF input, dBmV.
        total_in_dbmv: f64,
        /// Where its part says it clips, dBmV.
        clip_dbmv: f64,
    },
}

/// One modem's numbers over its bands.
#[derive(Debug, Clone, PartialEq)]
pub struct ModemBudget {
    /// Lowest level at its input over the whole downstream band, dBmV per 6 MHz.
    pub level_min_dbmv: f64,
    /// Highest level at its input over the whole downstream band, dBmV per 6 MHz.
    pub level_max_dbmv: f64,
    /// `level_max − level_min`, dB.
    pub spread_db: f64,
    /// Total input over the whole downstream band, dBmV.
    pub total_input_dbmv: f64,
    /// Lowest level over what it demodulates, which §3.3's floor applies to, dBmV per 6 MHz.
    pub demod_min_dbmv: f64,
    /// Highest level over what it demodulates, dBmV per 6 MHz.
    pub demod_max_dbmv: f64,
    /// Lowest transmit power, dBmV per 1.6 MHz.
    pub tx_min_dbmv_per_1p6mhz: f64,
    /// Highest transmit power, dBmV per 1.6 MHz.
    pub tx_max_dbmv_per_1p6mhz: f64,
    /// The transmit limit, dBmV per 1.6 MHz.
    pub tx_limit_dbmv_per_1p6mhz: f64,
    /// Highest power it actually transmits: on a D3.0 modem, only over its SC-QAM channels, dBmV per 1.6 MHz.
    pub tx_used_max_dbmv_per_1p6mhz: f64,
    /// Lowest power it actually transmits, dBmV per 1.6 MHz.
    pub tx_used_min_dbmv_per_1p6mhz: f64,
}

/// The whole plant's numbers at one sample.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelBudget {
    /// Per modem, in canonical order.
    pub modems: BTreeMap<ElementId, ModemBudget>,
}

/// §7.2: a modem's transmit limit per 1.6 MHz, on this profile's whole return band.
pub fn tx_limit_dbmv_per_1p6mhz(
    graph: &PlantGraph<'_>,
    tx_max_total_dbmv: f64,
    generation: ModemGeneration,
) -> f64 {
    let (us_lo_mhz, us_hi_mhz) = us_band(graph.profile());
    match generation {
        ModemGeneration::D31 => p1p6hi_dbmv(tx_max_total_dbmv, 0.0, us_hi_mhz - us_lo_mhz),
        // D3.0: total composite power over its SC-QAM channels, 6.4 MHz each.
        ModemGeneration::D30 => {
            let channels = graph.profile().channel_plan.us_sc_qam.len().max(1) as f64;
            tx_max_total_dbmv - db(channels) - per_1p6_from_per_6p4_db()
        }
    }
}

/// The band a modem demodulates: the profile's, as far as its rating reaches.
pub fn modem_ds_band(graph: &PlantGraph<'_>, spec_rated_hi_mhz: f64) -> (f64, f64) {
    let (lo, hi) = ds_band(graph.profile());
    (lo, hi.min(spec_rated_hi_mhz))
}

const STEP_MHZ: f64 = 1.0;

fn inclusive_grid(lo_mhz: f64, hi_mhz: f64) -> Grid {
    let n = ((hi_mhz - lo_mhz) / STEP_MHZ).floor() as usize + 1;
    let mut f_mhz: Vec<f64> = (0..n).map(|i| lo_mhz + i as f64 * STEP_MHZ).collect();
    if f_mhz.last().is_none_or(|last| *last < hi_mhz) {
        f_mhz.push(hi_mhz);
    }
    Grid::new(f_mhz)
}

/// §3.2, §7.2: the numbers for every modem at one sample.
pub fn level_budget(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    overlay: &Overlay,
) -> Result<LevelBudget, PhysicsError> {
    let walker = Walker::new(graph, sample, overlay)?;
    let (ds_lo_mhz, ds_hi_mhz) = ds_band(graph.profile());
    let (us_lo_mhz, us_hi_mhz) = us_band(graph.profile());
    let target = graph.profile().design_levels.us_node_in_target_dbmv;

    let mut modems: BTreeMap<ElementId, ModemBudget> = BTreeMap::new();
    let down = inclusive_grid(ds_lo_mhz, ds_hi_mhz);
    walker.run(&down, Band::Down, &mut |view: &ModemView<'_>| {
        let rated_hi_mhz = view.component.rated_hi_mhz.unwrap_or(ds_hi_mhz);
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut demod_lo, mut demod_hi) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut total_lin = 0.0;
        for (i, &f) in view.grid.f_mhz.iter().enumerate() {
            let level_dbmv = view.level_dbmv[i];
            lo = lo.min(level_dbmv);
            hi = hi.max(level_dbmv);
            if f <= rated_hi_mhz {
                demod_lo = demod_lo.min(level_dbmv);
                demod_hi = demod_hi.max(level_dbmv);
            }
            // Levels are per 6 MHz and the grid is per MHz.
            total_lin += 10f64.powf(level_dbmv / 10.0) * STEP_MHZ / 6.0;
        }
        modems.insert(
            view.modem.id.clone(),
            ModemBudget {
                level_min_dbmv: lo,
                level_max_dbmv: hi,
                spread_db: hi - lo,
                total_input_dbmv: db(total_lin),
                demod_min_dbmv: demod_lo,
                demod_max_dbmv: demod_hi,
                tx_min_dbmv_per_1p6mhz: f64::INFINITY,
                tx_max_dbmv_per_1p6mhz: f64::NEG_INFINITY,
                tx_limit_dbmv_per_1p6mhz: f64::INFINITY,
                tx_used_max_dbmv_per_1p6mhz: f64::NEG_INFINITY,
                tx_used_min_dbmv_per_1p6mhz: f64::INFINITY,
            },
        );
        Ok(())
    })?;

    let up = inclusive_grid(us_lo_mhz, us_hi_mhz);
    walker.run(&up, Band::Up, &mut |view: &ModemView<'_>| {
        let ComponentSpec::Modem(modem) = &view.component.spec else {
            return Err(PhysicsError::BadComponent(view.modem.id.clone()));
        };
        let rated_lo = view.component.rated_lo_mhz.unwrap_or(us_lo_mhz);
        if let Some(entry) = modems.get_mut(&view.modem.id) {
            for (i, &f) in view.grid.f_mhz.iter().enumerate() {
                if f < rated_lo {
                    continue;
                }
                let tx = target + view.loss_up_db[i] - per_1p6_from_per_6p4_db();
                entry.tx_min_dbmv_per_1p6mhz = entry.tx_min_dbmv_per_1p6mhz.min(tx);
                entry.tx_max_dbmv_per_1p6mhz = entry.tx_max_dbmv_per_1p6mhz.max(tx);
                let used = modem.generation == ModemGeneration::D31
                    || graph
                        .profile()
                        .channel_plan
                        .us_sc_qam
                        .iter()
                        .any(|c| (f - c.center_mhz).abs() <= c.width_mhz / 2.0);
                if used {
                    entry.tx_used_max_dbmv_per_1p6mhz = entry.tx_used_max_dbmv_per_1p6mhz.max(tx);
                    entry.tx_used_min_dbmv_per_1p6mhz = entry.tx_used_min_dbmv_per_1p6mhz.min(tx);
                }
            }
            entry.tx_limit_dbmv_per_1p6mhz =
                tx_limit_dbmv_per_1p6mhz(graph, modem.tx_max_total_dbmv, modem.generation);
        }
        Ok(())
    })?;
    Ok(LevelBudget { modems })
}

/// §3.3: the floor for the highest modulation this profile uses on a modem generation.
fn floor_dbmv(graph: &PlantGraph<'_>, generation: ModemGeneration) -> f64 {
    let plan = &graph.profile().channel_plan;
    let levels = &graph.profile().design_levels;
    let highest = match generation {
        ModemGeneration::D30 => plan.ds_sc_qam.iter().map(|c| c.modulation).max_by(|a, b| {
            crate::fec::bits_per_symbol(*a).total_cmp(&crate::fec::bits_per_symbol(*b))
        }),
        ModemGeneration::D31 => plan
            .ofdm
            .iter()
            .flat_map(|c| c.data_profiles.iter().map(|p| p.modulation))
            .max_by(|a, b| {
                crate::fec::bits_per_symbol(*a).total_cmp(&crate::fec::bits_per_symbol(*b))
            }),
    };
    highest
        .and_then(|qam| {
            levels
                .modem_in_min_dbmv_by_qam
                .iter()
                .find(|floor| floor.qam == qam)
                .map(|floor| floor.min_dbmv)
        })
        .unwrap_or(f64::NEG_INFINITY)
}

/// §3.4: an amp's alignment, at one sample.
fn check_amps(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    issues: &mut Vec<PhysicsIssue>,
) -> Result<(), PhysicsError> {
    let profile = graph.profile();
    let (_, ds_hi_mhz) = ds_band(profile);
    let out_dbmv = out_level_dbmv(profile, ds_hi_mhz);
    for element in graph.actives() {
        let active = graph
            .component(&element.component_ref)
            .and_then(|c| c.spec.active())
            .ok_or_else(|| PhysicsError::BadComponent(element.id.clone()))?;
        if out_dbmv > active.max_out_dbmv_at_hi {
            issues.push(PhysicsIssue::OutputExceeded {
                active: element.id.clone(),
            });
        }
        if !matches!(element.kind, ElementKind::Amp { .. }) {
            continue;
        }
        let in_dbmv = out_dbmv - section_loss_db(graph, sample, &element.id, ds_hi_mhz)?;
        let gain_db = out_dbmv - in_dbmv;
        if gain_db > active.max_gain_db {
            issues.push(PhysicsIssue::GainExceeded {
                amp: element.id.clone(),
                gain_db,
            });
        }
        if in_dbmv < active.min_in_dbmv {
            issues.push(PhysicsIssue::InputTooLow {
                amp: element.id.clone(),
                level_dbmv: in_dbmv,
            });
        }
    }
    Ok(())
}

/// Passive loss from the active above `id` down to `id`'s input, dB.
fn section_loss_db(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    id: &ElementId,
    f_mhz: f64,
) -> Result<f64, PhysicsError> {
    use schema::fault::Location;
    let mut loss_db = 0.0;
    let mut current = graph.element(id)?;
    while let Some((parent, edge)) = graph.parent(&current.id)? {
        loss_db += edge_loss_db(graph, sample, edge, f_mhz)?;
        let component = graph
            .component(&parent.component_ref)
            .ok_or_else(|| PhysicsError::BadComponent(parent.id.clone()))?;
        if component.spec.active().is_some() {
            return Ok(loss_db);
        }
        check_rated(component, f_mhz)?;
        // The parent's own crossing is from its input to the port this edge leaves by.
        if let Some(input) = parent
            .ports
            .iter()
            .find(|p| p.role == schema::plant::PortRole::Input)
        {
            let a = Location::Port {
                element_id: parent.id.clone(),
                port: input.id.clone(),
            };
            let b = Location::Port {
                element_id: parent.id.clone(),
                port: edge.from.port.clone(),
            };
            loss_db += crate::transfer::transfer(graph, sample, &a, &b, f_mhz)?.loss_db;
        }
        current = parent;
    }
    Ok(loss_db)
}

fn edge_loss_db(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    edge: &Edge,
    f_mhz: f64,
) -> Result<f64, PhysicsError> {
    use schema::fault::Location;
    let a = Location::Port {
        element_id: edge.from.element_id.clone(),
        port: edge.from.port.clone(),
    };
    let b = Location::Port {
        element_id: edge.to.element_id.clone(),
        port: edge.to.port.clone(),
    };
    Ok(crate::transfer::transfer(graph, sample, &a, &b, f_mhz)?.loss_db)
}

/// Every limit in §3.3, §3.4, §6.3 and §7.2, over these environment samples, de-duplicated by
/// keeping the worst case of each kind per element.
pub fn check_physics(
    graph: &PlantGraph<'_>,
    samples: &[EnvironmentSample],
    overlay: &Overlay,
) -> Result<Vec<PhysicsIssue>, PhysicsError> {
    let mut issues = Vec::new();
    let levels = &graph.profile().design_levels;
    let (ds_lo_mhz, ds_hi_mhz) = ds_band(graph.profile());
    for sample in samples {
        check_amps(graph, sample, &mut issues)?;
        let budget = level_budget(graph, sample, overlay)?;
        for (id, modem) in &budget.modems {
            let spec = graph
                .component(&graph.element(id)?.component_ref)
                .and_then(|c| match &c.spec {
                    ComponentSpec::Modem(m) => Some(m),
                    _ => None,
                })
                .ok_or_else(|| PhysicsError::BadComponent(id.clone()))?;
            if modem.demod_max_dbmv > levels.modem_in_max_dbmv {
                issues.push(PhysicsIssue::LevelTooHigh {
                    modem: id.clone(),
                    level_dbmv: modem.demod_max_dbmv,
                    f_mhz: ds_lo_mhz,
                });
            }
            let floor = floor_dbmv(graph, spec.generation);
            if modem.demod_min_dbmv < floor {
                issues.push(PhysicsIssue::LevelTooLow {
                    modem: id.clone(),
                    level_dbmv: modem.demod_min_dbmv,
                    f_mhz: ds_hi_mhz,
                    floor_dbmv: floor,
                });
            }
            if modem.demod_max_dbmv - modem.demod_min_dbmv > levels.psd_spread_max_db {
                issues.push(PhysicsIssue::PsdSpread {
                    modem: id.clone(),
                    spread_db: modem.demod_max_dbmv - modem.demod_min_dbmv,
                });
            }
            if modem.total_input_dbmv >= levels.modem_in_total_max_dbmv {
                issues.push(PhysicsIssue::TotalInputTooHigh {
                    modem: id.clone(),
                    total_dbmv: modem.total_input_dbmv,
                });
            }
            if modem.tx_used_max_dbmv_per_1p6mhz > modem.tx_limit_dbmv_per_1p6mhz {
                issues.push(PhysicsIssue::TxLimit {
                    modem: id.clone(),
                    required_dbmv_per_1p6mhz: modem.tx_used_max_dbmv_per_1p6mhz,
                    limit_dbmv_per_1p6mhz: modem.tx_limit_dbmv_per_1p6mhz,
                });
            }
            if spec.generation == ModemGeneration::D31 && modem.tx_used_min_dbmv_per_1p6mhz < 17.0 {
                issues.push(PhysicsIssue::TxBelowFloor {
                    modem: id.clone(),
                    required_dbmv_per_1p6mhz: modem.tx_used_min_dbmv_per_1p6mhz,
                });
            }
        }
    }
    // §19: loop resistance moves with temperature, so each active's lowest voltage and each supply's highest current over
    // the samples. A plant whose cable states no temperature term solves the same at every sample.
    let mut power = solve_ac(graph, overlay)?;
    for sample in samples {
        let at = solve_ac_at(graph, overlay, sample)?;
        for (id, active) in &mut power.actives {
            if let Some(other) = at.actives.get(id)
                && other.v_in < active.v_in
            {
                *active = other.clone();
            }
        }
        for (id, supply) in &mut power.supplies {
            if let Some(other) = at.supplies.get(id)
                && other.i_total > supply.i_total
            {
                *supply = other.clone();
            }
        }
    }
    for (id, active) in &power.actives {
        if active.v_in < active.v_min {
            issues.push(PhysicsIssue::Brownout {
                active: id.clone(),
                v_in: active.v_in,
            });
        }
    }
    for (id, supply) in &power.supplies {
        if supply.i_total > 0.8 * supply.i_rated {
            issues.push(PhysicsIssue::SupplyOverloaded {
                supply: id.clone(),
                i_total: supply.i_total,
            });
        }
    }
    inserter_ports(graph, &power, &mut issues);
    plug_in_families(graph, overlay, &mut issues);
    filters_in_the_way(graph, &mut issues)?;
    if let Some(sample) = samples.first() {
        transmit_into_tuners(graph, sample, overlay, &mut issues)?;
    }
    Ok(issues)
}

/// §6.2, §14: each inserter port's current, the actives beyond it, against the part's per-port rating where it states one.
fn inserter_ports(
    graph: &PlantGraph<'_>,
    power: &crate::ac::AcSolution,
    issues: &mut Vec<PhysicsIssue>,
) {
    for element in &graph.plant().elements {
        let Some(rated_a) = graph
            .component(&element.component_ref)
            .and_then(|c| match &c.spec {
                ComponentSpec::PowerInserter(spec) => spec.port_i_rated_a,
                _ => None,
            })
        else {
            continue;
        };
        for feed in &graph.power().feeds {
            for segment in &feed.segments {
                let edge = segment.edge;
                if edge.from.element_id != element.id && edge.to.element_id != element.id {
                    continue;
                }
                let i_a: f64 = segment
                    .actives_beyond
                    .iter()
                    .filter_map(|id| power.actives.get(id))
                    .map(|a| a.i_draw)
                    .sum();
                if i_a > rated_a {
                    issues.push(PhysicsIssue::InserterPortOverloaded {
                        inserter: element.id.clone(),
                        edge: edge.id.clone(),
                        i_a,
                        rated_a,
                    });
                }
            }
        }
    }
}

/// §15, §8: a misalignment's deltas against the values the active's family offers for each slot.
fn plug_in_families(graph: &PlantGraph<'_>, overlay: &Overlay, issues: &mut Vec<PhysicsIssue>) {
    use schema::component::PlugInKind as K;
    for element in &graph.plant().elements {
        let (Some((pad_db, eq_db, return_pad_db)), Some(schema::plant::Fitted::Active(fitted))) = (
            overlay.amps.get(&element.id).and_then(|o| o.misalignment),
            &element.fitted,
        ) else {
            continue;
        };
        let Some(family) = graph
            .component(&element.component_ref)
            .and_then(|c| c.spec.active())
            .and_then(|a| a.plugin_family.as_deref())
        else {
            continue;
        };
        let range = |kinds: &[K]| -> Option<(f64, f64)> {
            let values: Vec<f64> = graph
                .library()
                .components
                .iter()
                .filter_map(|c| match &c.spec {
                    ComponentSpec::PlugIn(p) if p.family == family && kinds.contains(&p.kind) => {
                        Some(p.value_db)
                    }
                    _ => None,
                })
                .collect();
            let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
        };
        for (slot, delta_db, kinds) in [
            (&fitted.input_pad, pad_db, &[K::Pad][..]),
            (
                &fitted.input_eq,
                eq_db,
                &[K::Equalizer, K::CableSimulator][..],
            ),
            (&fitted.return_pad, return_pad_db, &[K::ReturnPad][..]),
        ] {
            let (Some(part), true) = (slot, delta_db != 0.0) else {
                continue;
            };
            let Some(ComponentSpec::PlugIn(spec)) = graph.component(part).map(|c| &c.spec) else {
                continue;
            };
            let Some((lo_db, hi_db)) = range(kinds) else {
                continue;
            };
            let value_db = spec.value_db + delta_db;
            if value_db < lo_db || value_db > hi_db {
                issues.push(PhysicsIssue::PlugInOutsideFamily {
                    element: element.id.clone(),
                    part: part.clone(),
                    value_db,
                    lo_db,
                    hi_db,
                });
            }
        }
    }
}

/// What a tuner takes before a modem's transmit overloads it, dBmV per 6.4 MHz: 46 dBmV through the best measured in-home isolation
/// of 25 dB, or 30 through the worst of 9 [TR-MIDSPLIT §6.2.1], both 21 at the tuner (our arithmetic).
pub const TUNER_LIMIT_DBMV_PER_6P4MHZ: f64 = 21.0;
/// The bottom of the tuner band, MHz [TR-MIDSPLIT §6.2].
const TUNER_LO_MHZ: f64 = 54.0;

/// §13: a modem's transmit power at each set-top or television behind the same splitter, where the return band reaches the tuner
/// band. Read at 54 MHz, the band's bottom, where cable loses least.
fn transmit_into_tuners(
    graph: &PlantGraph<'_>,
    sample: &EnvironmentSample,
    overlay: &Overlay,
    issues: &mut Vec<PhysicsIssue>,
) -> Result<(), PhysicsError> {
    use schema::fault::Location;
    use schema::plant::ElementKind;
    let (_, us_hi_mhz) = us_band(graph.profile());
    if us_hi_mhz <= TUNER_LO_MHZ {
        return Ok(());
    }
    let mut budget: Option<LevelBudget> = None;
    for splitter in &graph.plant().elements {
        let Some(iso_db) = graph
            .component(&splitter.component_ref)
            .and_then(|c| match &c.spec {
                ComponentSpec::Splitter(spec) => spec.leg_isolation_db,
                _ => None,
            })
        else {
            continue;
        };
        let mut by_leg: Vec<(Location, Vec<&schema::plant::Element>)> = Vec::new();
        for leg in splitter
            .ports
            .iter()
            .filter(|p| p.role == schema::plant::PortRole::Leg)
        {
            let at = Location::Port {
                element_id: splitter.id.clone(),
                port: leg.id.clone(),
            };
            let below = graph.below(&at)?;
            by_leg.push((at, below));
        }
        for (a, (leg_a, below_a)) in by_leg.iter().enumerate() {
            for modem in below_a
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Modem))
            {
                if budget.is_none() {
                    budget = Some(level_budget(graph, sample, overlay)?);
                }
                let Some(tx) = budget.as_ref().and_then(|b| b.modems.get(&modem.id)) else {
                    continue;
                };
                let tx_dbmv = tx.tx_used_max_dbmv_per_1p6mhz + db(6.4 / 1.6);
                let Some(modem_port) = modem.ports.first() else {
                    continue;
                };
                let to_modem = Location::Port {
                    element_id: modem.id.clone(),
                    port: modem_port.id.clone(),
                };
                let up_db =
                    crate::transfer::transfer(graph, sample, leg_a, &to_modem, TUNER_LO_MHZ)?
                        .loss_db;
                for (b, (leg_b, below_b)) in by_leg.iter().enumerate() {
                    if a == b {
                        continue;
                    }
                    for device in below_b {
                        let tuner = matches!(device.kind, ElementKind::Cpe)
                            && graph.component(&device.component_ref).is_some_and(|c| {
                                matches!(
                                    &c.spec,
                                    ComponentSpec::Cpe(spec) if matches!(
                                        spec.device,
                                        schema::component::CpeDevice::SetTop
                                            | schema::component::CpeDevice::Television
                                    )
                                )
                            });
                        let Some(port) = device.ports.first().filter(|_| tuner) else {
                            continue;
                        };
                        let to_device = Location::Port {
                            element_id: device.id.clone(),
                            port: port.id.clone(),
                        };
                        let down_db = crate::transfer::transfer(
                            graph,
                            sample,
                            leg_b,
                            &to_device,
                            TUNER_LO_MHZ,
                        )?
                        .loss_db;
                        let level_dbmv = tx_dbmv - up_db - iso_db - down_db;
                        if level_dbmv > TUNER_LIMIT_DBMV_PER_6P4MHZ {
                            issues.push(PhysicsIssue::TransmitIntoTuner {
                                modem: modem.id.clone(),
                                device: device.id.clone(),
                                level_dbmv,
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// §13: a filter's stop bands against the channels every device below it uses: a modem both ways, a `Cpe` downstream.
fn filters_in_the_way(
    graph: &PlantGraph<'_>,
    issues: &mut Vec<PhysicsIssue>,
) -> Result<(), PhysicsError> {
    let plan = &graph.profile().channel_plan;
    let mut down: Vec<(f64, f64)> = plan
        .ds_sc_qam
        .iter()
        .map(|c| {
            (
                c.center_mhz - c.width_mhz / 2.0,
                c.center_mhz + c.width_mhz / 2.0,
            )
        })
        .collect();
    down.extend(
        plan.ofdm
            .iter()
            .map(|c| (c.first_active_mhz, c.last_active_mhz)),
    );
    let mut up: Vec<(f64, f64)> = plan
        .us_sc_qam
        .iter()
        .map(|c| {
            (
                c.center_mhz - c.width_mhz / 2.0,
                c.center_mhz + c.width_mhz / 2.0,
            )
        })
        .collect();
    up.extend(
        plan.ofdma
            .iter()
            .map(|c| (c.first_active_mhz, c.last_active_mhz)),
    );
    for element in &graph.plant().elements {
        let Some(ComponentSpec::Filter(filter)) =
            graph.component(&element.component_ref).map(|c| &c.spec)
        else {
            continue;
        };
        let Some(out) = element
            .ports
            .iter()
            .find(|p| p.role != schema::plant::PortRole::Input)
        else {
            continue;
        };
        let below = graph.below(&schema::fault::Location::Port {
            element_id: element.id.clone(),
            port: out.id.clone(),
        })?;
        for device in below {
            let used: Vec<&(f64, f64)> = match device.kind {
                schema::plant::ElementKind::Modem => down.iter().chain(&up).collect(),
                schema::plant::ElementKind::Cpe => down.iter().collect(),
                _ => continue,
            };
            for &&(lo_mhz, hi_mhz) in &used {
                if filter
                    .stop_bands
                    .iter()
                    .any(|band| band.lo_mhz < hi_mhz && band.hi_mhz > lo_mhz)
                {
                    issues.push(PhysicsIssue::FilterBlocksChannel {
                        filter: element.id.clone(),
                        device: device.id.clone(),
                        lo_mhz,
                        hi_mhz,
                    });
                }
            }
        }
    }
    Ok(())
}

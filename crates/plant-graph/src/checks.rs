//! Checks on a well-formed plant: whether it's allowed, and whether a fault set fits it.

use schema::common::{ComponentId, EdgeId, ElementId};
use schema::component::qualifies;
use schema::fault::{FaultKind, FaultSet, Location};
use schema::plant::{EdgeKind, ElementKind};
use thiserror::Error;

use crate::graph::{PlantGraph, QueryError};

/// A well-formed plant that isn't to spec. Step 3 adds the checks that need physics.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum DesignIssue {
    /// A modem sits behind more amps than the profile allows.
    #[error("modem {modem} is at N+{depth}, over the profile's N+{max}")]
    CascadeExceeded {
        /// The modem.
        modem: ElementId,
        /// Amps between the root and the modem.
        depth: usize,
        /// The profile's limit.
        max: u8,
    },
    /// A part can't be used on this profile.
    #[error("part {component} doesn't qualify for this profile")]
    PartNotQualified {
        /// The part.
        component: ComponentId,
    },
    /// No supply reaches an active.
    #[error("active {element} has no power")]
    Unpowered {
        /// The active.
        element: ElementId,
    },
    /// More than one supply reaches an active.
    #[error("active {element} is reached by more than one supply")]
    ContestedPower {
        /// The active.
        element: ElementId,
    },
}

/// One thing wrong with one fault.
#[derive(Error, Debug, Clone, PartialEq)]
#[error("fault {fault_index}: {problem}")]
pub struct FaultRefError {
    /// Its position in the fault set.
    pub fault_index: usize,
    /// What's wrong.
    pub problem: FaultRefProblem,
}

/// How a fault fails to fit a plant. References and ranges only, nothing physical.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum FaultRefProblem {
    /// A location that doesn't exist on this plant.
    #[error(transparent)]
    Location(#[from] QueryError),
    /// Water ingress on something other than a span.
    #[error("edge {0} isn't a span")]
    NotASpan(EdgeId),
    /// Water ingress running past the end of its span.
    #[error("{affected_len_ft} ft from {pos_ft} ft doesn't fit on edge {edge} of {len_ft} ft")]
    WaterOverruns {
        /// The span.
        edge: EdgeId,
        /// Where the affected length starts, feet.
        pos_ft: f64,
        /// The affected length, feet.
        affected_len_ft: f64,
        /// The span's length, feet.
        len_ft: f64,
    },
    /// `amp_id` names something other than an amp.
    #[error("{0} isn't an amp")]
    NotAnAmp(ElementId),
    /// `modem_id` names something other than a modem.
    #[error("{0} isn't a modem")]
    NotAModem(ElementId),
    /// `rpd_id` names something other than an RPD.
    #[error("{0} isn't an RPD")]
    NotAnRpd(ElementId),
    /// `psu_id` names no power supply.
    #[error("{0} isn't a power supply")]
    UnknownSupply(ElementId),
    /// `node_id` names something other than an analog node.
    #[error("{0} isn't an analog node")]
    NotANode(ElementId),
    /// A port named on an element that has none by that name.
    #[error("{element} has no port {port}")]
    NoSuchPort {
        /// The element.
        element: ElementId,
        /// The port.
        port: schema::common::PortId,
    },
    /// An area with a negative or unknown radius.
    #[error("an area of radius {0} ft")]
    BadArea(f64),
}

impl PlantGraph<'_> {
    /// Whether this plant is allowed, as far as topology can say. Empty means no issue found.
    pub fn check_design(&self) -> Vec<DesignIssue> {
        let mut issues = Vec::new();
        let max = self.profile().max_cascade;
        for modem in self.modems() {
            let depth = self.cascade(&modem.id).map_or(0, |amps| amps.len());
            if depth > usize::from(max) {
                issues.push(DesignIssue::CascadeExceeded {
                    modem: modem.id.clone(),
                    depth,
                    max,
                });
            }
        }
        for line in self.plant().parts_list() {
            let qualified = self
                .component(&line.component_ref)
                .is_some_and(|component| qualifies(component, self.profile()));
            if !qualified {
                issues.push(DesignIssue::PartNotQualified {
                    component: line.component_ref,
                });
            }
        }
        let power = self.power();
        issues.extend(
            power
                .unpowered
                .into_iter()
                .map(|element| DesignIssue::Unpowered { element }),
        );
        issues.extend(
            power
                .contested
                .into_iter()
                .map(|element| DesignIssue::ContestedPower { element }),
        );
        issues
    }

    /// Whether every fault refers to something real on this plant, within range.
    pub fn check_faults(&self, faults: &FaultSet) -> Result<(), Vec<FaultRefError>> {
        let errors: Vec<_> = faults
            .faults
            .iter()
            .enumerate()
            .filter_map(|(fault_index, fault)| {
                let problem = self.check_fault(&fault.kind).err()?;
                Some(FaultRefError {
                    fault_index,
                    problem,
                })
            })
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_fault(&self, kind: &FaultKind) -> Result<(), FaultRefProblem> {
        match kind {
            FaultKind::WaterIngress {
                span_id,
                pos_ft,
                affected_len_ft,
                ..
            } => {
                self.resolve(&Location::Span {
                    span_id: span_id.clone(),
                    pos_ft: *pos_ft,
                })?;
                let EdgeKind::Span { len_ft, .. } = self.edge(span_id)?.kind else {
                    return Err(FaultRefProblem::NotASpan(span_id.clone()));
                };
                if *affected_len_ft < 0.0 || pos_ft + affected_len_ft > len_ft {
                    return Err(FaultRefProblem::WaterOverruns {
                        edge: span_id.clone(),
                        pos_ft: *pos_ft,
                        affected_len_ft: *affected_len_ft,
                        len_ft,
                    });
                }
            }
            FaultKind::ConnectorReflection { location, .. }
            | FaultKind::Suckout { location, .. }
            | FaultKind::FrequencySelectiveLoss { location, .. } => {
                self.resolve(location)?;
            }
            FaultKind::UpstreamIngress { entry_location, .. }
            | FaultKind::DownstreamIngress { entry_location, .. } => {
                self.resolve(entry_location)?;
            }
            FaultKind::CommonPathDistortion { location, .. } => {
                self.resolve(location)?;
            }
            FaultKind::AdjacentLevelOffset { .. } | FaultKind::ForwardOptics { .. } => {}
            FaultKind::AmpDrift { amp_id, .. } | FaultKind::ResonantPeak { amp_id, .. } => {
                if !matches!(self.element(amp_id)?.kind, ElementKind::Amp { .. }) {
                    return Err(FaultRefProblem::NotAnAmp(amp_id.clone()));
                }
            }
            FaultKind::Misalignment { active_id, .. } => {
                if !matches!(
                    self.element(active_id)?.kind,
                    ElementKind::Amp { .. } | ElementKind::Node { .. } | ElementKind::Rpd { .. }
                ) {
                    return Err(FaultRefProblem::NotAnAmp(active_id.clone()));
                }
            }
            FaultKind::PowerSupplySag { psu_id, .. } => {
                if !self
                    .plant()
                    .power_supplies
                    .iter()
                    .any(|supply| &supply.id == psu_id)
                {
                    return Err(FaultRefProblem::UnknownSupply(psu_id.clone()));
                }
            }
            FaultKind::BadDrop { modem_id, .. } => {
                if !matches!(self.element(modem_id)?.kind, ElementKind::Modem) {
                    return Err(FaultRefProblem::NotAModem(modem_id.clone()));
                }
            }
            FaultKind::RpdFailure { rpd_id, .. }
            | FaultKind::CinLinkCut { rpd_id, .. }
            | FaultKind::GcpLoss { rpd_id, .. }
            | FaultKind::TimingLoss { rpd_id, .. }
            | FaultKind::RfmMisalignment { rpd_id, .. } => {
                if !matches!(self.element(rpd_id)?.kind, ElementKind::Rpd { .. }) {
                    return Err(FaultRefProblem::NotAnRpd(rpd_id.clone()));
                }
            }
            FaultKind::SpanCut {
                span_id, pos_ft, ..
            }
            | FaultKind::PowerShort {
                span_id, pos_ft, ..
            } => {
                self.resolve(&Location::Span {
                    span_id: span_id.clone(),
                    pos_ft: *pos_ft,
                })?;
            }
            FaultKind::AnalogNodeFailure { node_id, .. } => {
                if !matches!(self.element(node_id)?.kind, ElementKind::Node { .. }) {
                    return Err(FaultRefProblem::NotANode(node_id.clone()));
                }
            }
            FaultKind::WaterInDevice { element_id, .. } => {
                self.element(element_id)?;
            }
            FaultKind::DamagedDevice {
                element_id,
                ports_lost,
                ..
            } => {
                let element = self.element(element_id)?;
                for port in ports_lost {
                    if !element.ports.iter().any(|p| &p.id == port) {
                        return Err(FaultRefProblem::NoSuchPort {
                            element: element_id.clone(),
                            port: port.clone(),
                        });
                    }
                }
            }
            // An area: any place is one, and the sim reads what's inside it.
            FaultKind::UtilityOutage { radius_ft, .. } => {
                if radius_ft.is_nan() || *radius_ft < 0.0 {
                    return Err(FaultRefProblem::BadArea(*radius_ft));
                }
            }
        }
        Ok(())
    }
}

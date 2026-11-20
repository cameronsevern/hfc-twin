//! What the version 2 stages read, resolved once: every part the rules name, from the library, and every rule's value.
//! A value comes from `DesignRules`, never from a constant here.

use schema::common::{ComponentId, ProfileId};
use schema::component::{
    ActiveSpec, CableSpec, Component, ComponentLibrary, ComponentSpec, ConnectorKind,
    ConnectorSpec, CpeDevice, LineEqualizerSpec, PlugInSpec, TapSpec,
};
use schema::design_rules::{
    Complete, DesignRules, LineRole, NodeOptics, PremisesTemplate, RulesError, StationLevels,
    UnusedPort,
};
use schema::profile::{NodeKind, Profile};

use super::DesignError;

/// A tap the rules allow.
#[derive(Debug, Clone)]
pub struct TapPart<'a> {
    /// The part.
    pub part: &'a Component,
    /// Its spec.
    pub spec: &'a TapSpec,
}

/// One family of taps, through values highest first.
#[derive(Debug, Clone)]
pub struct TapFamily<'a> {
    /// Ports.
    pub ports: u8,
    /// Through taps, highest value first.
    pub through: Vec<TapPart<'a>>,
    /// Terminating taps, highest value first.
    pub terminating: Vec<TapPart<'a>>,
}

/// A plug-in the rules allow.
#[derive(Debug, Clone)]
pub struct PlugIn<'a> {
    /// The part.
    pub part: &'a Component,
    /// Its spec.
    pub spec: &'a PlugInSpec,
}

/// A line equalizer the rules allow.
#[derive(Debug, Clone)]
pub struct LineEq<'a> {
    /// The part.
    pub part: &'a Component,
    /// Its spec.
    pub spec: &'a LineEqualizerSpec,
}

/// A cable and the parts that go with it.
#[derive(Debug, Clone)]
pub struct Hardline<'a> {
    /// The part.
    pub part: &'a Component,
    /// Its spec.
    pub spec: &'a CableSpec,
    /// The pin connector at each end.
    pub pin: &'a Component,
    /// The splice at a reel's end, if the rules name one for its size.
    pub splice: Option<&'a Component>,
}

/// An amp the rules name.
#[derive(Debug, Clone)]
pub struct Amp<'a> {
    /// The part.
    pub part: &'a Component,
    /// Its spec.
    pub spec: &'a ActiveSpec,
}

/// Everything the stages read, resolved.
#[derive(Debug, Clone)]
pub struct Kit<'a> {
    /// The rules.
    pub rules: &'a DesignRules,
    /// Their version 2 sections.
    pub complete: Complete<'a>,
    /// The library.
    pub library: &'a ComponentLibrary,
    /// The target profile.
    pub profile: &'a Profile,
    /// The root.
    pub root: &'a Component,
    /// The root's active spec.
    pub root_active: &'a ActiveSpec,
    /// Station levels.
    pub station: &'a StationLevels,
    /// Tap-port target at the top of the band, dBmV.
    pub port_hi_dbmv: f64,
    /// At the bottom, where the rules state one; otherwise the port is held at the top alone.
    pub port_lo_dbmv: Option<f64>,
    /// Either way, dB.
    pub port_tolerance_db: f64,
    /// How far the port's tilt may sit from the target's, dB, where the rules set a limit.
    pub tilt_tolerance_db: Option<f64>,
    /// The last tap's port at least, dBmV at the top.
    pub end_min_dbmv: f64,
    /// Whether a line ends on a terminating tap.
    pub end_terminating: bool,
    /// Tap families, fewest ports first.
    pub taps: Vec<TapFamily<'a>>,
    /// Express cable.
    pub express: Hardline<'a>,
    /// Feeder cable.
    pub feeder: Hardline<'a>,
    /// Express amp.
    pub express_amp: Amp<'a>,
    /// Feeder amp.
    pub feeder_amp: Amp<'a>,
    /// Conditioners, smallest first.
    pub conditioners: Vec<PlugIn<'a>>,
    /// Line equalizers, smallest first.
    pub line_eqs: Vec<LineEq<'a>>,
    /// The splitter.
    pub splitter: &'a Component,
    /// Couplers, lowest value first.
    pub couplers: Vec<&'a Component>,
    /// The terminator.
    pub terminator: &'a Component,
    /// What an unused tap port gets.
    pub unused_port: UnusedPort,
    /// The drop F connector.
    pub drop_f: &'a Component,
    /// Hardline between splices, feet.
    pub reel_len_ft: f64,
    /// The jumper between two housings at one structure, feet.
    pub jumper_len_ft: f64,
    /// The power inserter.
    pub inserter: &'a Component,
    /// Templates allowed on this profile, in the rules' order.
    pub templates: Vec<&'a PremisesTemplate>,
    /// RF ports a node uses.
    pub rf_ports: usize,
    /// Amps in cascade, at most.
    pub max_cascade: usize,
    /// Actives below a node, at most.
    pub max_actives: usize,
    /// The optics.
    pub optics: &'a NodeOptics,
}

fn part<'a>(library: &'a ComponentLibrary, id: &ComponentId) -> Result<&'a Component, DesignError> {
    library
        .get(id)
        .ok_or_else(|| DesignError::MissingPart(id.to_string()))
}

fn active(component: &Component) -> Result<&ActiveSpec, DesignError> {
    component
        .spec
        .active()
        .ok_or_else(|| DesignError::MissingPart(format!("{} isn't an active", component.id)))
}

fn cable_of(component: &Component) -> Result<&CableSpec, DesignError> {
    match &component.spec {
        ComponentSpec::Cable(c) | ComponentSpec::Drop(c) => Ok(c),
        _ => Err(DesignError::MissingPart(format!(
            "{} isn't a cable",
            component.id
        ))),
    }
}

/// The connector of `kind` for a cable family, from the rules' list.
pub fn connector_for<'a>(
    connectors: &[&'a Component],
    kind: ConnectorKind,
    family: Option<&str>,
) -> Option<&'a Component> {
    connectors.iter().copied().find(|c| {
        matches!(&c.spec, ComponentSpec::Connector(ConnectorSpec { kind: k, cable_family, .. })
            if *k == kind && cable_family.as_deref() == family)
    })
}

impl<'a> Kit<'a> {
    /// The target's tilt at the port, `at_hi − at_lo`, where the rules state a level at the bottom of the band.
    pub fn target_tilt(&self) -> Option<f64> {
        self.port_lo_dbmv.map(|lo| self.port_hi_dbmv - lo)
    }

    /// Resolves the rules against the library for `profile`. Fails, naming what's missing, if the rules can't drive the
    /// version 2 designer on this profile.
    pub fn new(
        rules: &'a DesignRules,
        library: &'a ComponentLibrary,
        profile: &'a Profile,
        express_cable: Option<&ComponentId>,
    ) -> Result<Kit<'a>, DesignError> {
        rules.validate(library).map_err(rules_error)?;
        let complete = rules.for_design().map_err(rules_error)?;
        let per_profile = complete.profile(&profile.id).ok_or_else(|| {
            DesignError::MissingPart(format!(
                "rules {} say nothing for profile {}",
                rules.id, profile.id
            ))
        })?;
        let target = complete.tap_port(&profile.id).ok_or_else(|| {
            DesignError::MissingPart(format!(
                "rules {} give no tap-port target for {}",
                rules.id, profile.id
            ))
        })?;
        let root = part(library, &per_profile.root_ref.value)?;
        let root_class_ok = matches!(
            (profile.node_kind, &root.spec),
            (NodeKind::Analog, ComponentSpec::Node(_)) | (NodeKind::Rpd, ComponentSpec::Rpd(_))
        );
        if !root_class_ok || !schema::component::qualifies(root, profile) {
            return Err(DesignError::MissingPart(format!(
                "{} doesn't qualify as the root on {}",
                root.id, profile.id
            )));
        }
        let mut taps = Vec::new();
        for family in &rules.tap_families {
            let mut through = Vec::new();
            let mut terminating = Vec::new();
            for id in family.parts.iter().flat_map(|p| &p.value) {
                let component = part(library, id)?;
                let ComponentSpec::Tap(spec) = &component.spec else {
                    continue;
                };
                if !schema::component::qualifies(component, profile) {
                    continue;
                }
                let tap = TapPart {
                    part: component,
                    spec,
                };
                if spec.terminating {
                    terminating.push(tap);
                } else {
                    through.push(tap);
                }
            }
            let by_value = |a: &TapPart<'_>, b: &TapPart<'_>| {
                b.spec
                    .tap_value_db
                    .total_cmp(&a.spec.tap_value_db)
                    .then(a.part.id.cmp(&b.part.id))
            };
            through.sort_by(by_value);
            terminating.sort_by(by_value);
            if !through.is_empty() {
                taps.push(TapFamily {
                    ports: family.ports,
                    through,
                    terminating,
                });
            }
        }
        taps.sort_by_key(|f| f.ports);
        if taps.is_empty() {
            return Err(DesignError::MissingPart(format!(
                "no tap qualifies on {}",
                profile.id
            )));
        }
        let connectors: Vec<&Component> = complete
            .fittings
            .connectors
            .value
            .iter()
            .map(|id| part(library, id))
            .collect::<Result<_, _>>()?;
        let hardline = |id: &ComponentId| -> Result<Hardline<'a>, DesignError> {
            let component = part(library, id)?;
            let spec = cable_of(component)?;
            let family = spec.family.as_deref();
            let pin = connector_for(&connectors, ConnectorKind::HardlinePin, family)
                .ok_or_else(|| DesignError::MissingPart(format!("a pin connector for {id}")))?;
            let splice = connector_for(&connectors, ConnectorKind::HardlineSplice, family);
            Ok(Hardline {
                part: component,
                spec,
                pin,
                splice,
            })
        };
        let role = |r: LineRole| {
            rules
                .roles
                .iter()
                .find(|x| x.role == r)
                .ok_or_else(|| DesignError::MissingPart(format!("rules for the {r:?} role")))
        };
        let express_rule = role(LineRole::Express)?;
        let feeder_rule = role(LineRole::Feeder)?;
        let express_id = express_cable.unwrap_or(&express_rule.cable_ref.value);
        let amp = |id: &ComponentId| -> Result<Amp<'a>, DesignError> {
            let component = part(library, id)?;
            if !schema::component::qualifies(component, profile) {
                return Err(DesignError::MissingPart(format!(
                    "{id} doesn't qualify on {}",
                    profile.id
                )));
            }
            Ok(Amp {
                part: component,
                spec: active(component)?,
            })
        };
        let mut conditioners: Vec<PlugIn<'a>> = Vec::new();
        for id in &complete.conditioning.conditioners.value {
            let component = part(library, id)?;
            if let ComponentSpec::PlugIn(spec) = &component.spec {
                conditioners.push(PlugIn {
                    part: component,
                    spec,
                });
            }
        }
        // Shaped by a value first, smallest first; those read from a table (the faceplate cable simulators, task 25) after.
        conditioners.sort_by(|a, b| {
            (!a.spec.loss_table.is_empty())
                .cmp(&!b.spec.loss_table.is_empty())
                .then(a.spec.value_db.total_cmp(&b.spec.value_db))
        });
        let mut line_eqs: Vec<LineEq<'a>> = Vec::new();
        for id in &complete.conditioning.line_equalizers.value {
            let component = part(library, id)?;
            if let ComponentSpec::LineEqualizer(spec) = &component.spec {
                line_eqs.push(LineEq {
                    part: component,
                    spec,
                });
            }
        }
        line_eqs.sort_by(|a, b| a.spec.value_db.total_cmp(&b.spec.value_db));
        let mut couplers: Vec<&Component> = complete
            .branching
            .couplers
            .value
            .iter()
            .map(|id| part(library, id))
            .collect::<Result<_, _>>()?;
        couplers.sort_by(|a, b| coupled(a).total_cmp(&coupled(b)));
        let premises = &complete.premises_parts;
        let templates: Vec<&PremisesTemplate> = per_profile
            .premises
            .value
            .iter()
            .filter_map(|id| rules.premises.iter().find(|t| &t.id == id))
            .collect();
        let drop_f = connector_for(&connectors, ConnectorKind::DropF, None)
            .ok_or_else(|| DesignError::MissingPart("a drop F connector".into()))?;
        let inserter_id = &rules
            .powering
            .inserter_ref
            .as_ref()
            .ok_or_else(|| DesignError::MissingPart("powering.inserter_ref".into()))?
            .value;
        Ok(Kit {
            rules,
            complete,
            library,
            profile,
            root,
            root_active: active(root)?,
            station: &per_profile.station,
            port_hi_dbmv: target.at_hi_dbmv.value,
            port_lo_dbmv: target.at_lo_dbmv.as_ref().map(|r| r.value),
            port_tolerance_db: target.tolerance_db.value,
            tilt_tolerance_db: target.tilt_tolerance_db.as_ref().map(|t| t.value),
            end_min_dbmv: rules.end_of_line.min_port_dbmv_at_hi.value,
            end_terminating: rules.end_of_line.last_tap_terminating.value,
            taps,
            express: hardline(express_id)?,
            feeder: hardline(&feeder_rule.cable_ref.value)?,
            express_amp: amp(&express_rule.amp_ref.value)?,
            feeder_amp: amp(&feeder_rule.amp_ref.value)?,
            conditioners,
            line_eqs,
            splitter: part(library, &complete.branching.splitter_ref.value)?,
            couplers,
            terminator: part(library, &premises.terminator_ref.value)?,
            unused_port: premises.unused_port.value,
            drop_f,
            reel_len_ft: complete.fittings.hardline_reel_len_ft.value,
            jumper_len_ft: complete.fittings.jumper_len_ft.value,
            inserter: part(library, inserter_id)?,
            templates,
            rf_ports: rules.node.rf_ports.value.max(1) as usize,
            max_cascade: rules.node.max_cascade.value as usize,
            max_actives: rules
                .node
                .max_actives
                .as_ref()
                .map_or(usize::MAX, |r| r.value as usize),
            optics: &per_profile.optics.value,
        })
    }

    /// The part for a kind of customer device.
    pub fn cpe(&self, device: CpeDevice) -> Option<&'a Component> {
        self.complete
            .premises_parts
            .cpe_refs
            .value
            .iter()
            .find(|c| c.device == device)
            .and_then(|c| self.library.get(&c.component_ref))
    }

    /// The profile it designs for.
    pub fn profile_id(&self) -> &ProfileId {
        &self.profile.id
    }
}

/// A coupler's coupled value, dB.
pub fn coupled(component: &Component) -> f64 {
    match &component.spec {
        ComponentSpec::DirectionalCoupler(c) => c.coupled_value_db,
        _ => 0.0,
    }
}

fn rules_error(errors: Vec<RulesError>) -> DesignError {
    DesignError::Rules(
        errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; "),
    )
}

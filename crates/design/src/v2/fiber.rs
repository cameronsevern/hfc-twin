//! Stage `fiber`: the fiber plant from the hub to every node (`docs/physics.md` §16, fiber-optics.md). The route is the
//! router task 14 built, a Steiner tree over streets, grown here over the structures so the poles the fiber passes are
//! the coax plant's own. Sheaths are sized along it from the rules' fibers per node and standard counts; closures go at
//! branches, at a node the route passes on its way, and at a reel's end. Each node gets a link each way with its own
//! strands. Per link, the transmitter, any hub coupler and the receivers are the first combination, in the rules' order,
//! that `sim::optics` reads inside each receiver's window with the rules' margin, with the forward composite index under
//! the rules' clear-of-clipping limit at the profile's loading and the return laser under its clip.

use std::collections::{BTreeMap, BTreeSet};

use schema::common::{
    ComponentId, FiberCableId, FiberElementId, HubId, LinkId, LonLat, PortId, ServiceGroupId,
};
use schema::component::{ComponentSpec, OpticalDirection};
use schema::design_rules::NodeOptics;
use schema::fiber::{
    FiberCable, FiberElement, FiberElementKind, FiberPlant, Hop, Hub, OpticalLink,
    ServiceGroupBinding,
};
use schema::plant::{Fitted, Installation, Plant, RecordStatus, RoutePoint};
use sim::optics::{forward_channels, link_issues, node_link};

use super::DesignError;
use super::kit::Kit;
use super::nodes::street_graph;
use super::structures::Structures;
use crate::geo::dist_ft;
use crate::route::steiner_tree;

/// The parts a node's links use, chosen.
#[derive(Debug, Clone, PartialEq)]
pub enum Optics {
    /// Analog.
    Analog {
        /// Forward transmitter.
        tx: ComponentId,
        /// Hub coupler, if any.
        coupler: Option<ComponentId>,
        /// Hub amplifier after the transmitter, if any.
        amp: Option<ComponentId>,
        /// The node's receiver.
        rx: ComponentId,
        /// The node's return laser.
        rtx: ComponentId,
        /// The hub's return receiver.
        rrx: ComponentId,
    },
    /// Digital: the optic at both ends.
    Digital {
        /// The optic.
        optic: ComponentId,
    },
}

/// What the fiber stage built.
#[derive(Debug, Clone)]
pub struct FiberDesign {
    /// The fiber plant.
    pub fiber: FiberPlant,
    /// Each node's plant with its optics fitted on its root.
    pub plants: Vec<Plant>,
    /// Per node, the optics chosen, or why none fit.
    pub choices: BTreeMap<ServiceGroupId, Result<Optics, String>>,
    /// Structures the fiber uses, by index.
    pub sites: BTreeSet<usize>,
    /// Per node, the runs its fibers follow.
    pub runs: BTreeMap<ServiceGroupId, BTreeSet<usize>>,
}

struct Builder<'k, 'a> {
    kit: &'k Kit<'a>,
    s: &'k Structures,
    hub_geo: LonLat,
    elements: Vec<FiberElement>,
    cables: Vec<FiberCable>,
    links: Vec<OpticalLink>,
    sites: BTreeSet<usize>,
    counters: BTreeMap<&'static str, usize>,
}

impl Builder<'_, '_> {
    fn next(&mut self, kind: &'static str) -> usize {
        let n = self.counters.entry(kind).or_insert(0);
        *n += 1;
        *n
    }

    fn element(
        &mut self,
        id: &str,
        kind: FiberElementKind,
        part: &ComponentId,
        site: Option<usize>,
        ports: Vec<&str>,
    ) -> FiberElementId {
        let id = FiberElementId::new(id);
        let (geo, hub, structure) = match site {
            Some(s) => {
                self.sites.insert(s);
                (self.s.sites[s].geo, None, Some(self.s.sites[s].id.clone()))
            }
            None => (self.hub_geo, Some(HubId::new("hub-1")), None),
        };
        self.elements.push(FiberElement {
            id: id.clone(),
            kind,
            component_ref: part.clone(),
            geo: Some(geo),
            hub,
            structure,
            ports: ports.into_iter().map(PortId::new).collect(),
            record_status: RecordStatus::Designed,
        });
        id
    }

    fn jumper(
        &mut self,
        part: &ComponentId,
        from: &FiberElementId,
        to: &FiberElementId,
    ) -> FiberCableId {
        let n = self.next("jumper");
        let id = FiberCableId::new(format!("jumper-{n}"));
        let len = self
            .kit
            .rules
            .fiber
            .jumper_len_ft
            .as_ref()
            .map_or(1.0, |r| r.value);
        self.cables.push(FiberCable {
            id: id.clone(),
            component_ref: part.clone(),
            from: from.clone(),
            to: to.clone(),
            len_ft: len,
            installation: Installation::Underground,
            path: Vec::new(),
            support: None,
            record_status: RecordStatus::Designed,
        });
        id
    }
}

/// Builds the fiber plant for `nodes` (each a plant whose root is at `sites[i]`), and fits each root's optics.
pub fn fiber(
    kit: &Kit<'_>,
    s: &Structures,
    hub_geo: LonLat,
    region_id: &str,
    nodes: &[(Plant, usize)],
) -> Result<FiberDesign, DesignError> {
    let rules = &kit.rules.fiber;
    let need = |what: &str| DesignError::Rules(format!("fiber.{what}"));
    let closure_part = rules
        .closure_ref
        .as_ref()
        .ok_or_else(|| need("closure_ref"))?
        .value
        .clone();
    let panel_part = rules
        .panel_ref
        .as_ref()
        .ok_or_else(|| need("panel_ref"))?
        .value
        .clone();
    let connector = rules
        .connector_ref
        .as_ref()
        .ok_or_else(|| need("connector_ref"))?
        .value
        .clone();
    let jumper_part = rules
        .jumper_ref
        .as_ref()
        .ok_or_else(|| need("jumper_ref"))?
        .value
        .clone();
    let duplex_part = rules
        .duplex_jumper_ref
        .as_ref()
        .ok_or_else(|| need("duplex_jumper_ref"))?
        .value
        .clone();
    let index_max = rules
        .composite_index_max
        .as_ref()
        .ok_or_else(|| need("composite_index_max"))?
        .value;
    let margin = rules.link_margin_db.value;
    let digital = matches!(kit.optics, NodeOptics::Digital { .. });
    let per_node = if digital {
        rules.fibers_per_rpd.value
    } else {
        rules.fibers_per_analog_node.value
    }
    .max(2);
    // Sheath parts by count, smallest first.
    let mut sheaths: Vec<(u32, ComponentId)> = rules
        .cable_refs
        .iter()
        .flat_map(|r| &r.value)
        .filter_map(|id| match kit.library.get(id).map(|c| &c.spec) {
            Some(ComponentSpec::FiberCable(c)) => Some((c.fiber_count, id.clone())),
            _ => None,
        })
        .collect();
    sheaths.sort();
    let sheath_for = |fibers: u32| -> Result<(u32, ComponentId), DesignError> {
        sheaths
            .iter()
            .find(|(n, _)| *n >= fibers)
            .cloned()
            .ok_or_else(|| {
                DesignError::MissingPart(format!("a fiber cable of {fibers} fibers or more"))
            })
    };

    let mut b = Builder {
        kit,
        s,
        hub_geo,
        elements: Vec::new(),
        cables: Vec::new(),
        links: Vec::new(),
        sites: BTreeSet::new(),
        counters: BTreeMap::new(),
    };
    // The route: Steiner over the structures from the hub's structure to every node site.
    let hub_site = s.nearest(hub_geo).ok_or(DesignError::NoStreets)?;
    let node_sites: Vec<usize> = nodes.iter().map(|(_, site)| *site).collect();
    let graph = street_graph(s);
    let tree = steiner_tree(&graph, hub_site, &node_sites);
    let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (&v, &(p, _, _)) in &tree.parent {
        children.entry(p).or_default().push(v);
    }
    for list in children.values_mut() {
        list.sort_unstable();
    }
    // Prune what leads to no node.
    let mut at_site: BTreeMap<usize, Vec<ServiceGroupId>> = BTreeMap::new();
    for (plant, site) in nodes {
        at_site
            .entry(*site)
            .or_default()
            .push(plant.service_group.clone());
    }
    fn below(
        v: usize,
        children: &BTreeMap<usize, Vec<usize>>,
        at_site: &BTreeMap<usize, Vec<ServiceGroupId>>,
        out: &mut BTreeMap<usize, Vec<ServiceGroupId>>,
    ) -> Vec<ServiceGroupId> {
        let mut groups: Vec<ServiceGroupId> = at_site.get(&v).cloned().unwrap_or_default();
        for c in children.get(&v).into_iter().flatten() {
            groups.extend(below(*c, children, at_site, out));
        }
        out.insert(v, groups.clone());
        groups
    }
    let mut served_below: BTreeMap<usize, Vec<ServiceGroupId>> = BTreeMap::new();
    below(hub_site, &children, &at_site, &mut served_below);
    for list in children.values_mut() {
        list.retain(|c| served_below.get(c).is_some_and(|g| !g.is_empty()));
    }

    // The hub.
    let panel_ports = match kit.library.get(&panel_part).map(|c| &c.spec) {
        Some(ComponentSpec::PatchPanel(p)) => p.ports.max(1),
        _ => 24,
    };
    let panels_needed = ((nodes.len() as u32 * per_node).div_ceil(panel_ports)).max(1);
    let mut panels = Vec::new();
    for k in 1..=panels_needed {
        panels.push(b.element(
            &format!("panel-{k}"),
            FiberElementKind::PatchPanel,
            &panel_part,
            None,
            Vec::new(),
        ));
    }
    let panel_for = |i: usize| panels[(i * per_node as usize) / panel_ports as usize].clone();

    // Sheaths and closures down the route: depth first, a new sheath wherever the route branches or reaches a node, or
    // the reel runs out.
    let reel = rules.reel_len_ft.value;
    // Per service group, the path of (cable, strand) and splice closures from the hub's panel to the node end.
    let mut paths: BTreeMap<ServiceGroupId, Vec<(FiberCableId, u32, Option<FiberElementId>)>> =
        BTreeMap::new();
    let mut node_ends: BTreeMap<ServiceGroupId, FiberElementId> = BTreeMap::new();
    struct Pending {
        from: FiberElementId,
        from_site: Option<usize>,
        runs: Vec<usize>,
        at: usize,
        groups: Vec<ServiceGroupId>,
        len_ft: f64,
    }
    let order: Vec<ServiceGroupId> = nodes.iter().map(|(p, _)| p.service_group.clone()).collect();
    let mut fiber_runs: BTreeMap<ServiceGroupId, BTreeSet<usize>> = BTreeMap::new();
    let mut work = vec![Pending {
        from: panels[0].clone(),
        from_site: None,
        runs: Vec::new(),
        at: hub_site,
        groups: served_below.get(&hub_site).cloned().unwrap_or_default(),
        len_ft: dist_ft(hub_geo, s.sites[hub_site].geo),
    }];
    while let Some(mut p) = work.pop() {
        let v = p.at;
        let kids = children.get(&v).cloned().unwrap_or_default();
        let here = at_site.get(&v).cloned().unwrap_or_default();
        let ends_here = kids.is_empty();
        let reel_end = p.len_ft >= reel;
        // A sheath runs on until the route reaches a node, branches, or comes to a reel's end.
        if here.is_empty() && kids.len() == 1 && !reel_end {
            let c = kids[0];
            let r = s.run_between(v, c).unwrap_or(0);
            p.runs.push(r);
            p.len_ft += s.runs[r].len_ft;
            p.at = c;
            work.push(p);
            continue;
        }
        // The sheath from `p.from` ends here, at a closure or the node's end.
        let fibers = p.groups.len() as u32 * per_node;
        let (count, part) = sheath_for(fibers)?;
        let to_is_end = ends_here && here.len() == 1 && !reel_end;
        let to = if to_is_end {
            let sg = here[0].clone();
            let root_part = nodes
                .iter()
                .find(|(pl, _)| pl.service_group == sg)
                .and_then(|(pl, _)| pl.root().map(|r| r.component_ref.clone()))
                .unwrap_or_else(|| kit.root.id.clone());
            let id = b.element(
                &format!("node-end-{sg}"),
                FiberElementKind::NodeEnd {
                    service_group: sg.clone(),
                },
                &root_part,
                Some(v),
                Vec::new(),
            );
            node_ends.insert(sg, id.clone());
            id
        } else {
            let n = b.next("closure");
            b.element(
                &format!("closure-{n}"),
                FiberElementKind::SpliceClosure,
                &closure_part,
                Some(v),
                Vec::new(),
            )
        };
        let n = b.next("fiber");
        let cable_id = FiberCableId::new(format!("fiber-{n}"));
        let (path, installation) = {
            let mut out = Vec::new();
            let mut at = match p.from_site {
                Some(site) => site,
                None => {
                    // From the hub: its own structure first.
                    let site = &s.sites[hub_site];
                    b.sites.insert(hub_site);
                    out.push(RoutePoint {
                        geo: site.geo,
                        structure: Some(site.id.clone()),
                    });
                    hub_site
                }
            };
            let (mut aerial, mut buried) = (0.0, 0.0);
            for (i, &r) in p.runs.iter().enumerate() {
                let run = &s.runs[r];
                for q in run.path_from(at) {
                    out.push(RoutePoint {
                        geo: q,
                        structure: None,
                    });
                }
                at = run.other(at);
                if i + 1 < p.runs.len() {
                    b.sites.insert(at);
                    out.push(RoutePoint {
                        geo: s.sites[at].geo,
                        structure: Some(s.sites[at].id.clone()),
                    });
                }
                match run.installation {
                    Installation::Aerial => aerial += run.len_ft,
                    Installation::Underground => buried += run.len_ft,
                }
            }
            (
                out,
                if buried > aerial {
                    Installation::Underground
                } else {
                    Installation::Aerial
                },
            )
        };
        let from_geo = match p.from_site {
            Some(site) => s.sites[site].geo,
            None => hub_geo,
        };
        let len_ft = super::fittings::path_len_ft(from_geo, &path, s.sites[v].geo).max(1e-3);
        b.cables.push(FiberCable {
            id: cable_id.clone(),
            component_ref: part,
            from: p.from.clone(),
            to: to.clone(),
            len_ft,
            installation,
            path,
            support: Some(super::fittings::support(kit, installation)),
            record_status: RecordStatus::Designed,
        });
        b.sites.insert(v);
        // Strands: each group its fibers, in the region's node order.
        for g in &p.groups {
            fiber_runs
                .entry(g.clone())
                .or_default()
                .extend(p.runs.iter().copied());
        }
        let mut carries: Vec<ServiceGroupId> = p.groups.clone();
        carries.sort_by_key(|g| order.iter().position(|o| o == g));
        for (i, g) in carries.iter().enumerate() {
            let closure = (!to_is_end).then(|| to.clone());
            paths.entry(g.clone()).or_default().push((
                cable_id.clone(),
                i as u32 * per_node + 1,
                closure,
            ));
        }
        let _ = count;
        if to_is_end {
            continue;
        }
        // From the closure: a node here takes a jumper to its end, and every child its own sheath.
        for sg in &here {
            let root_part = nodes
                .iter()
                .find(|(pl, _)| &pl.service_group == sg)
                .and_then(|(pl, _)| pl.root().map(|r| r.component_ref.clone()))
                .unwrap_or_else(|| kit.root.id.clone());
            let end = b.element(
                &format!("node-end-{sg}"),
                FiberElementKind::NodeEnd {
                    service_group: sg.clone(),
                },
                &root_part,
                Some(v),
                Vec::new(),
            );
            let jumper = b.jumper(&jumper_part, &to, &end);
            // A node's own tail: a fiber each way on a two-fiber jumper would need two; one jumper per strand.
            let jumper2 = b.jumper(&jumper_part, &to, &end);
            paths.entry(sg.clone()).or_default().push((jumper, 1, None));
            paths
                .entry(sg.clone())
                .or_default()
                .push((jumper2, 1, None));
            node_ends.insert(sg.clone(), end);
        }
        for c in kids.iter().rev() {
            let r = s.run_between(v, *c).unwrap_or(0);
            work.push(Pending {
                from: to.clone(),
                from_site: Some(v),
                runs: vec![r],
                at: *c,
                groups: served_below.get(c).cloned().unwrap_or_default(),
                len_ft: s.runs[r].len_ft,
            });
        }
    }

    // Hub equipment, links and bindings, node by node, choosing optics.
    let mut plants = Vec::new();
    let mut choices = BTreeMap::new();
    let mut bindings = Vec::new();
    let timing_and_switch = if let NodeOptics::Digital {
        core_ref,
        switch_ref,
        timing_ref,
        ..
    } = kit.optics
    {
        let core = b.element("core-1", FiberElementKind::Cmts, core_ref, None, Vec::new());
        let ports: Vec<String> = (1..=nodes.len()).map(|i| format!("port-{i}")).collect();
        let switch = b.element(
            "switch-1",
            FiberElementKind::Switch,
            switch_ref,
            None,
            ports.iter().map(String::as_str).collect(),
        );
        b.element(
            "timing-1",
            FiberElementKind::TimingSource,
            timing_ref,
            None,
            Vec::new(),
        );
        Some((core, switch))
    } else {
        None
    };
    let cmts = if let NodeOptics::Analog { cmts_ref, .. } = kit.optics {
        Some(b.element("cmts-1", FiberElementKind::Cmts, cmts_ref, None, Vec::new()))
    } else {
        None
    };
    for (i, (plant, _)) in nodes.iter().enumerate() {
        let sg = plant.service_group.clone();
        let Some(end) = node_ends.get(&sg).cloned() else {
            choices.insert(sg, Err("no route to the hub".to_string()));
            plants.push(plant.clone());
            continue;
        };
        let route = paths.get(&sg).cloned().unwrap_or_default();
        let panel = panel_for(i);
        // The strands: forward on the first of the node's fibers, return on the second.
        let outside: Vec<(FiberCableId, u32, Option<FiberElementId>)> = route
            .iter()
            .filter(|(c, _, _)| c.as_str().starts_with("fiber-"))
            .cloned()
            .collect();
        let tails: Vec<FiberCableId> = route
            .iter()
            .filter(|(c, _, _)| c.as_str().starts_with("jumper-"))
            .map(|(c, _, _)| c.clone())
            .collect();
        let outside_hops = |strand_offset: u32| -> Vec<Hop> {
            let mut hops = Vec::new();
            for (k, (cable, strand, closure)) in outside.iter().enumerate() {
                hops.push(Hop::Fiber {
                    cable: cable.clone(),
                    strand: strand + strand_offset,
                });
                if let Some(c) = closure
                    && (k + 1 < outside.len() || !tails.is_empty())
                {
                    hops.push(Hop::Splice {
                        at: c.clone(),
                        loss_db: None,
                    });
                }
            }
            if let Some(t) = tails.get(strand_offset as usize) {
                hops.push(Hop::Fiber {
                    cable: t.clone(),
                    strand: 1,
                });
            }
            hops
        };
        let conn = |at: &FiberElementId| Hop::Connector {
            at: at.clone(),
            component_ref: connector.clone(),
        };
        match kit.optics {
            NodeOptics::Analog {
                forward_tx,
                hub_couplers,
                hub_amps,
                node_rx,
                return_tx,
                hub_rx,
                ..
            } => {
                // Try each combination, in the rules' order, on a trial plant of this node's links alone.
                let mut chosen: Option<Chosen> = None;
                let couplers: Vec<Option<&ComponentId>> = std::iter::once(None)
                    .chain(hub_couplers.iter().map(Some))
                    .collect();
                // An amplifier only after a transmitter in its band.
                let amps_for = |tx: &ComponentId| -> Vec<Option<&ComponentId>> {
                    let w = wavelength(kit, tx);
                    std::iter::once(None)
                        .chain(
                            hub_amps
                                .iter()
                                .filter(|a| match kit.library.get(a).map(|c| &c.spec) {
                                    Some(ComponentSpec::OpticalAmp(s)) => {
                                        w >= s.band_lo_nm && w <= s.band_hi_nm
                                    }
                                    _ => false,
                                })
                                .map(Some),
                        )
                        .collect()
                };
                let mut why = String::from("no optics in the rules");
                'search: for tx in forward_tx {
                    for amp in amps_for(tx) {
                        for coupler in &couplers {
                            for rx in node_rx {
                                for rtx in return_tx {
                                    for rrx in hub_rx {
                                        let mut t = Builder {
                                            kit,
                                            s,
                                            hub_geo,
                                            elements: Vec::new(),
                                            cables: Vec::new(),
                                            links: Vec::new(),
                                            sites: BTreeSet::new(),
                                            counters: b.counters.clone(),
                                        };
                                        let ftx = t.element(
                                            &format!("ftx-{sg}"),
                                            FiberElementKind::OpticalTx,
                                            tx,
                                            None,
                                            Vec::new(),
                                        );
                                        let rrx_id = t.element(
                                            &format!("rrx-{sg}"),
                                            FiberElementKind::OpticalRx,
                                            rrx,
                                            None,
                                            Vec::new(),
                                        );
                                        let mut fwd = vec![conn(&ftx)];
                                        // The amplifier, if any, between the transmitter and what follows.
                                        let mut src = ftx.clone();
                                        if let Some(a) = amp {
                                            let e = t.element(
                                                &format!("oamp-{sg}"),
                                                FiberElementKind::OpticalAmp,
                                                a,
                                                None,
                                                vec!["in", "out"],
                                            );
                                            let ja = t.jumper(&jumper_part, &src, &e);
                                            fwd.extend([
                                                Hop::Fiber {
                                                    cable: ja,
                                                    strand: 1,
                                                },
                                                conn(&e),
                                                Hop::Through {
                                                    element: e.clone(),
                                                    port_in: PortId::new("in"),
                                                    port_out: PortId::new("out"),
                                                },
                                                conn(&e),
                                            ]);
                                            src = e;
                                        }
                                        match coupler {
                                            Some(c) => {
                                                let split = t.element(
                                                    &format!("osplit-{sg}"),
                                                    FiberElementKind::OpticalPassive,
                                                    c,
                                                    None,
                                                    vec!["in", "out-1"],
                                                );
                                                let j1 = t.jumper(&jumper_part, &src, &split);
                                                let j2 = t.jumper(&jumper_part, &split, &panel);
                                                fwd.extend([
                                                    Hop::Fiber {
                                                        cable: j1,
                                                        strand: 1,
                                                    },
                                                    conn(&split),
                                                    Hop::Through {
                                                        element: split.clone(),
                                                        port_in: PortId::new("in"),
                                                        port_out: PortId::new("out-1"),
                                                    },
                                                    conn(&split),
                                                    Hop::Fiber {
                                                        cable: j2,
                                                        strand: 1,
                                                    },
                                                ]);
                                            }
                                            None => {
                                                let j = t.jumper(&jumper_part, &src, &panel);
                                                fwd.push(Hop::Fiber {
                                                    cable: j,
                                                    strand: 1,
                                                });
                                            }
                                        }
                                        fwd.push(conn(&panel));
                                        fwd.extend(outside_hops(0));
                                        fwd.push(conn(&end));
                                        let jr = t.jumper(&jumper_part, &panel, &rrx_id);
                                        let mut ret = vec![conn(&end)];
                                        let mut back = outside_hops(1);
                                        back.reverse();
                                        ret.extend(back);
                                        ret.extend([
                                            conn(&panel),
                                            Hop::Fiber {
                                                cable: jr,
                                                strand: 1,
                                            },
                                            conn(&rrx_id),
                                        ]);
                                        let links = vec![
                                            OpticalLink {
                                                id: LinkId::new(format!("link-{sg}-fwd")),
                                                service_group: sg.clone(),
                                                direction: OpticalDirection::Forward,
                                                wavelength_nm: wavelength(kit, tx),
                                                hops: fwd,
                                            },
                                            OpticalLink {
                                                id: LinkId::new(format!("link-{sg}-ret")),
                                                service_group: sg.clone(),
                                                direction: OpticalDirection::Return,
                                                wavelength_nm: wavelength(kit, rtx),
                                                hops: ret,
                                            },
                                        ];
                                        let mut fitted_plant = plant.clone();
                                        fit_root(
                                            &mut fitted_plant,
                                            Some(rx.clone()),
                                            Some(rtx.clone()),
                                            Vec::new(),
                                        );
                                        let mut trial = FiberPlant {
                                            schema_version: schema::fiber::SCHEMA_VERSION
                                                .to_string(),
                                            id: format!("fiber-{region_id}"),
                                            origin: schema::common::Origin::Synthetic,
                                            hubs: vec![Hub {
                                                id: HubId::new("hub-1"),
                                                geo: hub_geo,
                                                record_status: RecordStatus::Designed,
                                            }],
                                            elements: b.elements.clone(),
                                            cables: b.cables.clone(),
                                            links: links.clone(),
                                            bindings: cmts
                                                .iter()
                                                .map(|c| ServiceGroupBinding {
                                                    service_group: sg.clone(),
                                                    cmts: c.clone(),
                                                    port: PortId::new(format!("sg-{}", i + 1)),
                                                })
                                                .collect(),
                                            structures: Vec::new(),
                                        };
                                        trial.elements.extend(t.elements.iter().cloned());
                                        trial.cables.extend(t.cables.iter().cloned());
                                        match judge(
                                            kit,
                                            &trial,
                                            &fitted_plant,
                                            margin,
                                            index_max,
                                            tx,
                                        ) {
                                            Ok(()) => {
                                                chosen = Some((
                                                    Optics::Analog {
                                                        tx: tx.clone(),
                                                        coupler: coupler.cloned(),
                                                        amp: amp.cloned(),
                                                        rx: rx.clone(),
                                                        rtx: rtx.clone(),
                                                        rrx: rrx.clone(),
                                                    },
                                                    t.elements,
                                                    t.cables,
                                                    links,
                                                    fitted_plant,
                                                ));
                                                b.counters = t.counters;
                                                break 'search;
                                            }
                                            Err(e) => why = e,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                match chosen {
                    Some((optics, elements, cables, links, fitted_plant)) => {
                        b.elements.extend(elements);
                        b.cables.extend(cables);
                        b.links.extend(links);
                        plants.push(fitted_plant);
                        choices.insert(sg.clone(), Ok(optics));
                    }
                    None => {
                        plants.push(plant.clone());
                        choices.insert(sg.clone(), Err(why));
                    }
                }
                if let Some(c) = &cmts {
                    bindings.push(ServiceGroupBinding {
                        service_group: sg.clone(),
                        cmts: c.clone(),
                        port: PortId::new(format!("sg-{}", i + 1)),
                    });
                }
            }
            NodeOptics::Digital { transceivers, .. } => {
                let mut chosen = None;
                let mut why = String::from("no optics in the rules");
                for optic in transceivers {
                    let mut t = Builder {
                        kit,
                        s,
                        hub_geo,
                        elements: Vec::new(),
                        cables: Vec::new(),
                        links: Vec::new(),
                        sites: BTreeSet::new(),
                        counters: b.counters.clone(),
                    };
                    let sfp = t.element(
                        &format!("sfp-{sg}"),
                        FiberElementKind::Transceiver,
                        optic,
                        None,
                        Vec::new(),
                    );
                    let j = t.jumper(&duplex_part, &sfp, &panel);
                    let mut fwd = vec![
                        conn(&sfp),
                        Hop::Fiber {
                            cable: j.clone(),
                            strand: 1,
                        },
                        conn(&panel),
                    ];
                    fwd.extend(outside_hops(0));
                    fwd.push(conn(&end));
                    let mut ret = vec![conn(&end)];
                    let mut back = outside_hops(1);
                    back.reverse();
                    ret.extend(back);
                    ret.extend([
                        conn(&panel),
                        Hop::Fiber {
                            cable: j,
                            strand: 2,
                        },
                        conn(&sfp),
                    ]);
                    let w = wavelength(kit, optic);
                    let links = vec![
                        OpticalLink {
                            id: LinkId::new(format!("link-{sg}-fwd")),
                            service_group: sg.clone(),
                            direction: OpticalDirection::Forward,
                            wavelength_nm: w,
                            hops: fwd,
                        },
                        OpticalLink {
                            id: LinkId::new(format!("link-{sg}-ret")),
                            service_group: sg.clone(),
                            direction: OpticalDirection::Return,
                            wavelength_nm: w,
                            hops: ret,
                        },
                    ];
                    let mut fitted_plant = plant.clone();
                    fit_root(&mut fitted_plant, None, None, vec![optic.clone()]);
                    let mut trial = FiberPlant {
                        schema_version: schema::fiber::SCHEMA_VERSION.to_string(),
                        id: format!("fiber-{region_id}"),
                        origin: schema::common::Origin::Synthetic,
                        hubs: vec![Hub {
                            id: HubId::new("hub-1"),
                            geo: hub_geo,
                            record_status: RecordStatus::Designed,
                        }],
                        elements: b.elements.clone(),
                        cables: b.cables.clone(),
                        links: links.clone(),
                        bindings: Vec::new(),
                        structures: Vec::new(),
                    };
                    trial.elements.extend(t.elements.iter().cloned());
                    trial.cables.extend(t.cables.iter().cloned());
                    match judge(kit, &trial, &fitted_plant, margin, index_max, optic) {
                        Ok(()) => {
                            chosen = Some((
                                Optics::Digital {
                                    optic: optic.clone(),
                                },
                                t.elements,
                                t.cables,
                                links,
                                fitted_plant,
                            ));
                            b.counters = t.counters;
                            break;
                        }
                        Err(e) => why = e,
                    }
                }
                match chosen {
                    Some((optics, elements, cables, links, fitted_plant)) => {
                        b.elements.extend(elements);
                        b.cables.extend(cables);
                        b.links.extend(links);
                        plants.push(fitted_plant);
                        choices.insert(sg.clone(), Ok(optics));
                    }
                    None => {
                        plants.push(plant.clone());
                        choices.insert(sg.clone(), Err(why));
                    }
                }
                if let Some((core, _)) = &timing_and_switch {
                    bindings.push(ServiceGroupBinding {
                        service_group: sg.clone(),
                        cmts: core.clone(),
                        port: PortId::new(format!("sg-{}", i + 1)),
                    });
                }
            }
        }
    }
    // Ports a link passes through or a binding names.
    if let Some(c) = &cmts
        && let Some(e) = b.elements.iter_mut().find(|e| &e.id == c)
    {
        e.ports = bindings.iter().map(|x| x.port.clone()).collect();
    }
    if let Some((core, _)) = &timing_and_switch
        && let Some(e) = b.elements.iter_mut().find(|e| &e.id == core)
    {
        e.ports = bindings.iter().map(|x| x.port.clone()).collect();
    }
    let fiber = FiberPlant {
        schema_version: schema::fiber::SCHEMA_VERSION.to_string(),
        id: format!("fiber-{region_id}"),
        origin: schema::common::Origin::Synthetic,
        hubs: vec![Hub {
            id: HubId::new("hub-1"),
            geo: hub_geo,
            record_status: RecordStatus::Designed,
        }],
        elements: b.elements,
        cables: b.cables,
        links: b.links,
        bindings,
        structures: s.records(b.sites.iter().copied()),
    };
    Ok(FiberDesign {
        fiber,
        plants,
        choices,
        sites: b.sites,
        runs: fiber_runs,
    })
}

fn wavelength(kit: &Kit<'_>, part: &ComponentId) -> f64 {
    match kit.library.get(part).map(|c| &c.spec) {
        Some(ComponentSpec::OpticalTx(t)) => t.wavelength_nm,
        Some(ComponentSpec::Transceiver(t)) => t.wavelength_nm,
        _ => 1310.0,
    }
}

/// Fits a root's optics.
fn fit_root(
    plant: &mut Plant,
    rx: Option<ComponentId>,
    rtx: Option<ComponentId>,
    optics: Vec<ComponentId>,
) {
    let Some(root) = plant.root().map(|r| r.id.clone()) else {
        return;
    };
    if let Some(element) = plant.elements.iter_mut().find(|e| e.id == root)
        && let Some(Fitted::Active(f)) = &mut element.fitted
    {
        f.forward_rx = rx;
        f.return_tx = rtx;
        f.transceivers = optics;
    }
}

/// One node's optics as chosen: what they are, the hub elements, cables and links they add, and the node's plant fitted
/// to them.
type Chosen = (
    Optics,
    Vec<FiberElement>,
    Vec<FiberCable>,
    Vec<OpticalLink>,
    Plant,
);

/// A node's forward route in words: its length and what it passes, for the note that says why no optics fit.
fn route(fiber: &FiberPlant, plant: &Plant) -> String {
    let Some(link) = fiber.links.iter().find(|l| {
        l.service_group == plant.service_group && l.direction == OpticalDirection::Forward
    }) else {
        return "no route".into();
    };
    let (mut ft, mut splices, mut connectors) = (0.0, 0, 0);
    for hop in &link.hops {
        match hop {
            Hop::Fiber { cable, .. } => {
                ft += fiber
                    .cables
                    .iter()
                    .find(|c| &c.id == cable)
                    .map_or(0.0, |c| c.len_ft)
            }
            Hop::Splice { .. } => splices += 1,
            Hop::Connector { .. } => connectors += 1,
            Hop::Through { .. } => {}
        }
    }
    format!(
        "{:.1} km, {splices} splices, {connectors} connectors",
        ft * 0.000_304_8
    )
}

/// Whether a node's links pass: each received at least the rules' margin over its receiver's minimum and no more than its
/// maximum, the forward composite
/// index under the rules' limit at the profile's loading, and no `link_issues`.
fn judge(
    kit: &Kit<'_>,
    fiber: &FiberPlant,
    plant: &Plant,
    margin: f64,
    index_max: f64,
    tx: &ComponentId,
) -> Result<(), String> {
    let plan = &kit.profile.channel_plan;
    let link = node_link(fiber, plant, kit.library, plan)
        .map_err(|e| e.to_string())?
        .ok_or("no link")?;
    let issues = link_issues(&link, kit.library, plan);
    if !issues.is_empty() {
        return Err(format!("{issues:?} over {}", route(fiber, plant)));
    }
    for one in [&link.forward, &link.ret].into_iter().flatten() {
        let window = match kit.library.get(&one.receiver).map(|c| &c.spec) {
            Some(ComponentSpec::OpticalRx(rx)) => (rx.in_min_dbm, rx.in_max_dbm),
            Some(ComponentSpec::Transceiver(rx)) => (rx.rx_sensitivity_dbm, rx.rx_overload_dbm),
            _ => return Err(format!("{} receives nothing", one.receiver)),
        };
        let p = one.received_dbm.ok_or("no light")?;
        // The margin is headroom for repairs and ageing, which only lose light (fiber-optics.md §3); the top is the
        // receiver's own limit.
        if p < window.0 + margin || p > window.1 {
            return Err(format!(
                "{}: {p:.2} dBm against {:.1} + {margin} dB to {:.1}, over {}",
                one.link,
                window.0,
                window.1,
                route(fiber, plant)
            ));
        }
    }
    if let Some(ComponentSpec::OpticalTx(t)) = kit.library.get(tx).map(|c| &c.spec) {
        let index = t.omi_pct / 100.0 * forward_channels(plan).sqrt();
        if index > index_max {
            return Err(format!("{tx}: composite index {index:.3} over {index_max}"));
        }
    }
    Ok(())
}

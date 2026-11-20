//! Task 26's world on a small generated region: a fold, a dispatcher that never reads the faults, calls from what modems
//! show, trucks that find what's in reach. All synthetic.

use std::fs;

use design::v2::design;
use ops::{Action, NodeInput, Pair, Policy, Scenarios, Standing, World, WorldInputs};
use schema::common::{EdgeId, LonLat};
use schema::component::ComponentLibrary;
use schema::design_rules::DesignRules;
use schema::fault::{Fault, FaultKind, Location, Onset};
use schema::ops::{OpsRates, Outcome};
use schema::profile::Profile;
use schema::region::{DesignControls, LayoutFamily};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    let full = format!("{}/../../fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&fs::read_to_string(full).unwrap()).unwrap()
}

fn inputs(intensity: f64) -> WorldInputs {
    let library: ComponentLibrary = read("library/generic.json");
    let rules: DesignRules = read("design_rules/generic.json");
    let profile: Profile =
        schema::profile::migrate(read("profiles/d31_lowsplit_1002_iccap.json")).unwrap();
    let map = design::generate(LayoutFamily::Suburban, 3, 300);
    let controls = DesignControls {
        rules_id: Some(rules.id.clone()),
        build_over_ceiling: true,
        ..DesignControls::default()
    };
    let d = design(&map, &profile, &library, &rules, &controls).unwrap();
    WorldInputs {
        library,
        nodes: d
            .nodes
            .iter()
            .map(|n| NodeInput {
                plant: n.plant.clone(),
                profile: profile.clone(),
            })
            .collect(),
        fiber: Some(d.fiber.clone()),
        streets: map.streets.clone(),
        homes: map.homes.clone(),
        hub: map.headend,
        rates: read::<OpsRates>("ops/rates.json"),
        seed: 26,
        intensity,
    }
}

#[test]
fn one_advance_is_many() {
    let make = || World::new(std::sync::Arc::new(inputs(400.0)), Policy::Reactive).unwrap();
    let (mut a, mut b) = (make(), make());
    a.act(Action::DigIn { near: None });
    b.act(Action::DigIn { near: None });
    a.advance(6 * 3600).unwrap();
    for h in 1..=12 {
        b.advance(h * 1800).unwrap();
    }
    assert_eq!(
        serde_json::to_string(&a.records()).unwrap(),
        serde_json::to_string(&b.records()).unwrap()
    );
    assert_eq!(a.feed_since(0), b.feed_since(0));
}

#[test]
fn a_dig_in_is_called_in_and_fixed_from_calls_alone() {
    let mut w = World::new(std::sync::Arc::new(inputs(0.0)), Policy::Reactive).unwrap();
    w.advance(3600).unwrap();
    assert!(w.homes_not_up().is_empty(), "a quiet hour breaks nothing");
    w.act(Action::DigIn { near: None });
    w.advance(30 * 3600).unwrap();
    let r = w.records();
    assert!(r.counters.calls > 0, "{:?}", r.counters);
    assert!(r.counters.truck_rolls > 0);
    assert!(
        r.jobs
            .iter()
            .any(|j| matches!(j.outcome, Some(Outcome::Fixed { .. }))),
        "{:#?}",
        r.jobs
    );
    assert!(
        w.homes_not_up().iter().all(|(_, _, s)| *s != Standing::Out),
        "{:?}",
        w.homes_not_up()
    );
    // Fixed, then proved: the ticket closes only once telemetry shows its homes back.
    assert!(
        r.tickets
            .iter()
            .all(|k| k.status == schema::ops::TicketStatus::Closed),
        "{:#?}",
        r.tickets
    );
}

#[test]
fn the_twin_rolls_before_the_calls_pile_up() {
    let mut pair = Pair::new(inputs(0.0)).unwrap();
    pair.advance(3600).unwrap();
    pair.act(Action::DigIn { near: None });
    pair.advance(30 * 3600).unwrap();
    let (r, t) = (pair.reactive.records(), pair.twin.records());
    assert!(t.counters.fixes > 0, "{:?}", t.counters);
    // The same hazards in both: one seed and one log.
    assert_eq!(
        pair.reactive
            .feed_since(0)
            .iter()
            .filter(|f| f.truth)
            .count(),
        pair.twin.feed_since(0).iter().filter(|f| f.truth).count()
    );
    let first_dispatch = |c: &schema::ops::Counters| {
        c.onset_to_dispatch.0 as f64 / c.onset_to_dispatch.1.max(1) as f64
    };
    assert!(
        first_dispatch(&t.counters) <= first_dispatch(&r.counters),
        "twin {:?} reactive {:?}",
        t.counters,
        r.counters
    );
}

#[test]
fn both_worlds_of_a_pair_are_folds() {
    let (mut a, mut b) = (
        Pair::new(inputs(400.0)).unwrap(),
        Pair::new(inputs(400.0)).unwrap(),
    );
    a.act(Action::FiberCut);
    b.act(Action::FiberCut);
    a.advance(8 * 3600).unwrap();
    for h in 1..=16 {
        b.advance(h * 1800).unwrap();
    }
    for (x, y) in [(&a.reactive, &b.reactive), (&a.twin, &b.twin)] {
        assert_eq!(
            serde_json::to_string(&x.records()).unwrap(),
            serde_json::to_string(&y.records()).unwrap()
        );
    }
}

fn near(a: LonLat, b: LonLat) -> f64 {
    sim::faults::dist_ft(a, b)
}

#[test]
fn placed_events_land_where_they_were_placed() {
    let i = inputs(0.0);
    let home = i.homes[i.homes.len() / 2].geo;
    let mut w = World::new(std::sync::Arc::new(i), Policy::Reactive).unwrap();
    w.act(Action::DigIn { near: Some(home) });
    w.act(Action::CarHit { near: Some(home) });
    w.act(Action::PowerCut {
        center: home,
        radius_ft: 300.0,
        hours: 1.0,
    });
    w.advance(3 * 60).unwrap();
    let placed: Vec<_> = w.feed_since(0).into_iter().filter(|f| f.truth).collect();
    assert_eq!(placed.len(), 3, "{placed:#?}");
    for f in &placed {
        let at = f.geo.expect("a placed event says where");
        assert!(
            near(at, home) < 1500.0,
            "{} is {:.0} ft away",
            f.text,
            near(at, home)
        );
    }
}

#[test]
fn a_wrong_reading_rolls_a_truck_that_finds_nothing() {
    // House power out across one node: its modems go dark, which is all the twin can see, so it reads a break and rolls a
    // truck there, and the crew finds nothing in the plant to fix.
    let i = inputs(0.0);
    let plant = &i.nodes[0].plant;
    let geos: Vec<LonLat> = plant
        .elements
        .iter()
        .filter(|e| e.kind == schema::plant::ElementKind::Modem)
        .filter_map(|e| e.geo)
        .collect();
    let center = LonLat {
        lon_deg: geos.iter().map(|g| g.lon_deg).sum::<f64>() / geos.len() as f64,
        lat_deg: geos.iter().map(|g| g.lat_deg).sum::<f64>() / geos.len() as f64,
    };
    let radius_ft = geos.iter().map(|g| near(*g, center)).fold(0.0, f64::max) + 50.0;
    let mut w = World::new(std::sync::Arc::new(i), Policy::Twin).unwrap();
    w.act(Action::PowerCut {
        center,
        radius_ft,
        hours: 6.0,
    });
    w.advance(5 * 3600).unwrap();
    let r = w.records();
    let telemetry: Vec<_> = r
        .jobs
        .iter()
        .filter(|j| j.source == schema::ops::JobSource::Telemetry)
        .collect();
    assert!(
        !telemetry.is_empty(),
        "the twin should read the dark homes as a break: {:#?}",
        r.jobs
    );
    assert!(
        r.jobs
            .iter()
            .all(|j| !matches!(j.outcome, Some(Outcome::Fixed { .. })))
    );
    assert!(r.counters.no_trouble_found > 0, "{:?}", r.counters);
}

#[test]
fn a_fault_no_modem_feels_draws_no_call() {
    // Households answer to their own modems: a connector at 40 dB return loss is in the fault list and in no modem's
    // telemetry worth calling about.
    let i = inputs(0.0);
    let span = i.nodes[0]
        .plant
        .edges
        .iter()
        .find(|e| matches!(e.kind, schema::plant::EdgeKind::Span { .. }))
        .map(|e| e.id.clone())
        .unwrap();
    let mut w = World::new(std::sync::Arc::new(i), Policy::Reactive).unwrap();
    w.place(
        0,
        Fault {
            onset: Onset::Instant,
            kind: FaultKind::ConnectorReflection {
                location: Location::Span {
                    span_id: EdgeId::new(span.as_str()),
                    pos_ft: 1.0,
                },
                return_loss_db: 40.0,
                couplings: None,
            },
        },
        "a connector barely loose",
    );
    w.advance(12 * 3600).unwrap();
    assert_eq!(w.records().counters.calls, 0, "{:?}", w.records().counters);
    assert!(w.homes_not_up().iter().all(|(_, _, s)| *s == Standing::Off));
}

#[test]
fn a_quiet_week_on_one_street_breaks_nothing() {
    let plant: schema::plant::Plant = read("one_street/plant_lowsplit_iccap.json");
    let library: ComponentLibrary = read("library/generic.json");
    let profile: Profile = read("profiles/d31_lowsplit_1002_iccap.json");
    let hub = plant.elements.iter().find_map(|e| e.geo).unwrap_or(LonLat {
        lon_deg: -105.09,
        lat_deg: 39.9,
    });
    let graph = plant_graph::PlantGraph::new(&plant, &library, &profile).unwrap();
    assert!(graph.check_design().is_empty());
    let mut w = World::new(
        std::sync::Arc::new(WorldInputs {
            library: library.clone(),
            nodes: vec![NodeInput {
                plant: plant.clone(),
                profile,
            }],
            fiber: None,
            streets: Vec::new(),
            homes: Vec::new(),
            hub,
            rates: read::<OpsRates>("ops/rates.json"),
            seed: 26,
            intensity: 0.0,
        }),
        Policy::Reactive,
    )
    .unwrap();
    w.advance(7 * 86_400).unwrap();
    let c = w.records().counters;
    assert_eq!(
        (c.calls, c.truck_rolls, c.customer_minutes_out),
        (0, 0, 0),
        "{c:?}"
    );
}

#[test]
fn a_rates_file_missing_a_rate_is_refused() {
    let mut i = inputs(1.0);
    i.rates.rates.remove("handle_min");
    assert!(World::new(std::sync::Arc::new(i), Policy::Twin).is_err());
}

/// The demo's day, on a 1,600-home region: a storm at dawn, then a dig-in, a car into the plant and a fiber cut at noon.
/// Prints both worlds' counts and how long a world hour takes. `cargo test --release -p ops -- --ignored a_demo_day`.
#[test]
#[ignore]
fn a_demo_day() {
    let library: ComponentLibrary = read("library/generic.json");
    let rules: DesignRules = read("design_rules/generic.json");
    let profile: Profile =
        schema::profile::migrate(read("profiles/d31_highsplit_1218_rpd.json")).unwrap();
    let map = design::generate(LayoutFamily::Suburban, 1, 1600);
    let controls = DesignControls {
        rules_id: Some(rules.id.clone()),
        build_over_ceiling: true,
        ..DesignControls::default()
    };
    let d = design(&map, &profile, &library, &rules, &controls).unwrap();
    let inputs = WorldInputs {
        library,
        nodes: d
            .nodes
            .iter()
            .map(|n| NodeInput {
                plant: n.plant.clone(),
                profile: profile.clone(),
            })
            .collect(),
        fiber: Some(d.fiber.clone()),
        streets: map.streets.clone(),
        homes: map.homes.clone(),
        hub: map.headend,
        rates: read::<OpsRates>("ops/rates.json"),
        seed: 26,
        intensity: 50.0,
    };
    // Task 28 wrote the demo day down as a scenario: a storm at dawn, then a dig-in, a car and a fiber cut at noon.
    let scenarios: Scenarios = read("ops/scenarios.json");
    let day = scenarios.get("stormy-monday").unwrap();
    assert_eq!((day.seed, day.intensity), (26, 50.0));
    let mut pair = Pair::new(inputs).unwrap();
    day.apply(&mut pair);
    let started = std::time::Instant::now();
    pair.advance(24 * 3600).unwrap();
    let per_hour = started.elapsed().as_secs_f64() / 24.0;
    for (name, w) in [("calls", &pair.reactive), ("twin", &pair.twin)] {
        eprintln!("{name}: {:?}", w.counters());
        for f in w
            .feed_since(0)
            .iter()
            .filter(|f| !f.text.contains("power company") && !f.text.starts_with("A call"))
        {
            eprintln!(
                "  {:>5} {}{}",
                f.t_s / 60,
                if f.truth { "TRUTH " } else { "" },
                f.text
            );
        }
    }
    eprintln!("{per_hour:.2} s of wall time per world hour, both worlds");
}

#[test]
fn the_pair_on_two_threads_is_the_pair_in_turn() {
    let make = || {
        let mut p = Pair::new(inputs(400.0)).unwrap();
        p.act(Action::DigIn { near: None });
        p.act_at(2 * 3600, Action::FiberCut);
        p
    };
    let (mut a, mut b) = (make(), make());
    a.advance(5 * 3600).unwrap();
    b.advance_in_turn(5 * 3600).unwrap();
    for (x, y) in [(&a.reactive, &b.reactive), (&a.twin, &b.twin)] {
        assert_eq!(
            serde_json::to_string(&x.records()).unwrap(),
            serde_json::to_string(&y.records()).unwrap()
        );
        assert_eq!(x.feed_since(0), y.feed_since(0));
        assert_eq!(x.series(), y.series());
    }
}

#[test]
fn an_action_written_ahead_is_the_action_made_then() {
    let make = || World::new(std::sync::Arc::new(inputs(50.0)), Policy::Twin).unwrap();
    let (mut ahead, mut then) = (make(), make());
    ahead.act_at(3 * 3600, Action::DigIn { near: None });
    ahead.act_at(3 * 3600, Action::FiberCut);
    then.advance(3 * 3600 - 60).unwrap();
    then.act(Action::DigIn { near: None });
    then.act(Action::FiberCut);
    ahead.advance(8 * 3600).unwrap();
    then.advance(8 * 3600).unwrap();
    assert_eq!(
        serde_json::to_string(&ahead.records()).unwrap(),
        serde_json::to_string(&then.records()).unwrap()
    );
    assert_eq!(ahead.feed_since(0), then.feed_since(0));
    assert_eq!(ahead.actions_taken(), then.actions_taken());
}

#[test]
fn every_scenario_reads_and_its_first_action_lands() {
    let scenarios: Scenarios = read("ops/scenarios.json");
    assert_eq!(scenarios.schema_version, ops::scenario::SCHEMA_VERSION);
    assert!(scenarios.scenarios.len() >= 3);
    let round = serde_json::to_string(&scenarios).unwrap();
    assert_eq!(
        serde_json::from_str::<Scenarios>(&round).unwrap(),
        scenarios
    );
    for s in &scenarios.scenarios {
        assert!(!s.actions.is_empty(), "{}", s.id);
        let first = s.actions.iter().map(|p| p.at_s).min().unwrap();
        let mut w = World::new(std::sync::Arc::new(inputs(0.0)), Policy::Reactive).unwrap();
        for p in &s.actions {
            w.act_at(p.at_s, p.action.clone());
        }
        w.advance(first - 60).unwrap();
        assert!(w.actions_taken().is_empty(), "{}", s.id);
        w.advance(first).unwrap();
        assert!(!w.actions_taken().is_empty(), "{}", s.id);
    }
}

#[test]
fn the_sky_follows_the_clock_and_the_weather() {
    let mut w = World::new(std::sync::Arc::new(inputs(0.0)), Policy::Reactive).unwrap();
    let sky = w.sky();
    assert_eq!((sky.hour, sky.day), (6.0, 0));
    assert_eq!(sky.storm_until_s, None);
    w.act(Action::Storm {
        hours: 2.0,
        wind_m_per_s: 30.0,
        rain_mm_per_hr: 10.0,
    });
    w.act(Action::HeatWave {
        hours: 1.0,
        delta_c: 9.0,
    });
    w.advance(20 * 60).unwrap();
    let sky = w.sky();
    assert_eq!(sky.hour, 6.0 + 20.0 / 60.0);
    assert_eq!(sky.storm_until_s, Some(60 + 2 * 3600));
    assert_eq!((sky.wind_m_per_s, sky.rain_mm_per_hr), (30.0, 10.0));
    assert_eq!((sky.heat_delta_c, sky.heat_until_s), (9.0, Some(60 + 3600)));
    // `advance` goes to a world time: 18 h 20 min after Monday 06:00 is Tuesday 00:20.
    w.advance(18 * 3600 + 20 * 60).unwrap();
    let sky = w.sky();
    assert_eq!((sky.hour, sky.day), (20.0 / 60.0, 1));
    assert_eq!(sky.storm_until_s, None);
    // A sample every ten world minutes from the start, whatever the steps did.
    let times: Vec<i64> = w.series().iter().map(|s| s.t_s).collect();
    assert_eq!(
        times,
        (0..=w.t_s / 600).map(|i| i * 600).collect::<Vec<_>>()
    );
}

/// How much the demo day's result rests on its draws (task 28): the stormy Monday on other seeds, and with its noon events a
/// minute either side. Printed, not asserted: it's a reading of this world, and the spec records it.
#[test]
#[ignore]
fn demo_days_across_seeds() {
    let library: ComponentLibrary = read("library/generic.json");
    let rules: DesignRules = read("design_rules/generic.json");
    let profile: Profile =
        schema::profile::migrate(read("profiles/d31_highsplit_1218_rpd.json")).unwrap();
    let map = design::generate(LayoutFamily::Suburban, 1, 1600);
    let controls = DesignControls {
        rules_id: Some(rules.id.clone()),
        build_over_ceiling: true,
        ..DesignControls::default()
    };
    let d = design(&map, &profile, &library, &rules, &controls).unwrap();
    let scenarios: Scenarios = read("ops/scenarios.json");
    let day = scenarios.get("stormy-monday").unwrap().clone();
    let variants: Vec<(u64, i64)> = [26, 1, 2, 3, 4, 5]
        .iter()
        .map(|s| (*s, 0))
        .chain([(26, -60), (26, 60)])
        .collect();
    let rows: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = variants
            .iter()
            .map(|(seed, shift)| {
                let (d, map, library, profile, day) = (&d, &map, &library, &profile, &day);
                scope.spawn(move || {
                    let inputs = WorldInputs {
                        library: library.clone(),
                        nodes: d
                            .nodes
                            .iter()
                            .map(|n| NodeInput {
                                plant: n.plant.clone(),
                                profile: profile.clone(),
                            })
                            .collect(),
                        fiber: Some(d.fiber.clone()),
                        streets: map.streets.clone(),
                        homes: map.homes.clone(),
                        hub: map.headend,
                        rates: read::<OpsRates>("ops/rates.json"),
                        seed: *seed,
                        intensity: day.intensity,
                    };
                    let mut pair = Pair::new(inputs).unwrap();
                    for p in &day.actions {
                        let at = if p.at_s > 3600 { p.at_s + shift } else { p.at_s };
                        pair.act_at(at, p.action.clone());
                    }
                    pair.advance(24 * 3600).unwrap();
                    let (a, b) = (pair.reactive.counters(), pair.twin.counters());
                    format!(
                        "seed {seed:>2} noon {shift:+>3}s | calls {:>7} cm, {} rolls, {} ntf | twin {:>7} cm, {} rolls, {} ntf | twin/calls {:.2}",
                        a.customer_minutes_out,
                        a.truck_rolls,
                        a.no_trouble_found,
                        b.customer_minutes_out,
                        b.truck_rolls,
                        b.no_trouble_found,
                        b.customer_minutes_out as f64 / a.customer_minutes_out.max(1) as f64
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for r in rows {
        eprintln!("{r}");
    }
}

#[test]
fn every_roll_is_a_trip_and_a_driving_truck_is_on_its_last_one() {
    let mut w = World::new(std::sync::Arc::new(inputs(0.0)), Policy::Reactive).unwrap();
    w.act(Action::DigIn { near: None });
    for h in 1..=30 {
        w.advance(h * 3600).unwrap();
        for (k, _) in w.trucks_now() {
            if let schema::ops::TruckState::Driving {
                job,
                path,
                depart_t_s,
                arrive_t_s,
            } = &k.state
            {
                let last = w
                    .trips_since(0)
                    .into_iter()
                    .rfind(|t| t.truck == k.id)
                    .unwrap();
                assert_eq!(
                    (last.job, &last.path, last.depart_t_s, last.arrive_t_s),
                    (Some(*job), path, *depart_t_s, *arrive_t_s)
                );
            }
        }
    }
    let rolls = w.trips_since(0).iter().filter(|t| t.job.is_some()).count() as u64;
    assert_eq!(rolls, w.counters().truck_rolls);
    assert!(rolls > 0);
    // Trips from a time on are the ones still going then or set off after.
    let later = w.trips_since(5 * 3600);
    assert!(later.iter().all(|t| t.arrive_t_s >= 5 * 3600));
}

#[test]
fn a_disaster_with_nothing_to_hit_says_so() {
    let mut given = inputs(0.0);
    for n in &mut given.nodes {
        for e in &mut n.plant.edges {
            if let schema::plant::EdgeKind::Span { installation, .. } = &mut e.kind {
                *installation = schema::plant::Installation::Aerial;
            }
        }
    }
    given.fiber = None;
    let mut w = World::new(std::sync::Arc::new(given), Policy::Reactive).unwrap();
    w.act(Action::DigIn { near: None });
    w.act(Action::FiberCut);
    w.advance(120).unwrap();
    let feed = w.feed_since(0);
    assert!(
        feed.iter()
            .any(|f| f.truth && f.text.contains("none of this region's cable is buried")),
        "{feed:?}"
    );
    assert!(
        feed.iter()
            .any(|f| f.truth && f.text.contains("no fiber plant")),
        "{feed:?}"
    );
    assert!(w.truth().is_empty());
}

/// Two nodes on one fiber cable, both dark at once for a reason that isn't the fiber (house power out across both): the twin
/// reads it as the fiber and sends splicers, who find nothing. It must not send them to that fiber again the same day: the next
/// truck goes to the next rung. A user watched truck 3 go back to one node's fiber over and over (task 29).
#[test]
fn splicers_who_found_a_fiber_clean_arent_sent_back_to_it() {
    let i = inputs(0.0);
    let geos: Vec<LonLat> = i.nodes[..2]
        .iter()
        .flat_map(|n| n.plant.elements.iter())
        .filter(|e| e.kind == schema::plant::ElementKind::Modem)
        .filter_map(|e| e.geo)
        .collect();
    let center = LonLat {
        lon_deg: geos.iter().map(|g| g.lon_deg).sum::<f64>() / geos.len() as f64,
        lat_deg: geos.iter().map(|g| g.lat_deg).sum::<f64>() / geos.len() as f64,
    };
    let radius_ft = geos.iter().map(|g| near(*g, center)).fold(0.0, f64::max) + 50.0;
    let mut w = World::new(std::sync::Arc::new(i), Policy::Twin).unwrap();
    w.act(Action::PowerCut {
        center,
        radius_ft,
        hours: 10.0,
    });
    w.advance(10 * 3600).unwrap();
    let r = w.records();
    let fiber: Vec<_> = r
        .jobs
        .iter()
        .filter(|j| matches!(j.target, schema::ops::JobTarget::Fiber { .. }))
        .collect();
    assert!(
        !fiber.is_empty(),
        "two nodes dark on one cable should read as the fiber first: {:#?}",
        r.jobs
    );
    for (k, a) in fiber.iter().enumerate() {
        for b in &fiber[k + 1..] {
            let again = a.target == b.target
                && a.outcome == Some(Outcome::NoTroubleFound)
                && a.done_t_s
                    .is_some_and(|d| b.created_t_s >= d && b.created_t_s - d < 86_400);
            assert!(
                !again,
                "splicers sent back to a fiber they found clean: {a:#?}\n{b:#?}"
            );
        }
    }
}

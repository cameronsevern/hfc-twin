//! Inputs in Postgres, as JSONB: plants, faults, work orders, evidence, maps, regions, environment. Nothing derived goes in (invariant 1), so a
//! restart recomputes what it needs. Plain Postgres and no cloud SDK (invariant 7).

use std::collections::BTreeMap;

use postgres::{Client, NoTls};
use schema::common::ServiceGroupId;
use schema::environment::EnvironmentSample;
use schema::fault::Fault;
use schema::plant::Plant;
use schema::region::{Region, StreetMap};
use schema::work_order::WorkOrder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use twin::lifecycle::Evidence;
use twin::state::DesignInput;
use twin::state::Node;
use twin::{OsmReport, Twin};

/// What can go wrong.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Serialize, Deserialize)]
struct NodeDoc {
    plant: Plant,
    faults: Vec<Fault>,
    work_orders: Vec<WorkOrder>,
    evidence: BTreeMap<uuid::Uuid, Evidence>,
}

#[derive(Serialize, Deserialize)]
struct MetaDoc {
    sample: EnvironmentSample,
    maps: BTreeMap<String, StreetMap>,
    regions: BTreeMap<String, Region>,
    #[serde(default)]
    design_inputs: BTreeMap<String, DesignInput>,
    #[serde(default)]
    osm_reports: BTreeMap<String, OsmReport>,
    /// Version 2. A store written before has none.
    #[serde(default)]
    fiber_plants: BTreeMap<String, schema::fiber::FiberPlant>,
    /// Task 22: fiber faults by region, as a fiber fault set each, so each carries its version. A store written before has none.
    #[serde(default)]
    fiber_faults: Vec<schema::fiber_fault::FiberFaultSet>,
}

/// A connection, and the one table it needs.
pub struct Store {
    client: Client,
}

const META: &str = "meta";

impl Store {
    /// Connects, creating the table if it isn't there. Keeps trying for a minute, since a database starting beside this takes a moment.
    pub fn connect(url: &str) -> Result<Store, Error> {
        let mut tries = 0;
        let mut client = loop {
            match Client::connect(url, NoTls) {
                Ok(client) => break client,
                Err(error) if tries < 60 => {
                    if tries == 0 {
                        eprintln!("waiting for the database: {error:?}");
                    }
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                Err(error) => return Err(format!("{error:?}").into()),
            }
        };
        client.batch_execute(
            "CREATE TABLE IF NOT EXISTS twin_state (
                 key TEXT PRIMARY KEY,
                 doc JSONB NOT NULL,
                 updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
             )",
        )?;
        Ok(Store { client })
    }

    fn put(&mut self, key: &str, doc: &Value) -> Result<(), Error> {
        self.client.execute(
            "INSERT INTO twin_state (key, doc) VALUES ($1, $2)
             ON CONFLICT (key) DO UPDATE SET doc = EXCLUDED.doc, updated_at = now()",
            &[&key, doc],
        )?;
        Ok(())
    }

    /// Saves one node.
    pub fn save_node(&mut self, twin: &Twin, id: &ServiceGroupId) -> Result<(), Error> {
        if let Some(node) = twin.nodes.get(id) {
            let doc = NodeDoc {
                plant: node.plant.clone(),
                faults: node.faults.clone(),
                work_orders: node.work_orders.clone(),
                evidence: node.evidence.clone(),
            };
            self.put(&format!("node:{id}"), &serde_json::to_value(doc)?)?;
        }
        Ok(())
    }

    /// Saves what isn't per node.
    pub fn save_meta(&mut self, twin: &Twin) -> Result<(), Error> {
        let doc = MetaDoc {
            sample: twin.sample.clone(),
            maps: twin.maps.clone(),
            regions: twin.regions.clone(),
            design_inputs: twin.design_inputs.clone(),
            osm_reports: twin.osm_reports.clone(),
            fiber_plants: twin.fiber_plants.clone(),
            fiber_faults: twin
                .fiber_faults
                .iter()
                .map(|(region, faults)| schema::fiber_fault::FiberFaultSet {
                    schema_version: schema::fiber_fault::SCHEMA_VERSION.to_string(),
                    region: region.clone(),
                    faults: faults.clone(),
                })
                .collect(),
        };
        self.put(META, &serde_json::to_value(doc)?)
    }

    /// Saves everything, and forgets nodes the twin no longer has, or they'd come back on restart.
    pub fn save_all(&mut self, twin: &Twin) -> Result<(), Error> {
        let keep: Vec<String> = twin.nodes.keys().map(|id| format!("node:{id}")).collect();
        self.client.execute(
            "DELETE FROM twin_state WHERE key LIKE 'node:%' AND NOT (key = ANY($1))",
            &[&keep],
        )?;
        for id in twin.nodes.keys() {
            self.save_node(twin, id)?;
        }
        self.save_meta(twin)
    }

    /// Reads what's stored into a twin. Returns how many nodes it found. Documents written on an earlier schema version
    /// come forward by migration as they load (invariant 3).
    pub fn load_into(&mut self, twin: &mut Twin) -> Result<usize, Error> {
        let rows = self
            .client
            .query("SELECT key, doc FROM twin_state ORDER BY key", &[])?;
        let mut count = 0;
        for row in rows {
            let key: String = row.get(0);
            let doc: Value = row.get(1);
            if key == META {
                let meta: MetaDoc = serde_json::from_value(doc)?;
                twin.sample = meta.sample;
                twin.maps = meta
                    .maps
                    .into_iter()
                    .map(|(id, map)| Ok((id, schema::region::migrate_map(map)?)))
                    .collect::<Result<_, String>>()?;
                twin.regions = meta
                    .regions
                    .into_iter()
                    .map(|(id, region)| Ok((id, schema::region::migrate(region)?)))
                    .collect::<Result<_, String>>()?;
                twin.design_inputs = meta.design_inputs;
                twin.osm_reports = meta.osm_reports;
                twin.fiber_plants = meta.fiber_plants;
                twin.fiber_faults = meta
                    .fiber_faults
                    .into_iter()
                    .map(|set| {
                        let set = schema::fiber_fault::migrate(set)?;
                        Ok((set.region, set.faults))
                    })
                    .collect::<Result<_, String>>()?;
            } else if key.starts_with("node:") {
                let node: NodeDoc = serde_json::from_value(doc)?;
                let plant = schema::plant::migrate(node.plant)?;
                let work_orders = node
                    .work_orders
                    .into_iter()
                    .map(schema::work_order::migrate_work_order)
                    .collect::<Result<Vec<_>, String>>()?;
                twin.nodes.insert(
                    plant.service_group.clone(),
                    Node {
                        plant,
                        faults: node.faults,
                        work_orders,
                        evidence: node.evidence,
                    },
                );
                count += 1;
            }
        }
        Ok(count)
    }

    /// Clears it. Tests only have a use for this.
    pub fn wipe(&mut self) -> Result<(), Error> {
        self.client.execute("DELETE FROM twin_state", &[])?;
        Ok(())
    }
}

/// Which state a tool changed: a node, the region as a whole, or nothing.
pub enum Touched {
    /// Read-only.
    Nothing,
    /// One node.
    Node(ServiceGroupId),
    /// What isn't per node.
    Meta,
    /// Potentially anything.
    All,
}

/// What a tool call changes. A new tool that writes has to be added here, and the test on this list fails until it is.
pub fn touched_by(name: &str, arguments: &Value) -> Touched {
    let group = || {
        arguments
            .get("service_group")
            .and_then(Value::as_str)
            .map(ServiceGroupId::new)
    };
    match name {
        // A fiber fault is the region's, kept with what isn't per node (task 22).
        "inject_fault" | "clear_faults" if arguments.get("region").is_some() => Touched::Meta,
        "inject_fault"
        | "clear_faults"
        | "diagnose"
        | "dispatch_work_order"
        | "complete_work_order"
        | "verify_work_order"
        | "mark_built"
        | "apply_migration" => group().map_or(Touched::Nothing, Touched::Node),
        "set_environment" | "generate_layout" | "import_osm" | "import_osm_area"
        | "set_headend" => Touched::Meta,
        "design_region" | "discard_map" => Touched::All,
        _ => Touched::Nothing,
    }
}

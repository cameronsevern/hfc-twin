//! A stand-in for hardware. The sim drives emulated modems and a CMTS that answer SNMP and upload real
//! PNM files over TFTP, so telemetry reaches the adapter the way a real device's would (`docs/DESIGN.md`).
//! Everything it serves is synthetic, and its `sysDescr` says so (invariant 11).
#![deny(missing_docs)]

mod mib;

use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use plant_graph::PlantGraph;
use pnm_wire::oids;
use pnm_wire::oids::{Test, ctl_status, meas_status};
use pnm_wire::snmp::{self, Message, Oid, PduKind, Value};
use pnm_wire::tftp;
use schema::component::ComponentLibrary;
use schema::environment::{Environment, EnvironmentSample};
use schema::fault::{Fault, FaultSet};
use schema::observation::ModemObservation;
use schema::plant::Plant;
use schema::profile::Profile;
use sim::PhysicsError;
use sim::rng::hash_str;
use sim::telemetry::{Detail, Scenario, SimOptions, StepObservation, simulate};

pub use mib::{IfIndexes, mac_of};

/// What every emulated device's `sysDescr` starts with. An adapter reads this and marks data `Synthetic`.
pub const SYS_DESCR: &str = "hfc-twin emulated device; simulated data";

struct World {
    plant: Plant,
    library: ComponentLibrary,
    profile: Profile,
    faults: Vec<Fault>,
    samples: Vec<EnvironmentSample>,
    seed: u64,
    measurement_noise_db: f64,
    randomize: Option<u64>,
    steps: Option<Vec<StepObservation>>,
}

impl World {
    /// The latest step, simulating if it isn't cached. Each device reads the same step.
    fn latest(&mut self) -> Result<&StepObservation, PhysicsError> {
        if self.steps.is_none() {
            let graph = PlantGraph::new(&self.plant, &self.library, &self.profile)
                .map_err(|_| PhysicsError::BadComponent(self.plant.elements[0].id.clone()))?;
            let faults = FaultSet {
                schema_version: schema::fault::SCHEMA_VERSION.to_string(),
                seed: self.seed,
                faults: self.faults.clone(),
            };
            let environment = Environment {
                schema_version: schema::environment::SCHEMA_VERSION.to_string(),
                samples: self.samples.clone(),
                ambient_sources: Vec::new(),
            };
            let steps = simulate(&Scenario {
                graph: &graph,
                environment: &environment,
                faults: &faults,
                seed: self.seed,
                options: SimOptions {
                    detail: Detail::Full,
                    randomize: self.randomize,
                    measurement_noise_db: self.measurement_noise_db,
                    rxmer_scatter_db: 0.0,
                },
                link: None,
            })?;
            self.steps = Some(steps);
        }
        self.steps
            .as_ref()
            .and_then(|s| s.last())
            .ok_or_else(|| PhysicsError::BadComponent(self.plant.elements[0].id.clone()))
    }
}

/// One modem's control state: what a real one keeps while a test runs.
#[derive(Default)]
struct ModemControl {
    dest_ip_type: i64,
    dest_ip: Vec<u8>,
    dest_path: Vec<u8>,
    testing: Option<Test>,
    status: BTreeMap<(i64, u32), i64>,
    file_names: BTreeMap<(i64, u32), Vec<u8>>,
    uploads: u32,
}

struct Device {
    socket: UdpSocket,
    addr: SocketAddr,
}

/// A running emulated plant: one agent per modem and one for the CMTS.
pub struct Emulator {
    world: Arc<Mutex<World>>,
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
    /// Where the CMTS answers.
    pub cmts_addr: SocketAddr,
    /// `(element, MAC, address)`, what an inventory system and the CMTS would each hold part of.
    pub modems: Vec<(String, [u8; 6], SocketAddr)>,
}

/// The inputs an emulated plant starts from.
pub struct EmulatorConfig {
    /// The plant.
    pub plant: Plant,
    /// Parts.
    pub library: ComponentLibrary,
    /// The profile.
    pub profile: Profile,
    /// The seed.
    pub seed: u64,
    /// Domain randomization's seed, if on.
    pub randomize: Option<u64>,
    /// Device measurement noise, dB.
    pub measurement_noise_db: f64,
    /// Where uploads go: a real modem uses port 69, and this is here so tests needn't be root.
    pub tftp_port: u16,
}

fn bind_device() -> io::Result<Device> {
    let socket = UdpSocket::bind(("127.0.0.1", 0))?;
    socket.set_read_timeout(Some(Duration::from_millis(50)))?;
    let addr = socket.local_addr()?;
    Ok(Device { socket, addr })
}

impl Emulator {
    /// Starts one agent per modem, and one for the CMTS.
    pub fn start(config: EmulatorConfig, sample: EnvironmentSample) -> io::Result<Emulator> {
        let tftp_port = config.tftp_port;
        let world = Arc::new(Mutex::new(World {
            plant: config.plant,
            library: config.library,
            profile: config.profile,
            faults: Vec::new(),
            samples: vec![sample],
            seed: config.seed,
            measurement_noise_db: config.measurement_noise_db,
            randomize: config.randomize,
            steps: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();

        let modem_ids: Vec<String> = {
            let guard = world.lock().map_err(|_| io::Error::other("poisoned"))?;
            let mut ids: Vec<String> = guard
                .plant
                .elements
                .iter()
                .filter(|e| matches!(e.kind, schema::plant::ElementKind::Modem))
                .map(|e| e.id.to_string())
                .collect();
            ids.sort();
            ids
        };
        let mut modems = Vec::new();
        for id in &modem_ids {
            let device = bind_device()?;
            let mac = mac_of(id);
            modems.push((id.clone(), mac, device.addr));
            let (world, stop, id) = (world.clone(), stop.clone(), id.clone());
            handles.push(thread::spawn(move || {
                run_modem(device, &id, mac, world, stop, tftp_port);
            }));
        }

        let cmts = bind_device()?;
        let cmts_addr = cmts.addr;
        let directory: Vec<([u8; 6], SocketAddr)> =
            modems.iter().map(|(_, mac, addr)| (*mac, *addr)).collect();
        let (world_cmts, stop_cmts) = (world.clone(), stop.clone());
        handles.push(thread::spawn(move || {
            run_cmts(cmts, directory, world_cmts, stop_cmts, tftp_port);
        }));

        Ok(Emulator {
            world,
            stop,
            handles,
            cmts_addr,
            modems,
        })
    }

    /// Replaces the injected faults. Devices read the new physics from the next request.
    pub fn set_faults(&self, faults: Vec<Fault>) {
        if let Ok(mut world) = self.world.lock() {
            world.faults = faults;
            world.steps = None;
        }
    }

    /// Advances time, so counters accumulate as they would.
    pub fn push_sample(&self, sample: EnvironmentSample) {
        if let Ok(mut world) = self.world.lock() {
            world.samples.push(sample);
            world.steps = None;
        }
    }

    /// What the sim says this modem shows now, for checking the wire against. A real plant has no such call.
    pub fn truth(&self, id: &str) -> Option<ModemObservation> {
        let mut world = self.world.lock().ok()?;
        let step = world.latest().ok()?;
        step.modems
            .iter()
            .find(|m| m.element_id.as_str() == id)
            .cloned()
    }

    /// As `truth`, for the whole step.
    pub fn truth_step(&self) -> Option<StepObservation> {
        let mut world = self.world.lock().ok()?;
        world.latest().ok().cloned()
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

fn respond(request: &Message, table: &BTreeMap<Oid, Value>) -> Message {
    let bindings = request
        .bindings
        .iter()
        .map(|(oid, _)| match request.kind {
            PduKind::GetNext => match table
                .range((
                    std::ops::Bound::Excluded(oid.clone()),
                    std::ops::Bound::Unbounded,
                ))
                .next()
            {
                Some((next, value)) => (next.clone(), value.clone()),
                None => (oid.clone(), Value::EndOfMibView),
            },
            _ => match table.get(oid) {
                Some(value) => (oid.clone(), value.clone()),
                None => (oid.clone(), Value::NoSuchInstance),
            },
        })
        .collect();
    Message {
        community: request.community.clone(),
        kind: PduKind::Response,
        request_id: request.request_id,
        error_status: snmp::NO_ERROR,
        error_index: 0,
        bindings,
    }
}

fn set_error(request: &Message, status: i64) -> Message {
    Message {
        community: request.community.clone(),
        kind: PduKind::Response,
        request_id: request.request_id,
        error_status: status,
        error_index: 1,
        bindings: request.bindings.clone(),
    }
}

fn mac_hex(mac: &[u8; 6]) -> String {
    mac.iter().map(|b| format!("{b:02x}")).collect()
}

fn run_modem(
    device: Device,
    id: &str,
    mac: [u8; 6],
    world: Arc<Mutex<World>>,
    stop: Arc<AtomicBool>,
    tftp_port: u16,
) {
    let control = Arc::new(Mutex::new(ModemControl {
        dest_ip_type: 0,
        ..Default::default()
    }));
    let mut buffer = vec![0u8; 8192];
    while !stop.load(Ordering::Relaxed) {
        let Ok((n, from)) = device.socket.recv_from(&mut buffer) else {
            continue;
        };
        let Ok(request) = Message::decode(&buffer[..n]) else {
            continue;
        };
        let reply = match request.kind {
            PduKind::Set => handle_modem_set(&request, id, mac, &world, &control, tftp_port),
            _ => {
                let table = modem_table(id, mac, &world, &control);
                respond(&request, &table)
            }
        };
        let _ = device.socket.send_to(&reply.encode(), from);
    }
}

fn modem_table(
    id: &str,
    _mac: [u8; 6],
    world: &Arc<Mutex<World>>,
    control: &Arc<Mutex<ModemControl>>,
) -> BTreeMap<Oid, Value> {
    let mut table = BTreeMap::new();
    table.insert(
        oids::sys_descr(),
        Value::Octets(SYS_DESCR.as_bytes().to_vec()),
    );
    if let Ok(mut guard) = world.lock() {
        let profile = guard.profile.clone();
        if let Ok(step) = guard.latest()
            && let Some(modem) = step.modems.iter().find(|m| m.element_id.as_str() == id)
        {
            mib::modem_objects(modem, &profile, &mut table);
        }
    }
    if let Ok(state) = control.lock() {
        table.insert(
            oids::bulk_dest_ip_type(),
            Value::Integer(state.dest_ip_type),
        );
        table.insert(
            oids::bulk_dest_ip_addr(),
            Value::Octets(state.dest_ip.clone()),
        );
        table.insert(
            oids::bulk_dest_path(),
            Value::Octets(state.dest_path.clone()),
        );
        table.insert(
            oids::cm_ctl_status(),
            Value::Integer(if state.testing.is_some() {
                ctl_status::TEST_IN_PROGRESS
            } else {
                ctl_status::READY
            }),
        );
        table.insert(
            oids::cm_ctl_test(),
            Value::Integer(state.testing.map_or(0, Test::code)),
        );
        for ((test, if_index), status) in &state.status {
            if let Some(kind) = test_from(*test) {
                table.insert(kind.status(*if_index), Value::Integer(*status));
            }
        }
        for ((test, if_index), name) in &state.file_names {
            if let Some(kind) = test_from(*test) {
                table.insert(kind.file_name(*if_index), Value::Octets(name.clone()));
            }
        }
    }
    table
}

fn test_from(code: i64) -> Option<Test> {
    [Test::DsRxmer, Test::ChanEst, Test::UsPreeq, Test::Spectrum]
        .into_iter()
        .find(|t| t.code() == code)
}

/// A test's trigger, if this OID is one. `(test, ifIndex)`.
fn trigger(oid: &Oid) -> Option<(Test, u32)> {
    for test in [Test::DsRxmer, Test::ChanEst, Test::UsPreeq, Test::Spectrum] {
        let if_index = *oid.0.last()?;
        if test.enable(if_index) == *oid {
            return Some((test, if_index));
        }
    }
    None
}

fn handle_modem_set(
    request: &Message,
    id: &str,
    mac: [u8; 6],
    world: &Arc<Mutex<World>>,
    control: &Arc<Mutex<ModemControl>>,
    tftp_port: u16,
) -> Message {
    let Ok(mut state) = control.lock() else {
        return set_error(request, snmp::INCONSISTENT_VALUE);
    };
    for (oid, value) in &request.bindings {
        if *oid == oids::bulk_dest_ip_type() {
            // [CM-OSSI §D.5.3]: writable only when ready.
            if state.testing.is_some() {
                return set_error(request, snmp::INCONSISTENT_VALUE);
            }
            let Value::Integer(v) = value else {
                return set_error(request, snmp::WRONG_TYPE);
            };
            state.dest_ip_type = *v;
        } else if *oid == oids::bulk_dest_ip_addr() {
            if state.testing.is_some() {
                return set_error(request, snmp::INCONSISTENT_VALUE);
            }
            let Value::Octets(v) = value else {
                return set_error(request, snmp::WRONG_TYPE);
            };
            state.dest_ip = v.clone();
        } else if *oid == oids::bulk_dest_path() {
            if state.testing.is_some() {
                return set_error(request, snmp::INCONSISTENT_VALUE);
            }
            let Value::Octets(v) = value else {
                return set_error(request, snmp::WRONG_TYPE);
            };
            state.dest_path = v.clone();
        } else if let Some((test, if_index)) = trigger(oid) {
            // [CM-OSSI §D.2.3]: a covered test starts only at ready.
            if state.testing.is_some() {
                return set_error(request, snmp::INCONSISTENT_VALUE);
            }
            if state.dest_ip.len() != 4 {
                return set_error(request, snmp::INCONSISTENT_VALUE);
            }
            state.testing = Some(test);
            state
                .status
                .insert((test.code(), if_index), meas_status::BUSY);
            state.uploads += 1;
            let name = format!(
                "{}{}_{}_{}",
                String::from_utf8_lossy(&state.dest_path),
                match test {
                    Test::DsRxmer => "PNMDsMer",
                    Test::ChanEst => "PNMChEstCoef",
                    Test::UsPreeq => "PNMUsPreEq",
                    Test::Spectrum => "PNMSpecAnData",
                },
                mac_hex(&mac),
                state.uploads,
            );
            let destination = SocketAddr::from((
                [
                    state.dest_ip[0],
                    state.dest_ip[1],
                    state.dest_ip[2],
                    state.dest_ip[3],
                ],
                tftp_port,
            ));
            let (world, control, id) = (world.clone(), control.clone(), id.to_string());
            thread::spawn(move || {
                run_test(
                    test,
                    if_index,
                    &id,
                    mac,
                    &name,
                    destination,
                    &world,
                    &control,
                );
            });
        } else {
            return set_error(request, snmp::NOT_WRITABLE);
        }
    }
    Message {
        community: request.community.clone(),
        kind: PduKind::Response,
        request_id: request.request_id,
        error_status: snmp::NO_ERROR,
        error_index: 0,
        bindings: request.bindings.clone(),
    }
}

/// Builds the file and uploads it, as the device itself does [CM-OSSI §D.5.1].
#[allow(clippy::too_many_arguments)]
fn run_test(
    test: Test,
    if_index: u32,
    id: &str,
    mac: [u8; 6],
    name: &str,
    destination: SocketAddr,
    world: &Arc<Mutex<World>>,
    control: &Arc<Mutex<ModemControl>>,
) {
    let built = world.lock().ok().and_then(|mut guard| {
        let profile = guard.profile.clone();
        let step = guard.latest().ok()?;
        let modem = step.modems.iter().find(|m| m.element_id.as_str() == id)?;
        mib::build_file(test, if_index, modem, &profile, mac)
    });
    let outcome = built.map(|bytes| tftp::upload(destination, name, &bytes, tftp::BLOCK_SIZE_IPV4));
    if let Ok(mut state) = control.lock() {
        let status = match outcome {
            Some(Ok(())) => meas_status::SAMPLE_READY,
            _ => meas_status::ERROR,
        };
        state.status.insert((test.code(), if_index), status);
        state
            .file_names
            .insert((test.code(), if_index), name.as_bytes().to_vec());
        state.testing = None;
    }
}

fn run_cmts(
    device: Device,
    directory: Vec<([u8; 6], SocketAddr)>,
    world: Arc<Mutex<World>>,
    stop: Arc<AtomicBool>,
    tftp_port: u16,
) {
    let _ = tftp_port;
    let mut buffer = vec![0u8; 8192];
    while !stop.load(Ordering::Relaxed) {
        let Ok((n, from)) = device.socket.recv_from(&mut buffer) else {
            continue;
        };
        let Ok(request) = Message::decode(&buffer[..n]) else {
            continue;
        };
        let reply = if request.kind == PduKind::Set {
            set_error(&request, snmp::NOT_WRITABLE)
        } else {
            let mut table = BTreeMap::new();
            table.insert(
                oids::sys_descr(),
                Value::Octets(SYS_DESCR.as_bytes().to_vec()),
            );
            for (i, (mac, addr)) in directory.iter().enumerate() {
                let index = i as u32 + 1;
                table.insert(
                    oids::cmts_cm_mac().child(&[index]),
                    Value::Octets(mac.to_vec()),
                );
                table.insert(
                    oids::cmts_cm_ip().child(&[index]),
                    Value::IpAddress([127, 0, 0, 1]),
                );
                table.insert(
                    oids::emulator_cm_port().child(&[index]),
                    Value::Integer(i64::from(addr.port())),
                );
            }
            if let Ok(mut guard) = world.lock() {
                let profile = guard.profile.clone();
                if let Ok(step) = guard.latest() {
                    mib::cmts_objects(step, &profile, &directory, &mut table);
                }
            }
            respond(&request, &table)
        };
        let _ = device.socket.send_to(&reply.encode(), from);
    }
}

/// A stable hash, re-exported so a test can name a MAC without a modem.
pub fn stable_hash(text: &str) -> u64 {
    hash_str(text)
}

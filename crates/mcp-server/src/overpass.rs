//! Fetches a box of OpenStreetMap from Overpass for `import_osm_area`, then hands the extract to `import_osm`, which has no network
//! and whose result is the input the twin keeps (invariant 1). OSM changes, so the fetch happens once and the map is what's stored.

use std::io::Read;
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use twin::tools::AreaArgs;

/// Where to fetch from and how hard to try.
#[derive(Debug, Clone)]
pub struct Source {
    /// Overpass endpoints, tried in order.
    pub urls: Vec<String>,
    /// Tries per endpoint.
    pub tries: u32,
    /// Wait before a second try, doubled each time after.
    pub wait: Duration,
    /// How long one request may take.
    pub timeout: Duration,
    /// How long the whole fetch may take, across tries and servers. Someone is waiting on this.
    pub deadline: Duration,
}

impl Source {
    /// The public servers, or the one the operator named.
    pub fn new(url: Option<String>) -> Source {
        let urls = match url {
            Some(url) => vec![url],
            None => vec![
                "https://overpass-api.de/api/interpreter".to_string(),
                "https://overpass.kumi.systems/api/interpreter".to_string(),
            ],
        };
        Source {
            urls,
            tries: 3,
            wait: Duration::from_secs(5),
            timeout: Duration::from_secs(100),
            deadline: Duration::from_secs(150),
        }
    }
}

/// The most of a reply read. ureq's `into_string` stops at 10 MB, which a few blocks of a dense city pass; one import's largest
/// box (40 km²) of city stays under this.
const MAX_REPLY_BYTES: u64 = 256 * 1024 * 1024;

/// Reads a reply whole, up to `MAX_REPLY_BYTES`, saying so in words a person can act on if it's bigger.
fn read_reply(url: &str, response: ureq::Response) -> Result<String, String> {
    let mut text = String::new();
    response
        .into_reader()
        .take(MAX_REPLY_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("{url} sent an unreadable reply: {e}"))?;
    if text.len() as u64 > MAX_REPLY_BYTES {
        return Err(format!(
            "That area's map is over {} MB, more than one import can read. Draw a smaller area.",
            MAX_REPLY_BYTES / 1024 / 1024
        ));
    }
    Ok(text)
}

/// The road values `design::osm` keeps, so the server isn't asked for footpaths it would read and throw away.
const ROADS: &str = "^(primary|primary_link|secondary|secondary_link|tertiary|tertiary_link|residential|unclassified|living_street|service|road|trunk|trunk_link|track|busway)$";

fn query(area: &AreaArgs) -> String {
    let bbox = format!(
        "{},{},{},{}",
        area.south_deg, area.west_deg, area.north_deg, area.east_deg
    );
    format!(
        "[out:json][timeout:90];(way[\"highway\"~\"{ROADS}\"]({bbox});way[\"building\"]({bbox});node[\"addr:housenumber\"]({bbox}););out body;>;out skel qt;"
    )
}

/// Fetches the extract. Busy and gateway errors get retried, and other failures come back with what happened.
pub fn fetch(source: &Source, area: &AreaArgs) -> Result<String, String> {
    let body = query(area);
    let agent = ureq::AgentBuilder::new()
        .user_agent(concat!("hfc-twin/", env!("CARGO_PKG_VERSION")))
        .build();
    let started = Instant::now();
    let left = || source.deadline.saturating_sub(started.elapsed());
    let mut last = "no Overpass server is configured".to_string();
    'servers: for url in &source.urls {
        let mut wait = source.wait;
        for attempt in 0..source.tries {
            if attempt > 0 {
                if wait >= left() {
                    break 'servers;
                }
                sleep(wait);
                wait *= 2;
            }
            if left().is_zero() {
                break 'servers;
            }
            let request = agent.post(url).timeout(source.timeout.min(left()));
            match request.send_form(&[("data", body.as_str())]) {
                Ok(response) => return read_reply(url, response),
                Err(ureq::Error::Status(code, _)) if [429, 502, 503, 504].contains(&code) => {
                    last = format!("{url} answered {code}: busy or down");
                }
                Err(ureq::Error::Status(code, _)) => {
                    last = format!("{url} answered {code}");
                    break;
                }
                Err(error) => last = format!("couldn't reach {url}: {error}"),
            }
        }
    }
    Err(format!(
        "Couldn't get that area from OpenStreetMap. {last}. Nothing was imported, so try again in a minute."
    ))
}

/// What to do with a message.
#[derive(Debug)]
pub enum Prepared {
    /// Call the twin with this.
    Call(Value),
    /// Answer with this, without touching the twin.
    Reply(Value),
}

/// Turns an `import_osm_area` call into the `import_osm` call that follows the fetch. Anything else passes through.
pub fn prepare(message: &Value, source: &Source, known: impl Fn(&str) -> bool) -> Prepared {
    let is_area = message.get("method").and_then(Value::as_str) == Some("tools/call")
        && message.pointer("/params/name").and_then(Value::as_str) == Some("import_osm_area");
    if !is_area {
        return Prepared::Call(message.clone());
    }
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let failed = |text: String| {
        Prepared::Reply(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"content": [{"type": "text", "text": text}], "isError": true},
        }))
    };
    let arguments = message
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let area: AreaArgs = match serde_json::from_value(arguments) {
        Ok(area) => area,
        Err(error) => return failed(format!("bad arguments: {error}")),
    };
    if let Err(error) = area.check() {
        return failed(error);
    }
    let map_id = area.map_id();
    let overpass_json = if known(&map_id) {
        Value::Null
    } else {
        match fetch(source, &area) {
            Ok(text) => Value::String(text),
            Err(error) => return failed(error),
        }
    };
    Prepared::Call(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": "import_osm", "arguments": {
            "map_id": map_id,
            "overpass_json": overpass_json,
            "installation": area.installation,
            "headend": area.centre(),
        }},
    }))
}

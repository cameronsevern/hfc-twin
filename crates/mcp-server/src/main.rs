//! `hfc-twin-mcp [--stdio | --http ADDR] [--fixtures DIR] [--seed N] [--allow-identity] [--as-built] [--model FILE]... [--database-url URL] [--overpass-url URL]`.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Json, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use mcp_server::handle;
use mcp_server::overpass::{self, Prepared};
use mcp_server::persist::{Store, Touched, touched_by};
use serde_json::{Value, json};
use twin::{Twin, load_as_built, load_one_street};

const PAGE: &str = include_str!("../ui/index.html");
// MapLibre ships with the server, so the page loads with no CDN in reach (ui/vendor/README.md).
const MAPLIBRE_JS: &str = include_str!("../ui/vendor/maplibre-gl.js");
const MAPLIBRE_CSS: &str = include_str!("../ui/vendor/maplibre-gl.css");
// The operations screen (task 28), its own files beside the page. Not cached, as the page isn't, so a rebuilt server serves
// the screen that matches it.
const OPERATE_JS: &str = include_str!("../ui/operate.js");
const OPERATE_CSS: &str = include_str!("../ui/operate.css");

#[derive(Clone)]
struct Shared {
    twin: Arc<Mutex<Twin>>,
    store: Option<Arc<Mutex<Store>>>,
    overpass: Arc<overpass::Source>,
}

/// One message, then whatever it changed goes to the store. A failed save is reported in the log and doesn't lose the reply.
fn serve(shared: &Shared, message: &Value) -> Option<Value> {
    // A fetch takes up to a minute or so, and everyone else keeps the twin meanwhile.
    let known = |map_id: &str| {
        shared
            .twin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .maps
            .contains_key(map_id)
    };
    let prepared = match overpass::prepare(message, &shared.overpass, known) {
        Prepared::Call(call) => call,
        Prepared::Reply(reply) => return Some(reply),
    };
    let message = &prepared;
    let mut guard = shared
        .twin
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let reply = handle(&mut guard, message);
    let is_call = message.get("method").and_then(Value::as_str) == Some("tools/call");
    let succeeded = reply
        .as_ref()
        .is_some_and(|r| r.pointer("/result/isError") == Some(&json!(false)));
    if let (true, true, Some(store)) = (is_call, succeeded, &shared.store) {
        let name = message
            .pointer("/params/name")
            .and_then(Value::as_str)
            .unwrap_or("");
        let empty = json!({});
        let arguments = message.pointer("/params/arguments").unwrap_or(&empty);
        let mut store = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let saved = match touched_by(name, arguments) {
            Touched::Nothing => Ok(()),
            Touched::Node(id) => store.save_node(&guard, &id),
            Touched::Meta => store.save_meta(&guard),
            Touched::All => store.save_all(&guard),
        };
        if let Err(error) = saved {
            eprintln!("couldn't save after {name}: {error}");
        }
    }
    reply
}

async fn mcp(State(shared): State<Shared>, Json(message): Json<Value>) -> impl IntoResponse {
    let reply = tokio::task::spawn_blocking(move || serve(&shared, &message))
    .await
    .unwrap_or_else(|_| Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32603, "message": "internal error"}})));
    match reply {
        Some(value) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            value.to_string(),
        ),
        None => (
            StatusCode::ACCEPTED,
            [(header::CONTENT_TYPE, "application/json")],
            String::new(),
        ),
    }
}

async fn page() -> Html<&'static str> {
    Html(PAGE)
}

async fn maplibre_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        MAPLIBRE_JS,
    )
}

async fn operate_js() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        OPERATE_JS,
    )
}

async fn operate_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        OPERATE_CSS,
    )
}

async fn maplibre_css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        MAPLIBRE_CSS,
    )
}

fn stdio(shared: Shared) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => serve(&shared, &message),
            Err(error) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": error.to_string()}}),
            ),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

async fn http(shared: Shared, addr: String) -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(page))
        .route("/vendor/maplibre-gl.js", get(maplibre_js))
        .route("/vendor/maplibre-gl.css", get(maplibre_css))
        .route("/operate.js", get(operate_js))
        .route("/operate.css", get(operate_css))
        .route("/mcp", post(mcp))
        .with_state(shared);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("hfc-twin MCP at http://{addr}/mcp, map at http://{addr}/ (all data simulated)");
    axum::serve(listener, app).await
}

struct Options {
    http: Option<String>,
    fixtures: PathBuf,
    seed: u64,
    allow_identity: bool,
    model: Vec<PathBuf>,
    database_url: Option<String>,
    overpass_url: Option<String>,
    as_built: bool,
}

fn parse() -> Result<Options, String> {
    let mut options = Options {
        http: None,
        fixtures: PathBuf::from("fixtures"),
        seed: 1,
        allow_identity: false,
        model: Vec::new(),
        database_url: None,
        overpass_url: None,
        as_built: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stdio" => options.http = None,
            "--http" => options.http = Some(args.next().ok_or("--http needs an address")?),
            "--fixtures" => {
                options.fixtures = PathBuf::from(args.next().ok_or("--fixtures needs a path")?)
            }
            "--seed" => {
                options.seed = args
                    .next()
                    .ok_or("--seed needs a number")?
                    .parse()
                    .map_err(|e| format!("--seed: {e}"))?;
            }
            "--allow-identity" => options.allow_identity = true,
            "--as-built" => options.as_built = true,
            "--model" => options
                .model
                .push(PathBuf::from(args.next().ok_or("--model needs a path")?)),
            "--database-url" => {
                options.database_url = Some(args.next().ok_or("--database-url needs a URL")?)
            }
            "--overpass-url" => {
                options.overpass_url = Some(args.next().ok_or("--overpass-url needs a URL")?)
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(options)
}

fn main() -> ExitCode {
    let options = match parse() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    // `--as-built` starts on `fixtures/one_street_as_built/`, with its fiber plant and region (task 22).
    let loaded = if options.as_built {
        load_as_built(&options.fixtures, options.seed)
    } else {
        load_one_street(&options.fixtures, options.seed)
    };
    let mut twin = match loaded {
        Ok(twin) => twin,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    twin.identity_allowed = options.allow_identity;
    // Each `--model` goes where the plant it trained on says: a version 1 file to plant without version 2 parts, a version
    // 2 file to plant with them (task 24).
    for path in &options.model {
        let loaded = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                let value: serde_json::Value =
                    serde_json::from_str(&text).map_err(|e| e.to_string())?;
                if value.get("plant_schema").and_then(|v| v.as_str()) == Some("2") {
                    proposal::Proposer::from_json(&text)
                        .map(|m| {
                            eprintln!(
                                "proposal model {} loaded for version 2 plant",
                                path.display()
                            );
                            twin.as_built_model = Some(m);
                        })
                        .map_err(|e| e.to_string())
                } else {
                    proposal::Model::from_json(&text)
                        .map(|model| {
                            eprintln!(
                                "proposal model {} loaded, trained on {:?} data",
                                model.name, model.trained_on
                            );
                            twin.model = Some(model);
                        })
                        .map_err(|e| e.to_string())
                }
            });
        if let Err(error) = loaded {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    }
    let store = match &options.database_url {
        Some(url) => match Store::connect(url) {
            Ok(mut store) => {
                match store.load_into(&mut twin) {
                    Ok(0) => {
                        if let Err(error) = store.save_all(&twin) {
                            eprintln!("{error}");
                            return ExitCode::FAILURE;
                        }
                    }
                    Ok(n) => eprintln!("{n} nodes restored from the database"),
                    Err(error) => {
                        eprintln!("{error}");
                        return ExitCode::FAILURE;
                    }
                }
                Some(Arc::new(Mutex::new(store)))
            }
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let shared = Shared {
        twin: Arc::new(Mutex::new(twin)),
        store,
        overpass: Arc::new(overpass::Source::new(options.overpass_url.clone())),
    };
    let outcome = match options.http {
        Some(addr) => tokio::runtime::Runtime::new().and_then(|rt| rt.block_on(http(shared, addr))),
        None => stdio(shared),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

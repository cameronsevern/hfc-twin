//! JSON-RPC 2.0 for MCP: `initialize`, `ping`, `tools/list`, `tools/call`. No state of its own and no
//! other way in (invariant 8).
#![deny(missing_docs)]

pub mod overpass;
pub mod persist;

use serde_json::{Value, json};
use twin::{ToolError, Twin, call_tool, tool_specs};

/// The MCP revision this server speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

fn error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

fn result(id: &Value, value: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": value})
}

/// One message in, at most one out. A notification gets no reply.
pub fn handle(twin: &mut Twin, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str);
    let Some(id) = id else {
        // A notification: nothing to say back, whatever it was.
        return None;
    };
    let Some(method) = method else {
        return Some(error(&id, -32600, "invalid request"));
    };
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    Some(match method {
        "initialize" => result(
            &id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "hfc-twin", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "A digital twin of an HFC network. Everything here is simulated unless a result says Field. \
                                 Faults are physical parameters, and telemetry is computed from them.",
            }),
        ),
        "ping" => result(&id, json!({})),
        "tools/list" => result(&id, json!({"tools": tool_specs()})),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(&id, -32602, "tools/call needs a name"));
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match call_tool(twin, name, &arguments) {
                Ok(value) => result(
                    &id,
                    json!({
                        "content": [{"type": "text", "text": value.to_string()}],
                        "structuredContent": value,
                        "isError": false,
                    }),
                ),
                Err(ToolError::Unknown(name)) => error(&id, -32602, format!("no tool {name}")),
                Err(other) => result(
                    &id,
                    json!({
                        "content": [{"type": "text", "text": other.to_string()}],
                        "isError": true,
                    }),
                ),
            }
        }
        other => error(&id, -32601, format!("no method {other}")),
    })
}

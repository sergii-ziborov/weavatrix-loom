//! Optional **agent-only** MCP stdio host for Loom.
//!
//! Transport is [`mcport`]; semantics are the command bus. **Not** used by
//! Loom Studio or product packaging — Studio uses `loom-server` HTTP only.

use mcport::{json, serve, ServerIdentity, ToolReply, ToolServer, Value};
use wvx_command_bus::{project_validate, PROTOCOL_VERSION};
use wvx_ir::Project;

struct LoomTools;

impl ToolServer for LoomTools {
    fn identity(&self) -> ServerIdentity {
        ServerIdentity::new(
            "weavatrix-loom",
            env!("CARGO_PKG_VERSION"),
            "Bounded Loom tools: validate projects and inspect protocol version.",
        )
    }

    fn catalog(&mut self) -> Value {
        json!([
            {
                "name": "loom_protocol_version",
                "description": "Return the Loom command-bus protocol version.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "name": "validate_project",
                "description": "Validate a WVX project document (JSON object).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "object",
                            "description": "WVX project document"
                        }
                    },
                    "required": ["project"],
                    "additionalProperties": false
                }
            }
        ])
    }

    fn call(&mut self, name: &str, arguments: Value) -> ToolReply {
        match name {
            "loom_protocol_version" => ToolReply::structured(json!({
                "protocol_version": PROTOCOL_VERSION,
                "product": "weavatrix-loom",
                "version": env!("CARGO_PKG_VERSION"),
            })),
            "validate_project" => validate_project_tool(arguments),
            _ => ToolReply::error(format!("unknown tool: {name}")),
        }
    }
}

fn validate_project_tool(arguments: Value) -> ToolReply {
    let Some(project_val) = arguments.get("project") else {
        return ToolReply::error("missing `project` argument");
    };
    // Bridge mcport Value → serde_json via text.
    let raw = project_val.to_string();
    let project: Project = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => return ToolReply::error(format!("invalid project JSON: {e}")),
    };
    let resp = project_validate(&project);
    ToolReply::structured(resp)
}

fn main() -> std::io::Result<()> {
    // Gate F external demo (unpublished) — host-only, not part of wvx-command-bus.
    wvx_adapter_external_demo::register();
    serve(&mut LoomTools)
}

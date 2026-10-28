//! `oxypixel mcp`: a Model Context Protocol server on stdin and stdout.
//!
//! An AI assistant (Claude Desktop, Claude Code, and other MCP clients)
//! starts this command and sends JSON-RPC 2.0 messages, one per line. The
//! tools in [`tools`] read and write `.oxypix` documents, render frames as
//! PNG images the assistant can look at, export, and pack sprites with
//! OxyPack. Nothing here opens a window or a dialog, and nothing is written
//! to stdout except protocol messages; notes go to stderr.
//!
//! Written from the public MCP specification; no SDK is used.

mod tools;

use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// Protocol versions this server speaks, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Largest message accepted, in bytes. A longer line is refused and skipped.
const MAX_MESSAGE: usize = 16 * 1024 * 1024;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

pub const HELP: &str = "Usage: oxypixel mcp [--root DIR]

Runs a Model Context Protocol server on stdin and stdout for AI assistants.
Its tools inspect, create, draw on, and export .oxypix documents and pack
sprites. With --root, every path a tool reads or writes must be inside DIR.
";

/// `oxypixel mcp [--root DIR]`. Returns the process exit code.
pub fn run(args: &[String]) -> u8 {
    let root = match args {
        [] => None,
        [flag, dir] if flag == "--root" => match std::fs::canonicalize(dir) {
            Ok(dir) if dir.is_dir() => Some(dir),
            _ => {
                eprintln!("oxypixel mcp: --root {dir} is not a folder");
                return 2;
            }
        },
        [flag] if flag == "--help" || flag == "-h" => {
            print!("{HELP}");
            return 0;
        }
        _ => {
            eprint!("{HELP}");
            return 2;
        }
    };
    let mut server = Server::new(root);
    let stdin = io::stdin();
    match serve(&mut server, stdin.lock(), io::stdout().lock()) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("oxypixel mcp: {err}");
            1
        }
    }
}

/// Answer messages until the input ends.
pub(crate) fn serve(
    server: &mut Server,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    loop {
        let mut line = Vec::new();
        let read = input
            .by_ref()
            .take(MAX_MESSAGE as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        let reply = if line.len() > MAX_MESSAGE && line.last() != Some(&b'\n') {
            // Drop the rest of the oversized line.
            let mut rest = Vec::new();
            input.read_until(b'\n', &mut rest)?;
            Some(error_reply(
                Value::Null,
                INVALID_REQUEST,
                "message is too large",
            ))
        } else {
            match std::str::from_utf8(&line) {
                Ok(text) if text.trim().is_empty() => None,
                Ok(text) => server.handle(text),
                Err(_) => Some(error_reply(
                    Value::Null,
                    PARSE_ERROR,
                    "message is not UTF-8",
                )),
            }
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}

pub(crate) struct Server {
    root: Option<PathBuf>,
    /// The bridge file a running editor writes, for tools called without a path.
    bridge: Option<PathBuf>,
}

impl Server {
    pub(crate) fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            bridge: crate::bridge::info_path(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_bridge(mut self, info: PathBuf) -> Self {
        self.bridge = Some(info);
        self
    }

    /// One message in, at most one reply out. Notifications get no reply.
    pub(crate) fn handle(&mut self, text: &str) -> Option<Value> {
        let message: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(err) => return Some(error_reply(Value::Null, PARSE_ERROR, &err.to_string())),
        };
        let Some(object) = message.as_object() else {
            return Some(error_reply(
                Value::Null,
                INVALID_REQUEST,
                "expected a JSON-RPC object",
            ));
        };
        let id = object.get("id").cloned();
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            // A response from the client, or junk. Neither needs an answer.
            return id.map(|id| error_reply(id, INVALID_REQUEST, "missing method"));
        };
        let id = id?; // notifications: initialized, cancelled, and the rest
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        Some(match method {
            "initialize" => ok_reply(id, initialize(&params)),
            "ping" => ok_reply(id, json!({})),
            "tools/list" => ok_reply(id, json!({ "tools": tools::list() })),
            "tools/call" => self.call(id, &params),
            other => error_reply(id, METHOD_NOT_FOUND, &format!("unknown method {other}")),
        })
    }

    fn call(&mut self, id: Value, params: &Value) -> Value {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return error_reply(id, INVALID_PARAMS, "tools/call needs a tool name");
        };
        let empty = json!({});
        let args = params.get("arguments").unwrap_or(&empty);
        if !args.is_object() {
            return error_reply(id, INVALID_PARAMS, "arguments must be an object");
        }
        let Some(result) = tools::call(
            name,
            args,
            &Paths {
                root: self.root.as_deref(),
                bridge: self.bridge.as_deref(),
            },
        ) else {
            return error_reply(id, INVALID_PARAMS, &format!("unknown tool {name}"));
        };
        let result = match result {
            Ok(output) => {
                let mut content = vec![json!({ "type": "text", "text": output.text })];
                if let Some(png) = output.png {
                    content.push(json!({
                        "type": "image",
                        "data": base64(&png),
                        "mimeType": "image/png",
                    }));
                }
                json!({ "content": content, "isError": false })
            }
            // A tool that ran and failed is a result the assistant can read,
            // not a protocol error.
            Err(message) => json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true,
            }),
        };
        ok_reply(id, result)
    }
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "oxypixel",
            "title": "OxyPixel",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": "Tools for OxyPixel pixel-art documents (.oxypix). Coordinates are pixels from the top left, frames and layers count from 0, and layer 0 is the bottom. Colors are #RRGGBB or #RRGGBBAA. Use render to look at a frame after drawing. Editing tools save the document in place. Leave out path to work on the document open in the OxyPixel window, when its Live MCP bridge setting is on; each such call is one undo step there. create_document, export, and pack_sprites refuse to replace an existing file unless overwrite is true.",
    })
}

/// Where tools may read and write.
pub(crate) struct Paths<'a> {
    root: Option<&'a Path>,
    bridge: Option<&'a Path>,
}

impl Paths<'_> {
    pub(crate) fn bridge(&self) -> Option<&Path> {
        self.bridge
    }

    /// A path a tool was given, made absolute and checked against `--root`.
    /// The file itself need not exist yet, but its folder must.
    pub(crate) fn resolve(&self, text: &str) -> Result<PathBuf, String> {
        if text.trim().is_empty() {
            return Err("the path is empty".into());
        }
        let given = Path::new(text);
        let joined = match (self.root, given.is_absolute()) {
            (Some(root), false) => root.join(given),
            _ => given.to_path_buf(),
        };
        let name = joined
            .file_name()
            .ok_or_else(|| format!("{text} does not name a file"))?
            .to_owned();
        let parent = match joined.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let parent = std::fs::canonicalize(&parent)
            .map_err(|_| format!("the folder for {text} does not exist"))?;
        if let Some(root) = self.root {
            if !parent.starts_with(root) {
                return Err(format!("{text} is outside the folder this server may use"));
            }
        }
        Ok(parent.join(name))
    }
}

fn ok_reply(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_reply(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Standard base64 with padding, for image content.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(chunk.get(1).copied().unwrap_or(0)) << 8
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> shift & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;

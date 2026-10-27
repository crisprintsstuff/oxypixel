use std::path::Path;

use oxypixel_core::{load_oxypix, Rgba};
use serde_json::{json, Value};

use super::tools::fit_scale;
use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("oxypixel-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(std::fs::canonicalize(&dir).unwrap())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn request(server: &mut Server, id: i64, method: &str, params: Value) -> Value {
    let text =
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    server.handle(&text).expect("a request gets a reply")
}

/// Call a tool and return (is_error, text, image bytes as base64).
fn call(server: &mut Server, name: &str, args: Value) -> (bool, String, Option<String>) {
    let reply = request(
        server,
        7,
        "tools/call",
        json!({ "name": name, "arguments": args }),
    );
    let result = &reply["result"];
    let content = result["content"].as_array().expect("content");
    let text = content[0]["text"].as_str().unwrap_or_default().to_string();
    let image = content
        .iter()
        .find(|c| c["type"] == "image")
        .and_then(|c| c["data"].as_str())
        .map(str::to_string);
    (result["isError"] == true, text, image)
}

fn pixel(path: &Path, frame: usize, x: u32, y: u32) -> Rgba {
    load_oxypix(path)
        .unwrap()
        .composite_pixel(frame, x, y)
        .unwrap()
}

#[test]
fn base64_matches_the_standard_alphabet() {
    let cases = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foobar", "Zm9vYmFy"),
    ];
    for (plain, encoded) in cases {
        assert_eq!(base64(plain.as_bytes()), encoded);
    }
    assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
}

#[test]
fn initialize_negotiates_a_version_and_lists_tools() {
    let mut server = Server::new(None);
    let reply = request(
        &mut server,
        1,
        "initialize",
        json!({ "protocolVersion": "2025-03-26" }),
    );
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(reply["result"]["serverInfo"]["name"], "oxypixel");
    assert!(reply["result"]["capabilities"]["tools"].is_object());
    let newer = request(
        &mut server,
        2,
        "initialize",
        json!({ "protocolVersion": "2099-01-01" }),
    );
    assert_eq!(newer["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);

    let tools = request(&mut server, 3, "tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            assert_eq!(t["inputSchema"]["type"], "object");
            assert!(!t["description"].as_str().unwrap().is_empty());
            t["name"].as_str().unwrap()
        })
        .collect();
    for name in [
        "document_info",
        "render",
        "create_document",
        "draw",
        "add_layer",
        "set_layer",
        "add_frame",
        "set_frame",
        "set_tags",
        "set_palette",
        "export",
        "pack_sprites",
    ] {
        assert!(names.contains(&name), "{name} missing from {names:?}");
    }
    assert_eq!(
        request(&mut server, 4, "ping", json!({}))["result"],
        json!({})
    );
}

#[test]
fn protocol_errors_and_notifications() {
    let mut server = Server::new(None);
    assert_eq!(
        server.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
        None
    );
    let unknown = request(&mut server, 1, "resources/list", json!({}));
    assert_eq!(unknown["error"]["code"], METHOD_NOT_FOUND);
    assert_eq!(
        server.handle("{not json").unwrap()["error"]["code"],
        PARSE_ERROR
    );
    assert_eq!(
        server.handle("[1, 2]").unwrap()["error"]["code"],
        INVALID_REQUEST
    );
    let no_tool = request(&mut server, 2, "tools/call", json!({ "name": "teleport" }));
    assert_eq!(no_tool["error"]["code"], INVALID_PARAMS);
    let bad_args = request(
        &mut server,
        3,
        "tools/call",
        json!({ "name": "render", "arguments": 5 }),
    );
    assert_eq!(bad_args["error"]["code"], INVALID_PARAMS);
}

#[test]
fn draw_render_and_edit_a_document() {
    let dir = TempDir::new("flow");
    let mut server = Server::new(Some(dir.0.clone()));
    let doc = dir.0.join("hero.oxypix");

    let (err, text, _) = call(
        &mut server,
        "create_document",
        json!({ "path": "hero.oxypix", "width": 8, "height": 6 }),
    );
    assert!(!err, "{text}");
    let (err, text, _) = call(
        &mut server,
        "create_document",
        json!({ "path": "hero.oxypix", "width": 8, "height": 6 }),
    );
    assert!(err && text.contains("overwrite"), "{text}");

    let (err, text, _) = call(
        &mut server,
        "draw",
        json!({
            "path": "hero.oxypix",
            "color": "#E8A838",
            "operations": [
                { "op": "pixels", "points": [[0, 0], [7, 5]] },
                { "op": "line", "from": [0, 2], "to": [7, 2], "color": "#3AA17A" },
                { "op": "rect", "from": [5, 4], "to": [6, 5], "filled": true, "color": "#B33A3A" },
                { "op": "erase", "points": [[7, 5]] },
            ],
        }),
    );
    assert!(!err, "{text}");
    assert_eq!(pixel(&doc, 0, 0, 0), Rgba::new(0xE8, 0xA8, 0x38, 255));
    assert_eq!(pixel(&doc, 0, 3, 2), Rgba::new(0x3A, 0xA1, 0x7A, 255));
    assert_eq!(pixel(&doc, 0, 5, 4), Rgba::new(0xB3, 0x3A, 0x3A, 255));
    assert_eq!(pixel(&doc, 0, 7, 5), Rgba::CLEAR);

    let (err, text, _) = call(
        &mut server,
        "draw",
        json!({ "path": "hero.oxypix", "operations": [{ "op": "fill", "at": [1, 4], "color": "#14161F" }] }),
    );
    assert!(!err, "{text}");
    // The fill stops at the line across row 2.
    assert_eq!(pixel(&doc, 0, 1, 4), Rgba::new(0x14, 0x16, 0x1F, 255));
    assert_eq!(pixel(&doc, 0, 4, 5), Rgba::new(0x14, 0x16, 0x1F, 255));
    assert_eq!(pixel(&doc, 0, 1, 0), Rgba::CLEAR);

    // A bad operation fails the call and leaves the file as it was.
    let before = std::fs::read(&doc).unwrap();
    let (err, text, _) = call(
        &mut server,
        "draw",
        json!({ "path": "hero.oxypix", "color": "#FFFFFF", "operations": [{ "op": "pixels", "points": [[1, 1]] }, { "op": "pixels", "points": [[9, 9]] }] }),
    );
    assert!(
        err && text.contains("operation 1") && text.contains("outside"),
        "{text}"
    );
    assert_eq!(std::fs::read(&doc).unwrap(), before);
    let (err, text, _) = call(
        &mut server,
        "draw",
        json!({ "path": "hero.oxypix", "operations": [{ "op": "pixels", "points": [[1, 1]] }] }),
    );
    assert!(err && text.contains("color"), "{text}");

    let (err, text, image) = call(&mut server, "render", json!({ "path": "hero.oxypix" }));
    assert!(!err, "{text}");
    assert!(text.contains("64×"), "{text}");
    assert!(image.unwrap().starts_with("iVBORw0KGgo"), "a PNG");

    let (err, text, _) = call(
        &mut server,
        "add_layer",
        json!({ "path": "hero.oxypix", "name": "Shadow" }),
    );
    assert!(!err && text.contains("layer 1"), "{text}");
    let (err, _, _) = call(
        &mut server,
        "set_layer",
        json!({ "path": "hero.oxypix", "layer": 1, "opacity": 128, "visible": false, "blend": "multiply" }),
    );
    assert!(!err);
    let (err, text, _) = call(
        &mut server,
        "add_frame",
        json!({ "path": "hero.oxypix", "copy_of": 0, "duration_ms": 250, "name": "blink" }),
    );
    assert!(!err && text.contains("frame 1"), "{text}");
    let (err, _, _) = call(
        &mut server,
        "set_frame",
        json!({ "path": "hero.oxypix", "frame": 0, "name": "idle", "duration_ms": 120 }),
    );
    assert!(!err);
    let (err, text, _) = call(
        &mut server,
        "set_tags",
        json!({ "path": "hero.oxypix", "tags": [{ "name": "loop", "from": 0, "to": 1, "direction": "pingpong", "repeat": 2 }] }),
    );
    assert!(!err && text.contains("1 tag"), "{text}");
    let (err, text, _) = call(
        &mut server,
        "set_tags",
        json!({ "path": "hero.oxypix", "tags": [{ "name": "far", "from": 0, "to": 9 }] }),
    );
    assert!(err, "{text}");
    let (err, _, _) = call(
        &mut server,
        "set_palette",
        json!({ "path": "hero.oxypix", "colors": ["#000000", "#ffffff80"] }),
    );
    assert!(!err);

    let (err, text, _) = call(
        &mut server,
        "document_info",
        json!({ "path": "hero.oxypix" }),
    );
    assert!(!err, "{text}");
    let info: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        (info["width"].as_u64(), info["height"].as_u64()),
        (Some(8), Some(6))
    );
    assert_eq!(info["layers"][1]["name"], "Shadow");
    assert_eq!(info["layers"][1]["opacity"], 128);
    assert_eq!(info["layers"][1]["visible"], false);
    assert_eq!(info["layers"][1]["blend"], "multiply");
    assert_eq!(info["layers"][0]["blend"], "normal");
    assert_eq!(info["frames"][0]["name"], "idle");
    assert_eq!(info["frames"][0]["duration_ms"], 120);
    assert_eq!(info["frames"][1]["name"], "blink");
    assert_eq!(info["frames"][1]["duration_ms"], 250);
    assert_eq!(info["tags"][0]["direction"], "pingpong");
    assert_eq!(info["palette"], json!(["#000000", "#FFFFFF80"]));

    let (err, _, _) = call(
        &mut server,
        "set_frame",
        json!({ "path": "hero.oxypix", "frame": 1, "delete": true }),
    );
    assert!(!err);
    let (err, text, _) = call(
        &mut server,
        "set_frame",
        json!({ "path": "hero.oxypix", "frame": 0, "delete": true }),
    );
    assert!(err, "the last frame stays: {text}");
}

#[test]
fn export_and_pack_write_new_files_only() {
    let dir = TempDir::new("export");
    let mut server = Server::new(Some(dir.0.clone()));
    call(
        &mut server,
        "create_document",
        json!({ "path": "coin.oxypix", "width": 4, "height": 4, "background": "#E8A838" }),
    );
    assert_eq!(
        pixel(&dir.0.join("coin.oxypix"), 0, 3, 3),
        Rgba::new(0xE8, 0xA8, 0x38, 255)
    );
    call(
        &mut server,
        "add_frame",
        json!({ "path": "coin.oxypix", "copy_of": 0 }),
    );

    for (format, name) in [
        ("png", "coin.png"),
        ("bmp", "coin.bmp"),
        ("gif", "coin.gif"),
        ("sheet", "sheet.png"),
    ] {
        let (err, text, _) = call(
            &mut server,
            "export",
            json!({ "path": "coin.oxypix", "output": name, "format": format, "scale": 2, "json": true }),
        );
        assert!(!err, "{format}: {text}");
        assert!(dir.0.join(name).is_file(), "{name}");
    }
    assert!(dir.0.join("sheet.json").is_file());
    let (err, text, _) = call(
        &mut server,
        "export",
        json!({ "path": "coin.oxypix", "output": "coin.png", "format": "png" }),
    );
    assert!(err && text.contains("overwrite"), "{text}");
    let (err, _, _) = call(
        &mut server,
        "export",
        json!({ "path": "coin.oxypix", "output": "coin.png", "format": "png", "overwrite": true }),
    );
    assert!(!err);
    let (err, text, _) = call(
        &mut server,
        "export",
        json!({ "path": "coin.oxypix", "output": "x.tga", "format": "tga" }),
    );
    assert!(err && text.contains("unknown format"), "{text}");

    let (err, text, _) = call(
        &mut server,
        "pack_sprites",
        json!({ "inputs": ["coin.oxypix", "coin.png"], "output": "atlas.png", "packer": "maxrects", "dedupe": true }),
    );
    assert!(!err, "{text}");
    assert!(text.contains("from 2 files"), "{text}");
    assert!(dir.0.join("atlas.png").is_file() && dir.0.join("atlas.json").is_file());
}

#[test]
fn root_keeps_tools_inside_their_folder() {
    let dir = TempDir::new("root");
    let inner = dir.0.join("work");
    std::fs::create_dir_all(&inner).unwrap();
    let paths = Paths { root: Some(&inner) };
    assert_eq!(paths.resolve("a.oxypix").unwrap(), inner.join("a.oxypix"));
    assert!(paths
        .resolve("../a.oxypix")
        .unwrap_err()
        .contains("outside"));
    assert!(paths
        .resolve(dir.0.join("a.oxypix").to_str().unwrap())
        .unwrap_err()
        .contains("outside"));
    assert!(paths
        .resolve("missing/a.oxypix")
        .unwrap_err()
        .contains("does not exist"));
    assert!(paths.resolve("  ").is_err());
    let open = Paths { root: None };
    assert_eq!(
        open.resolve(dir.0.join("a.oxypix").to_str().unwrap())
            .unwrap(),
        dir.0.join("a.oxypix")
    );

    let mut server = Server::new(Some(inner));
    let (err, text, _) = call(
        &mut server,
        "create_document",
        json!({ "path": "../escape.oxypix", "width": 2, "height": 2 }),
    );
    assert!(err && text.contains("outside"), "{text}");
    assert!(!dir.0.join("escape.oxypix").exists());
}

#[test]
fn serve_answers_each_request_line_and_skips_notifications() {
    let mut server = Server::new(None);
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        "\n",
    );
    let mut output = Vec::new();
    serve(&mut server, input.as_bytes(), &mut output).unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["id"], 2);

    let mut huge = vec![b'x'; MAX_MESSAGE + 10];
    huge.push(b'\n');
    huge.extend_from_slice(br#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#);
    huge.push(b'\n');
    let mut output = Vec::new();
    serve(&mut server, huge.as_slice(), &mut output).unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], INVALID_REQUEST);
    assert_eq!(lines[1]["id"], 3);
}

#[test]
fn render_scale_fits_about_512_pixels() {
    assert_eq!(fit_scale(8, 6, 512), 64);
    assert_eq!(fit_scale(64, 64, 512), 8);
    assert_eq!(fit_scale(1024, 16, 512), 1);
    assert_eq!(fit_scale(0, 0, 512), 512);
}

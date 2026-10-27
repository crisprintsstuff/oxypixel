//! The MCP tools. Each call is self-contained: it loads the document named by
//! `path`, does its work through the same core calls the editor uses, and
//! saves it again. Files named as outputs are never replaced unless the call
//! says `overwrite: true`.

use std::path::{Path, PathBuf};

use oxypack::{counted, pack_files, write_atlas, PackOptions, Packer};
use oxypixel_core::{
    export_bmp, export_gif, export_gif_per_tag, export_png_scaled, export_sheet_files, load_oxypix,
    save_oxypix, tag_paths, BlendMode, Document, Editor, FrameTag, LoopDirection, MouseButton,
    Rgba, SheetFiles, SheetLayout, Tool, MAX_BRUSH, MAX_DIMENSION, MAX_FRAME_MS, MAX_PALETTE,
    MIN_FRAME_MS,
};
use serde_json::{json, Map, Value};

use super::Paths;

/// What a successful tool call returns: text, and a PNG for `render`.
pub(crate) struct Output {
    pub text: String,
    pub png: Option<Vec<u8>>,
}

impl Output {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            png: None,
        }
    }
}

type Args = Map<String, Value>;
type ToolResult = Result<Output, String>;

/// Most draw operations in one call.
const MAX_OPERATIONS: usize = 10_000;
/// Most points across the pixel and erase operations of one call.
const MAX_POINTS: usize = 1 << 20;
/// `render` picks a scale so the image is at most this many pixels a side.
const RENDER_TARGET: u32 = 512;

/// The tool list for `tools/list`.
pub(crate) fn list() -> Vec<Value> {
    let path = json!({ "type": "string", "description": "Path to a .oxypix document." });
    let frame = json!({ "type": "integer", "minimum": 0, "description": "Frame index, from 0. Defaults to 0." });
    let overwrite = json!({ "type": "boolean", "description": "Replace the output file if it exists. Defaults to false." });
    let point =
        json!({ "type": "array", "items": { "type": "integer" }, "minItems": 2, "maxItems": 2 });
    let color = json!({ "type": "string", "description": "#RRGGBB or #RRGGBBAA." });
    vec![
        tool(
            "document_info",
            "Document info",
            "Describe a .oxypix document: size, layers (bottom first), frames with durations and names, tags, and palette.",
            json!({ "path": path }),
            &["path"],
            true,
        ),
        tool(
            "render",
            "Render a frame",
            "Composite one frame and return it as a PNG image, scaled up with nearest neighbor so it is easy to see (about 512 pixels a side unless scale is given). Use it to check your drawing.",
            json!({
                "path": path,
                "frame": frame,
                "scale": { "type": "integer", "minimum": 1, "maximum": 64, "description": "Whole-number scale. Defaults to fitting about 512 pixels." },
            }),
            &["path"],
            true,
        ),
        tool(
            "create_document",
            "Create a document",
            "Create a new .oxypix document with one clear layer and one frame, using the default 16-color palette.",
            json!({
                "path": path,
                "width": { "type": "integer", "minimum": 1, "maximum": MAX_DIMENSION },
                "height": { "type": "integer", "minimum": 1, "maximum": MAX_DIMENSION },
                "background": { "type": "string", "description": "Optional #RRGGBB to fill the layer with. Clear when left out." },
                "overwrite": overwrite,
            }),
            &["path", "width", "height"],
            false,
        ),
        tool(
            "draw",
            "Draw",
            "Draw on one layer of one frame with the editor's own tools, then save. Operations run in order. Each is an object with op set to: \"pixels\" (points: [[x,y],...]), \"erase\" (points), \"line\" (from, to), \"rect\" or \"ellipse\" (from, to corners, optional filled), \"fill\" (at, optional tolerance 0-255 and global), \"outline\" (paints the clear pixels around the layer's art; optional corners), \"shadow\" (a hard drop shadow behind the art; offset [dx, dy], default [1, 1]), or \"replace\" (every pixel of exactly the old color becomes the color). Each operation may set its own color; otherwise the call's color is used. Brush size applies to pixels, erase, and line.",
            json!({
                "path": path,
                "frame": frame,
                "layer": { "type": "integer", "minimum": 0, "description": "Layer index, from 0 at the bottom. Defaults to the top layer." },
                "color": color,
                "brush": { "type": "integer", "minimum": 1, "maximum": MAX_BRUSH, "description": "Brush size. Defaults to 1." },
                "operations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "op": { "type": "string", "enum": ["pixels", "erase", "line", "rect", "ellipse", "fill", "outline", "shadow", "replace"] },
                            "old": color,
                            "corners": { "type": "boolean" },
                            "offset": point,
                            "points": { "type": "array", "items": point },
                            "from": point,
                            "to": point,
                            "at": point,
                            "filled": { "type": "boolean" },
                            "tolerance": { "type": "integer", "minimum": 0, "maximum": 255 },
                            "global": { "type": "boolean" },
                            "color": color,
                        },
                        "required": ["op"],
                    },
                },
            }),
            &["path", "operations"],
            false,
        ),
        tool(
            "add_layer",
            "Add a layer",
            "Add a clear layer, or an empty group, on top of the stack, in every frame. Use set_layer's group_into to move layers into a group.",
            json!({
                "path": path,
                "name": { "type": "string" },
                "group": { "type": "boolean", "description": "Add a group instead of a layer. A group holds the layers moved into it and is drawn as one picture with its own opacity and blend mode." },
            }),
            &["path"],
            false,
        ),
        tool(
            "set_layer",
            "Change a layer",
            "Rename a layer or group, show or hide it, change its opacity or blend mode, move it into or out of a group, or delete it.",
            json!({
                "path": path,
                "layer": { "type": "integer", "minimum": 0 },
                "name": { "type": "string" },
                "visible": { "type": "boolean" },
                "opacity": { "type": "integer", "minimum": 0, "maximum": 255 },
                "blend": { "type": "string", "enum": BlendMode::ALL.map(BlendMode::name), "description": "How the layer mixes with the layers below." },
                "delete": { "type": "boolean", "description": "Delete the layer, or a group with everything in it. The last drawable layer cannot be deleted." },
                "group_into": { "type": "boolean", "description": "Move the layer (or group) into the group just below it, as its top layer." },
                "group_out": { "type": "boolean", "description": "Move the layer (or group) out of its group, just above it." },
            }),
            &["path", "layer"],
            false,
        ),
        tool(
            "add_frame",
            "Add a frame",
            "Add a frame at the end, clear or as a copy of another frame (inserted after it).",
            json!({
                "path": path,
                "copy_of": { "type": "integer", "minimum": 0, "description": "Copy this frame instead of adding a clear one." },
                "duration_ms": { "type": "integer", "minimum": MIN_FRAME_MS, "maximum": MAX_FRAME_MS },
                "name": { "type": "string" },
            }),
            &["path"],
            false,
        ),
        tool(
            "set_frame",
            "Change a frame",
            "Rename a frame, change its duration, or delete it.",
            json!({
                "path": path,
                "frame": frame,
                "name": { "type": "string", "description": "An empty name removes it." },
                "duration_ms": { "type": "integer", "minimum": MIN_FRAME_MS, "maximum": MAX_FRAME_MS },
                "delete": { "type": "boolean", "description": "Delete the frame. The last frame cannot be deleted." },
            }),
            &["path", "frame"],
            false,
        ),
        tool(
            "set_tags",
            "Set animation tags",
            "Replace the document's animation tags. Frames count from 0 and to is included.",
            json!({
                "path": path,
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "from": { "type": "integer", "minimum": 0 },
                            "to": { "type": "integer", "minimum": 0 },
                            "direction": { "type": "string", "enum": ["forward", "reverse", "pingpong", "pingpong_reverse"] },
                            "repeat": { "type": "integer", "minimum": 0, "maximum": 65535, "description": "Times to play; 0 is forever." },
                        },
                        "required": ["name", "from", "to"],
                    },
                },
            }),
            &["path", "tags"],
            false,
        ),
        tool(
            "set_palette",
            "Set the palette",
            "Replace the document palette with a list of colors.",
            json!({ "path": path, "colors": { "type": "array", "items": color, "minItems": 1, "maxItems": MAX_PALETTE } }),
            &["path", "colors"],
            false,
        ),
        tool(
            "export",
            "Export",
            "Export a document: png or bmp (one frame), gif (all frames), or sheet (a spritesheet PNG, optionally with a JSON description, a Godot 4 SpriteFrames .tres, or a Unity .png.meta beside it). per_tag writes OUTPUT_<tag>.gif or .png for each animation tag instead.",
            json!({
                "path": path,
                "output": { "type": "string", "description": "File to write." },
                "format": { "type": "string", "enum": ["png", "bmp", "gif", "sheet"] },
                "frame": frame,
                "scale": { "type": "integer", "enum": [1, 2, 4], "description": "png and sheet only. Defaults to 1." },
                "columns": { "type": "integer", "minimum": 0, "description": "sheet only. 0 is one row." },
                "padding": { "type": "integer", "minimum": 0, "maximum": 256, "description": "sheet only." },
                "json": { "type": "boolean", "description": "sheet only: also write OUTPUT.json." },
                "godot": { "type": "boolean", "description": "sheet only: also write OUTPUT.tres, a Godot 4 SpriteFrames resource with one animation per tag." },
                "unity": { "type": "boolean", "description": "sheet only: also write OUTPUT.png.meta, Unity import settings with one sprite per frame." },
                "per_tag": { "type": "boolean", "description": "gif and sheet only: one file per tag, named OUTPUT_<tag>." },
                "overwrite": overwrite,
            }),
            &["path", "output", "format"],
            false,
        ),
        tool(
            "pack_sprites",
            "Pack sprites",
            "Pack the frames of image, .oxypix, and .aseprite files (or folders of them) into one atlas PNG with a JSON hash beside it, as the oxypack command does.",
            json!({
                "inputs": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                "output": { "type": "string", "description": "Atlas PNG to write." },
                "padding": { "type": "integer", "minimum": 0, "maximum": 256 },
                "max_width": { "type": "integer", "minimum": 1, "maximum": 16384 },
                "trim": { "type": "boolean" },
                "dedupe": { "type": "boolean" },
                "power_of_two": { "type": "boolean" },
                "packer": { "type": "string", "enum": ["shelf", "maxrects"] },
                "json": { "type": "boolean", "description": "Write the JSON hash. Defaults to true." },
                "overwrite": overwrite,
            }),
            &["inputs", "output"],
            false,
        ),
    ]
}

fn tool(
    name: &str,
    title: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
        "annotations": { "readOnlyHint": read_only, "openWorldHint": false },
    })
}

/// Run a tool. `None` when no tool has that name.
pub(crate) fn call(name: &str, args: &Value, paths: &Paths) -> Option<ToolResult> {
    let empty = Map::new();
    let args = args.as_object().unwrap_or(&empty);
    Some(match name {
        "document_info" => document_info(args, paths),
        "render" => render(args, paths),
        "create_document" => create_document(args, paths),
        "draw" => draw(args, paths),
        "add_layer" => add_layer(args, paths),
        "set_layer" => set_layer(args, paths),
        "add_frame" => add_frame(args, paths),
        "set_frame" => set_frame(args, paths),
        "set_tags" => set_tags(args, paths),
        "set_palette" => set_palette(args, paths),
        "export" => export(args, paths),
        "pack_sprites" => pack(args, paths),
        _ => return None,
    })
}

// ---- argument helpers -------------------------------------------------------

fn string<'a>(args: &'a Args, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}

fn opt_string<'a>(args: &'a Args, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a string")),
    }
}

fn opt_bool(args: &Args, key: &str) -> Result<Option<bool>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| format!("{key} must be true or false")),
    }
}

/// A whole number in `min..=max`.
fn opt_int(args: &Args, key: &str, min: i64, max: i64) -> Result<Option<i64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_i64() {
            Some(n) if (min..=max).contains(&n) => Ok(Some(n)),
            _ => Err(format!("{key} must be a whole number from {min} to {max}")),
        },
    }
}

fn int(args: &Args, key: &str, min: i64, max: i64) -> Result<i64, String> {
    opt_int(args, key, min, max)?.ok_or_else(|| format!("{key} is required"))
}

fn color(text: &str) -> Result<Rgba, String> {
    Rgba::parse_hex(text).ok_or_else(|| format!("{text} is not a #RRGGBB or #RRGGBBAA color"))
}

fn point(value: Option<&Value>, key: &str) -> Result<(i32, i32), String> {
    let limit = i64::from(MAX_DIMENSION) * 2;
    let coords = value
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .and_then(|a| Some((a[0].as_i64()?, a[1].as_i64()?)))
        .filter(|(x, y)| x.abs() <= limit && y.abs() <= limit)
        .ok_or_else(|| format!("{key} must be [x, y] with whole numbers"))?;
    Ok((coords.0 as i32, coords.1 as i32))
}

fn load(paths: &Paths, args: &Args) -> Result<(PathBuf, Document), String> {
    let path = paths.resolve(string(args, "path")?)?;
    let doc = load_oxypix(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok((path, doc))
}

fn save(path: &Path, doc: &Document) -> Result<(), String> {
    save_oxypix(doc, path).map_err(|err| format!("{}: {err}", path.display()))
}

fn frame_index(args: &Args, doc: &Document) -> Result<usize, String> {
    let last = doc.frame_count() as i64 - 1;
    Ok(opt_int(args, "frame", 0, last)
        .map_err(|_| format!("frame must be from 0 to {last}"))?
        .unwrap_or(0) as usize)
}

fn layer_index(args: &Args, doc: &Document, default_top: bool) -> Result<usize, String> {
    let last = doc.layer_count() as i64 - 1;
    match opt_int(args, "layer", 0, last).map_err(|_| format!("layer must be from 0 to {last}"))? {
        Some(n) => Ok(n as usize),
        None if default_top => Ok(last as usize),
        None => Err("layer is required".into()),
    }
}

/// An output path that is not there yet, or `overwrite` was given.
fn output(paths: &Paths, args: &Args, key: &str) -> Result<PathBuf, String> {
    let path = paths.resolve(string(args, key)?)?;
    if path.exists() && !opt_bool(args, "overwrite")?.unwrap_or(false) {
        return Err(format!(
            "{} already exists; pass overwrite: true to replace it",
            path.display()
        ));
    }
    Ok(path)
}

// ---- tools ------------------------------------------------------------------

fn info_json(doc: &Document) -> Value {
    let layers: Vec<Value> = doc
        .layers()
        .iter()
        .enumerate()
        .map(|(i, layer)| {
            json!({
                "index": i,
                "name": layer.name,
                "visible": layer.visible,
                "opacity": layer.opacity,
                "blend": layer.blend.name(),
                "group": layer.group,
                "parent": doc.layer_parent(i),
                "tileset": layer.tileset,
            })
        })
        .collect();
    let frames: Vec<Value> = (0..doc.frame_count())
        .map(|i| {
            json!({
                "index": i,
                "duration_ms": doc.frame_duration(i).unwrap_or(0),
                "name": doc.frame_name(i),
            })
        })
        .collect();
    let tags: Vec<Value> = doc
        .tags()
        .iter()
        .map(|tag| {
            json!({ "name": tag.name, "from": tag.from, "to": tag.to, "direction": tag.direction.json_name(), "repeat": tag.repeat })
        })
        .collect();
    let palette: Vec<String> = doc.palette().iter().map(|c| c.to_hex()).collect();
    let tilesets: Vec<Value> = doc
        .tilesets()
        .iter()
        .map(|set| {
            json!({ "name": set.name, "tile_width": set.tile_width(), "tile_height": set.tile_height(), "tiles": set.len() })
        })
        .collect();
    json!({
        "width": doc.width(),
        "height": doc.height(),
        "layers": layers,
        "frames": frames,
        "tags": tags,
        "palette": palette,
        "tilesets": tilesets,
    })
}

fn document_info(args: &Args, paths: &Paths) -> ToolResult {
    let (_, doc) = load(paths, args)?;
    serde_json::to_string_pretty(&info_json(&doc))
        .map(Output::text)
        .map_err(|err| err.to_string())
}

/// The largest whole-number scale that keeps both sides at most `target`, at least 1.
pub(crate) fn fit_scale(width: u32, height: u32, target: u32) -> u32 {
    (target / width.max(height).max(1)).max(1)
}

fn render(args: &Args, paths: &Paths) -> ToolResult {
    let (_, doc) = load(paths, args)?;
    let frame = frame_index(args, &doc)?;
    let scale = match opt_int(args, "scale", 1, 64)? {
        Some(n) => n as u32,
        None => fit_scale(doc.width(), doc.height(), RENDER_TARGET),
    };
    if u64::from(doc.width()) * u64::from(scale) > 4096
        || u64::from(doc.height()) * u64::from(scale) > 4096
    {
        return Err("that scale makes the image larger than 4096 pixels a side".into());
    }
    let pixels = doc.composite(frame).map_err(|err| err.to_string())?;
    let png = scaled_png(&pixels, doc.width(), doc.height(), scale)?;
    Ok(Output {
        text: format!(
            "Frame {frame} of {}, {}×{} shown at {scale}×.",
            counted(doc.frame_count(), "frame", "frames"),
            doc.width(),
            doc.height()
        ),
        png: Some(png),
    })
}

/// Nearest-neighbor `scale`× copy of straight-alpha pixels, as PNG bytes.
fn scaled_png(pixels: &[Rgba], width: u32, height: u32, scale: u32) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let (sw, sh) = (width * scale, height * scale);
    let mut rgba = Vec::with_capacity(sw as usize * sh as usize * 4);
    for y in 0..sh {
        let row = (y / scale) as usize * width as usize;
        for x in 0..sw {
            rgba.extend_from_slice(&pixels[row + (x / scale) as usize].to_bytes());
        }
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, sw, sh, image::ExtendedColorType::Rgba8)
        .map_err(|err| err.to_string())?;
    Ok(png)
}

fn create_document(args: &Args, paths: &Paths) -> ToolResult {
    let path = output(paths, args, "path")?;
    let max = i64::from(MAX_DIMENSION);
    let width = int(args, "width", 1, max)? as u32;
    let height = int(args, "height", 1, max)? as u32;
    let mut editor = Editor::new(width, height).map_err(|err| err.to_string())?;
    if let Some(background) = opt_string(args, "background")? {
        editor.set_primary(color(background)?);
        editor.set_tool(Tool::Bucket);
        editor.set_fill_global(true);
        editor.pointer_down(0, 0, MouseButton::Primary, false);
        editor.pointer_up(0, 0, false);
    }
    save(&path, editor.document())?;
    Ok(Output::text(format!(
        "Created {} ({width}×{height}, 1 layer, 1 frame).",
        path.display()
    )))
}

fn draw(args: &Args, paths: &Paths) -> ToolResult {
    let (path, doc) = load(paths, args)?;
    let frame = frame_index(args, &doc)?;
    let layer = layer_index(args, &doc, true)?;
    if doc.layer(layer).is_some_and(|l| l.group) {
        return Err(format!(
            "layer {layer} is a group and has no pixels; draw on a layer inside it"
        ));
    }
    let mut editor = Editor::from_document(doc);
    editor
        .set_active_frame(frame)
        .and_then(|()| editor.set_active_layer(layer))
        .map_err(|err| err.to_string())?;
    let brush = opt_int(args, "brush", 1, i64::from(MAX_BRUSH))?.unwrap_or(1) as u32;
    let base = opt_string(args, "color")?.map(color).transpose()?;
    let operations = args
        .get("operations")
        .and_then(Value::as_array)
        .ok_or("operations must be a list")?;
    if operations.len() > MAX_OPERATIONS {
        return Err(format!("at most {MAX_OPERATIONS} operations in one call"));
    }
    let mut points_used = 0usize;
    for (n, op) in operations.iter().enumerate() {
        let op = op
            .as_object()
            .ok_or_else(|| format!("operation {n} must be an object"))?;
        apply(&mut editor, op, base, brush, &mut points_used)
            .map_err(|err| format!("operation {n}: {err}"))?;
    }
    save(&path, editor.document())?;
    Ok(Output::text(format!(
        "Drew {} on layer {layer} of frame {frame} and saved {}.",
        counted(operations.len(), "operation", "operations"),
        path.display()
    )))
}

fn apply(
    editor: &mut Editor,
    op: &Args,
    base: Option<Rgba>,
    brush: u32,
    points_used: &mut usize,
) -> Result<(), String> {
    let kind = string(op, "op")?;
    let paint = || -> Result<Rgba, String> {
        match opt_string(op, "color")? {
            Some(text) => color(text),
            None => base.ok_or_else(|| "needs a color (on the operation or the call)".into()),
        }
    };
    let (w, h) = (
        editor.document().width() as i32,
        editor.document().height() as i32,
    );
    let inside = |(x, y): (i32, i32), key: &str| -> Result<(i32, i32), String> {
        if (0..w).contains(&x) && (0..h).contains(&y) {
            Ok((x, y))
        } else {
            Err(format!("{key} [{x}, {y}] is outside the {w}×{h} canvas"))
        }
    };
    editor.set_brush(brush);
    match kind {
        "pixels" | "erase" => {
            let points = op
                .get("points")
                .and_then(Value::as_array)
                .ok_or("points must be a list of [x, y]")?;
            *points_used += points.len();
            if *points_used > MAX_POINTS {
                return Err(format!("at most {MAX_POINTS} points in one call"));
            }
            if kind == "pixels" {
                editor.set_primary(paint()?);
                editor.set_tool(Tool::Pencil);
            } else {
                editor.set_tool(Tool::Eraser);
            }
            for value in points {
                let (x, y) = inside(point(Some(value), "point")?, "point")?;
                editor.pointer_down(x, y, MouseButton::Primary, false);
                editor.pointer_up(x, y, false);
            }
        }
        "line" | "rect" | "ellipse" => {
            let from = point(op.get("from"), "from")?;
            let to = point(op.get("to"), "to")?;
            editor.set_primary(paint()?);
            editor.set_tool(match kind {
                "line" => Tool::Line,
                "rect" => Tool::Rectangle,
                _ => Tool::Ellipse,
            });
            editor.set_shape_filled(opt_bool(op, "filled")?.unwrap_or(false));
            editor.pointer_down(from.0, from.1, MouseButton::Primary, false);
            editor.pointer_move(to.0, to.1, false);
            editor.pointer_up(to.0, to.1, false);
        }
        "fill" => {
            let (x, y) = inside(point(op.get("at"), "at")?, "at")?;
            editor.set_primary(paint()?);
            editor.set_tool(Tool::Bucket);
            editor.set_fill_tolerance(opt_int(op, "tolerance", 0, 255)?.unwrap_or(0) as u8);
            editor.set_fill_global(opt_bool(op, "global")?.unwrap_or(false));
            editor.pointer_down(x, y, MouseButton::Primary, false);
            editor.pointer_up(x, y, false);
        }
        "outline" => {
            let corners = opt_bool(op, "corners")?.unwrap_or(false);
            editor
                .outline(paint()?, corners)
                .map_err(|err| err.to_string())?;
        }
        "shadow" => {
            let (dx, dy) = match op.get("offset") {
                Some(value) => point(Some(value), "offset")?,
                None => (1, 1),
            };
            editor
                .drop_shadow(paint()?, dx, dy)
                .map_err(|err| err.to_string())?;
        }
        "replace" => {
            let old = color(string(op, "old")?)?;
            editor
                .replace_color(old, paint()?, false, false)
                .map_err(|err| err.to_string())?;
        }
        other => return Err(format!("unknown op {other}")),
    }
    Ok(())
}

fn add_layer(args: &Args, paths: &Paths) -> ToolResult {
    let (path, doc) = load(paths, args)?;
    let mut editor = Editor::from_document(doc);
    if opt_bool(args, "group")?.unwrap_or(false) {
        editor.add_group()
    } else {
        editor.add_layer()
    }
    .map_err(|err| err.to_string())?;
    if let Some(name) = opt_string(args, "name")? {
        editor.rename_layer(name);
    }
    let index = editor.document().active_layer();
    save(&path, editor.document())?;
    Ok(Output::text(format!("Added layer {index} and saved.")))
}

fn set_layer(args: &Args, paths: &Paths) -> ToolResult {
    let (path, doc) = load(paths, args)?;
    let layer = layer_index(args, &doc, false)?;
    let mut editor = Editor::from_document(doc);
    editor
        .set_active_layer(layer)
        .map_err(|err| err.to_string())?;
    if opt_bool(args, "delete")?.unwrap_or(false) {
        editor.delete_layer().map_err(|err| err.to_string())?;
        save(&path, editor.document())?;
        return Ok(Output::text(format!("Deleted layer {layer} and saved.")));
    }
    if let Some(name) = opt_string(args, "name")? {
        editor.rename_layer(name);
    }
    if let Some(visible) = opt_bool(args, "visible")? {
        editor.set_layer_visible(visible);
    }
    if let Some(opacity) = opt_int(args, "opacity", 0, 255)? {
        editor.set_layer_opacity(opacity as u8);
    }
    if opt_bool(args, "group_into")?.unwrap_or(false) {
        editor.indent_layer().map_err(|err| err.to_string())?;
    }
    if opt_bool(args, "group_out")?.unwrap_or(false) {
        editor.outdent_layer().map_err(|err| err.to_string())?;
    }
    if let Some(name) = opt_string(args, "blend")? {
        let mode =
            BlendMode::from_name(name).ok_or_else(|| format!("unknown blend mode {name}"))?;
        editor.set_layer_blend(mode);
    }
    save(&path, editor.document())?;
    Ok(Output::text(format!("Updated layer {layer} and saved.")))
}

fn add_frame(args: &Args, paths: &Paths) -> ToolResult {
    let (path, doc) = load(paths, args)?;
    let last = doc.frame_count() as i64 - 1;
    let copy_of = opt_int(args, "copy_of", 0, last)
        .map_err(|_| format!("copy_of must be from 0 to {last}"))?;
    let mut editor = Editor::from_document(doc);
    match copy_of {
        Some(source) => editor
            .set_active_frame(source as usize)
            .and_then(|()| editor.duplicate_frame()),
        None => editor
            .set_active_frame(last as usize)
            .and_then(|()| editor.add_frame()),
    }
    .map_err(|err| err.to_string())?;
    let index = editor.document().active_frame();
    if let Some(ms) = opt_int(
        args,
        "duration_ms",
        i64::from(MIN_FRAME_MS),
        i64::from(MAX_FRAME_MS),
    )? {
        editor.set_frame_duration(ms as u32);
    }
    if let Some(name) = opt_string(args, "name")? {
        editor.rename_frame(name);
    }
    save(&path, editor.document())?;
    Ok(Output::text(format!("Added frame {index} and saved.")))
}

fn set_frame(args: &Args, paths: &Paths) -> ToolResult {
    let (path, doc) = load(paths, args)?;
    let frame = frame_index(args, &doc)?;
    let mut editor = Editor::from_document(doc);
    editor
        .set_active_frame(frame)
        .map_err(|err| err.to_string())?;
    if opt_bool(args, "delete")?.unwrap_or(false) {
        editor.delete_frame().map_err(|err| err.to_string())?;
        save(&path, editor.document())?;
        return Ok(Output::text(format!("Deleted frame {frame} and saved.")));
    }
    if let Some(ms) = opt_int(
        args,
        "duration_ms",
        i64::from(MIN_FRAME_MS),
        i64::from(MAX_FRAME_MS),
    )? {
        editor.set_frame_duration(ms as u32);
    }
    if let Some(name) = opt_string(args, "name")? {
        editor.rename_frame(name);
    }
    save(&path, editor.document())?;
    Ok(Output::text(format!("Updated frame {frame} and saved.")))
}

fn set_tags(args: &Args, paths: &Paths) -> ToolResult {
    let (path, doc) = load(paths, args)?;
    let list = args
        .get("tags")
        .and_then(Value::as_array)
        .ok_or("tags must be a list")?;
    let mut tags = Vec::with_capacity(list.len());
    for (n, value) in list.iter().enumerate() {
        let tag = value
            .as_object()
            .ok_or_else(|| format!("tag {n} must be an object"))?;
        let last = doc.frame_count() as i64 - 1;
        let name = opt_string(tag, "direction")?.unwrap_or("forward");
        let direction = LoopDirection::ALL
            .into_iter()
            .find(|d| d.json_name() == name)
            .ok_or_else(|| format!("tag {n}: unknown direction {name}"))?;
        tags.push(FrameTag {
            name: string(tag, "name")?.to_string(),
            from: int(tag, "from", 0, last)? as usize,
            to: int(tag, "to", 0, last)? as usize,
            direction,
            repeat: opt_int(tag, "repeat", 0, i64::from(u16::MAX))?.unwrap_or(0) as u16,
        });
    }
    let mut editor = Editor::from_document(doc);
    while !editor.document().tags().is_empty() {
        editor.remove_tag(0).map_err(|err| err.to_string())?;
    }
    for (n, tag) in tags.into_iter().enumerate() {
        editor
            .add_tag(tag)
            .map_err(|err| format!("tag {n}: {err}"))?;
    }
    save(&path, editor.document())?;
    Ok(Output::text(format!(
        "Saved {}.",
        counted(editor.document().tags().len(), "tag", "tags")
    )))
}

fn set_palette(args: &Args, paths: &Paths) -> ToolResult {
    let (path, doc) = load(paths, args)?;
    let colors = args
        .get("colors")
        .and_then(Value::as_array)
        .ok_or("colors must be a list")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or("colors must be strings".to_string())
                .and_then(color)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut editor = Editor::from_document(doc);
    editor.set_palette(colors).map_err(|err| err.to_string())?;
    save(&path, editor.document())?;
    Ok(Output::text(format!(
        "Saved a palette of {}.",
        counted(editor.document().palette().len(), "color", "colors")
    )))
}

fn export(args: &Args, paths: &Paths) -> ToolResult {
    let (_, doc) = load(paths, args)?;
    let out = output(paths, args, "output")?;
    let format = string(args, "format")?;
    let frame = frame_index(args, &doc)?;
    let scale = opt_int(args, "scale", 1, 4)?.unwrap_or(1) as u32;
    if ![1, 2, 4].contains(&scale) {
        return Err("scale must be 1, 2, or 4".into());
    }
    let per_tag = opt_bool(args, "per_tag")?.unwrap_or(false);
    if per_tag {
        if !matches!(format, "gif" | "sheet") {
            return Err("per_tag works with gif and sheet".into());
        }
        if doc.tags().is_empty() {
            return Err("the document has no tags".into());
        }
        if !opt_bool(args, "overwrite")?.unwrap_or(false) {
            if let Some(taken) = tag_paths(&out, doc.tags()).iter().find(|p| p.exists()) {
                return Err(format!(
                    "{} already exists; pass overwrite: true to replace it",
                    taken.display()
                ));
            }
        }
    }
    let written = match format {
        "png" => export_png_scaled(&doc, frame, &out, scale).map(|()| vec![out.clone()]),
        "bmp" => export_bmp(&doc, frame, &out).map(|()| vec![out.clone()]),
        "gif" if per_tag => export_gif_per_tag(&doc, &out),
        "gif" => export_gif(&doc, &out).map(|()| vec![out.clone()]),
        "sheet" => {
            let layout = SheetLayout {
                scale,
                columns: opt_int(args, "columns", 0, i64::from(u32::MAX))?.unwrap_or(0) as u32,
                padding: opt_int(args, "padding", 0, 256)?.unwrap_or(0) as u32,
            };
            let files = SheetFiles {
                json: opt_bool(args, "json")?.unwrap_or(false),
                godot: opt_bool(args, "godot")?.unwrap_or(false),
                unity: opt_bool(args, "unity")?.unwrap_or(false),
            };
            export_sheet_files(&doc, layout, &out, files, per_tag)
        }
        other => {
            return Err(format!(
                "unknown format {other}; use png, bmp, gif, or sheet"
            ))
        }
    };
    let written = written.map_err(|err| err.to_string())?;
    let names: Vec<String> = written.iter().map(|p| p.display().to_string()).collect();
    Ok(Output::text(format!("Wrote {}.", names.join(", "))))
}

fn pack(args: &Args, paths: &Paths) -> ToolResult {
    let inputs = args
        .get("inputs")
        .and_then(Value::as_array)
        .filter(|list| !list.is_empty())
        .ok_or("inputs must be a list of files or folders")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| "inputs must be strings".to_string())
                .and_then(|text| paths.resolve(text))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let out = output(paths, args, "output")?;
    let defaults = PackOptions::default();
    let options = PackOptions {
        packer: match opt_string(args, "packer")?.unwrap_or("shelf") {
            "shelf" => Packer::Shelf,
            "maxrects" => Packer::MaxRects,
            other => return Err(format!("unknown packer {other}; use shelf or maxrects")),
        },
        padding: opt_int(args, "padding", 0, 256)?.map_or(defaults.padding, |n| n as u32),
        trim: opt_bool(args, "trim")?.unwrap_or(false),
        dedupe: opt_bool(args, "dedupe")?.unwrap_or(false),
        max_width: opt_int(args, "max_width", 1, 16_384)?.map_or(defaults.max_width, |n| n as u32),
        power_of_two: opt_bool(args, "power_of_two")?.unwrap_or(false),
    };
    let packed = pack_files(&inputs, options)?;
    write_atlas(&packed, &out, opt_bool(args, "json")?.unwrap_or(true))?;
    let mut text = format!(
        "Packed {} from {} into {}×{} {}.",
        counted(packed.atlas.placements.len(), "sprite", "sprites"),
        counted(packed.file_count, "file", "files"),
        packed.atlas.width,
        packed.atlas.height,
        out.display()
    );
    for note in &packed.notes {
        text.push_str("\nNote: ");
        text.push_str(note);
    }
    Ok(Output::text(text))
}

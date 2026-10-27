//! File, then Pack sprites: OxyPack in a window.
//!
//! Pick files and folders, set the options, press Pack to see the atlas, then
//! save the PNG and its JSON. The packing is the `oxypack` library, so the
//! window and the `oxypack` command make the same atlas from the same inputs.

use std::path::PathBuf;

use eframe::egui::{self, Color32, TextureHandle, TextureOptions, Vec2};
use oxypack::{counted, pack_files, packable, write_atlas, PackOptions, Packed, Packer};

use crate::native;

/// Largest side of the preview, in points.
const PREVIEW_SIDE: f32 = 320.0;

pub struct PackWindow {
    pub open: bool,
    inputs: Vec<PathBuf>,
    /// A path typed by hand, for when no system dialog is available.
    typed: String,
    output: String,
    options: PackOptions,
    json: bool,
    packed: Option<Packed>,
    texture: Option<TextureHandle>,
    /// The inputs or options changed since the last Pack.
    stale: bool,
    status: Option<Result<String, String>>,
}

impl Default for PackWindow {
    fn default() -> Self {
        Self {
            open: false,
            inputs: Vec::new(),
            typed: String::new(),
            output: String::new(),
            options: PackOptions::default(),
            json: true,
            packed: None,
            texture: None,
            stale: false,
            status: None,
        }
    }
}

/// Which file a path field names: the typed text, with surrounding quotes
/// and spaces removed, or `None` when nothing is left.
fn typed_path(text: &str) -> Option<PathBuf> {
    let trimmed = text.trim().trim_matches('"').trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// Add `path` unless it is already listed. Returns why it was not added.
fn add_input(inputs: &mut Vec<PathBuf>, path: PathBuf) -> Result<(), String> {
    if !path.is_dir() && !packable(&path) {
        return Err(format!(
            "{} is not a .png, .bmp, .gif, .oxypix, or .aseprite file, or a folder",
            path.display()
        ));
    }
    if !inputs.contains(&path) {
        inputs.push(path);
    }
    Ok(())
}

/// The atlas PNG path to save to: `.png` is added when the name has no extension.
fn output_path(text: &str) -> Option<PathBuf> {
    let path = typed_path(text)?;
    Some(if path.extension().is_none() {
        path.with_extension("png")
    } else {
        path
    })
}

impl PackWindow {
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Pack sprites")
            .open(&mut open)
            .resizable(true)
            .default_width(380.0)
            .max_width(560.0)
            .show(ctx, |ui| self.contents(ui));
        self.open = open;
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        ui.label(
            "Pack frames from image, .oxypix, and .aseprite files into one atlas PNG and a JSON description.",
        );
        ui.add_space(4.0);
        ui.strong("Inputs");
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("pack-inputs")
            .max_height(120.0)
            .show(ui, |ui| {
                if self.inputs.is_empty() {
                    ui.weak("Nothing yet. Add files or a folder.");
                }
                for (index, path) in self.inputs.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("−").on_hover_text("Remove").clicked() {
                            remove = Some(index);
                        }
                        let suffix = if path.is_dir() { " (folder)" } else { "" };
                        let full = path.display().to_string();
                        ui.add(egui::Label::new(format!("{full}{suffix}")).truncate())
                            .on_hover_text(full);
                    });
                }
            });
        if let Some(index) = remove {
            self.inputs.remove(index);
            self.stale = true;
        }
        ui.horizontal(|ui| {
            if ui.button("Add files…").clicked() {
                for path in native::pick_files(
                    "Add sprites",
                    "Sprites",
                    &["png", "bmp", "gif", "oxypix", "aseprite", "ase"],
                ) {
                    self.add(path);
                }
            }
            if ui.button("Add folder…").clicked() {
                if let Some(path) = native::pick_folder("Add a folder of sprites") {
                    self.add(path);
                }
            }
        });
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.typed)
                    .hint_text("or type a file or folder path")
                    .desired_width(260.0),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Add").clicked() || enter {
                if let Some(path) = typed_path(&self.typed) {
                    self.add(path);
                    self.typed.clear();
                }
            }
        });

        ui.separator();
        ui.strong("Options");
        let before = (self.options, self.json);
        egui::Grid::new("pack-options")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Packer");
                egui::ComboBox::from_id_salt("pack-packer")
                    .selected_text(match self.options.packer {
                        Packer::Shelf => "Rows",
                        Packer::MaxRects => "MaxRects",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.options.packer, Packer::Shelf, "Rows")
                            .on_hover_text("Rows, tallest sprite first. The default.");
                        ui.selectable_value(&mut self.options.packer, Packer::MaxRects, "MaxRects")
                            .on_hover_text("Fills the gaps rows leave. Usually shorter.");
                    });
                ui.end_row();
                ui.label("Padding");
                ui.add(egui::DragValue::new(&mut self.options.padding).range(0..=256))
                    .on_hover_text("Clear pixels between sprites.");
                ui.end_row();
                ui.label("Max width");
                ui.add(egui::DragValue::new(&mut self.options.max_width).range(1..=16384))
                    .on_hover_text("The atlas is never wider than this.");
                ui.end_row();
            });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.options.trim, "Trim").on_hover_text(
                "Crop each sprite to its non-clear pixels. The JSON keeps the offset.",
            );
            ui.checkbox(&mut self.options.dedupe, "Dedupe")
                .on_hover_text("Store sprites with the same pixels once.");
            ui.checkbox(&mut self.options.power_of_two, "Power of two")
                .on_hover_text("Round the atlas size up to powers of two.");
            ui.checkbox(&mut self.json, "Write JSON")
                .on_hover_text("Write <name>.json beside the PNG.");
        });
        if (self.options, self.json) != before {
            self.stale = true;
        }

        ui.separator();
        ui.horizontal(|ui| {
            let can_pack = !self.inputs.is_empty();
            if ui
                .add_enabled(can_pack, egui::Button::new("Pack"))
                .on_disabled_hover_text("Add a file or folder first.")
                .clicked()
            {
                self.pack(ui.ctx());
            }
            if let Some(packed) = &self.packed {
                let mut text = format!(
                    "{} from {}, {}×{}",
                    counted(packed.atlas.placements.len(), "sprite", "sprites"),
                    counted(packed.file_count, "file", "files"),
                    packed.atlas.width,
                    packed.atlas.height
                );
                if !packed.tags.is_empty() {
                    text.push_str(&format!(", {}", counted(packed.tags.len(), "tag", "tags")));
                }
                if self.stale {
                    text.push_str(" (out of date, press Pack)");
                }
                ui.label(text);
            }
        });
        if let (Some(packed), Some(texture)) = (&self.packed, &self.texture) {
            for note in &packed.notes {
                ui.colored_label(Color32::from_rgb(220, 170, 90), note);
            }
            let (w, h) = (packed.atlas.width as f32, packed.atlas.height as f32);
            let fit = (PREVIEW_SIDE / w.max(h)).min(8.0);
            // Whole-pixel steps when the atlas is small enough to scale up.
            let scale = if fit >= 1.0 { fit.floor() } else { fit };
            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                ui.image((texture.id(), Vec2::new(w * scale, h * scale)));
            });
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Save to");
            ui.add(
                egui::TextEdit::singleline(&mut self.output)
                    .hint_text("atlas.png")
                    .desired_width(220.0),
            );
            if ui.button("Browse…").clicked() {
                let start = typed_path(&self.output).and_then(|p| p.parent().map(PathBuf::from));
                if let Some(path) =
                    native::pick_path(true, "Save atlas", "PNG image", &["png"], start.as_deref())
                {
                    self.output = path.display().to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            let ready = self.packed.is_some() && output_path(&self.output).is_some();
            if ui
                .add_enabled(ready, egui::Button::new("Save atlas"))
                .on_disabled_hover_text("Pack first, and choose where to save.")
                .clicked()
            {
                self.save(ui.ctx());
            }
        });
        match &self.status {
            Some(Ok(text)) => {
                ui.label(text);
            }
            Some(Err(text)) => {
                ui.colored_label(Color32::from_rgb(220, 100, 90), text);
            }
            None => {}
        }
    }

    fn add(&mut self, path: PathBuf) {
        match add_input(&mut self.inputs, path) {
            Ok(()) => {
                self.stale = true;
                self.status = None;
            }
            Err(message) => self.status = Some(Err(message)),
        }
    }

    fn pack(&mut self, ctx: &egui::Context) {
        match pack_files(&self.inputs, self.options) {
            Ok(packed) => {
                let size = [packed.atlas.width as usize, packed.atlas.height as usize];
                let rgba: Vec<u8> = packed
                    .atlas
                    .pixels
                    .iter()
                    .flat_map(|px| [px.r, px.g, px.b, px.a])
                    .collect();
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                match &mut self.texture {
                    Some(texture) => texture.set(image, TextureOptions::NEAREST),
                    None => {
                        self.texture =
                            Some(ctx.load_texture("pack-atlas", image, TextureOptions::NEAREST))
                    }
                }
                self.packed = Some(packed);
                self.stale = false;
                self.status = None;
            }
            Err(message) => self.status = Some(Err(message)),
        }
    }

    /// Pack again when the inputs changed, then write the files.
    fn save(&mut self, ctx: &egui::Context) {
        if self.stale {
            self.pack(ctx);
            if self.stale {
                return;
            }
        }
        let (Some(packed), Some(path)) = (&self.packed, output_path(&self.output)) else {
            return;
        };
        self.status = Some(match write_atlas(packed, &path, self.json) {
            Ok(()) if self.json => Ok(format!(
                "Saved {} and {}",
                path.display(),
                path.with_extension("json").display()
            )),
            Ok(()) => Ok(format!("Saved {}", path.display())),
            Err(message) => Err(message),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_paths_lose_quotes_and_get_a_png_extension() {
        assert_eq!(
            typed_path("  \"C:\\art\\hero.aseprite\" "),
            Some(PathBuf::from("C:\\art\\hero.aseprite"))
        );
        assert_eq!(typed_path("   "), None);
        assert_eq!(
            output_path("out/atlas"),
            Some(PathBuf::from("out/atlas.png"))
        );
        assert_eq!(output_path("atlas.PNG"), Some(PathBuf::from("atlas.PNG")));
        assert_eq!(output_path(""), None);
    }

    #[test]
    fn inputs_take_sprite_files_and_folders_once() {
        let mut inputs = Vec::new();
        assert!(add_input(&mut inputs, PathBuf::from("hero.aseprite")).is_ok());
        assert!(add_input(&mut inputs, PathBuf::from("hero.aseprite")).is_ok());
        assert!(add_input(&mut inputs, std::env::temp_dir()).is_ok());
        assert!(add_input(&mut inputs, PathBuf::from("notes.txt")).is_err());
        assert_eq!(inputs.len(), 2);
    }
}

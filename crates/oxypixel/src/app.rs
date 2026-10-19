//! Editor window: menus, tools, layers, frames, and file dialogs.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText, ViewportCommand};
use oxypixel_core::{
    advance, decode_first_rgba, export_bmp, export_gif, export_png, export_spritesheet, fit_into,
    import_image, load_oxypix, save_oxypix, Editor, Fit, Rgba, Tool,
};

use crate::canvas::{self, ViewState};
use crate::native;
use crate::shortcuts::{self, Command};
use crate::theme::AMBER;

const PRESETS: &[u32] = &[16, 32, 64, 128, 256, 512];

enum Note {
    Info(String),
    Error(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogKind {
    New,
    Path,
    Unsaved,
    About,
    Shortcuts,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PathKind {
    Open,
    Save,
    ImportDocument,
    ImportLayer,
    ExportPng,
    ExportBmp,
    ExportGif,
    ExportSheet,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    New,
    Open,
    Import,
    Example,
    Quit,
}

pub struct OxyApp {
    editor: Editor,
    view: ViewState,
    save_path: Option<PathBuf>,
    note: Option<Note>,
    dialog: Option<DialogKind>,
    dialog_error: Option<String>,
    path_kind: PathKind,
    path_buf: String,
    pending: Option<Pending>,
    new_width: String,
    new_height: String,
    hex: String,
    swatch: usize,
    hover: Option<(i32, i32)>,
    playing: bool,
    play_acc: u32,
    last_time: Option<f64>,
    allow_quit: bool,
}

impl OxyApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        let mut app = Self {
            editor: Editor::default(),
            view: ViewState::default(),
            save_path: None,
            note: None,
            dialog: None,
            dialog_error: None,
            path_kind: PathKind::Open,
            path_buf: String::new(),
            pending: None,
            new_width: "64".to_string(),
            new_height: "64".to_string(),
            hex: String::new(),
            swatch: 5,
            hover: None,
            playing: false,
            play_acc: 0,
            last_time: None,
            allow_quit: false,
        };
        app.hex = app.editor.primary().to_hex();
        if let Some(path) = open {
            app.open_path(path);
        }
        app
    }

    fn title(&self) -> String {
        let name = self
            .save_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        if self.editor.is_dirty() {
            format!("OxyPixel - {name} *")
        } else {
            format!("OxyPixel - {name}")
        }
    }

    fn note_ok(&mut self, text: impl Into<String>) {
        self.note = Some(Note::Info(text.into()));
    }

    fn note_err(&mut self, err: impl std::fmt::Display) {
        self.note = Some(Note::Error(err.to_string()));
    }

    fn apply<T>(
        &mut self,
        edit: impl FnOnce(&mut Editor) -> Result<T, oxypixel_core::Error>,
    ) -> Option<T> {
        let result = edit(&mut self.editor);
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.note_err(err);
                None
            }
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match load_oxypix(&path) {
            Ok(doc) => {
                self.editor.replace_document(doc);
                self.save_path = Some(path);
                self.view.fit_next = true;
                self.playing = false;
                self.hex = self.editor.primary().to_hex();
                self.note = None;
            }
            Err(err) => self.note_err(err),
        }
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        match save_oxypix(self.editor.document(), &path) {
            Ok(()) => {
                self.editor.mark_clean();
                self.save_path = Some(path);
                self.note_ok("Saved.");
                true
            }
            Err(err) => {
                self.note_err(err);
                false
            }
        }
    }
}

impl eframe::App for OxyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(ViewportCommand::Title(self.title()));
        if ctx.input(|i| i.viewport().close_requested())
            && self.editor.is_dirty()
            && !self.allow_quit
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
            self.dialog = Some(DialogKind::Unsaved);
        }
        self.tick_playback(ctx);
        self.handle_shortcuts(ctx);
        self.top_bar(ctx);
        self.bottom_bar(ctx);
        self.tool_panel(ctx);
        self.side_panel(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            self.hover = canvas::show_canvas(ui, &mut self.editor, &mut self.view, !self.playing);
        });
        self.show_dialog(ctx);
    }
}

impl OxyApp {
    fn tick_playback(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.playing {
            if let Some(prev) = self.last_time {
                let dt = ((now - prev) * 1000.0).clamp(0.0, 200.0) as u32;
                let count = self.editor.document().frame_count();
                let durations: Vec<u32> = (0..count)
                    .map(|i| self.editor.document().frame_duration(i).unwrap_or(100))
                    .collect();
                let current = self.editor.document().active_frame();
                let (next, acc) = advance(current, &durations, self.play_acc, dt);
                self.play_acc = acc;
                if next != current {
                    let _ = self.editor.set_active_frame(next);
                }
            }
            ctx.request_repaint();
        }
        self.last_time = Some(now);
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            if escape {
                self.dialog = None;
                self.pending = None;
                self.dialog_error = None;
            }
            return;
        }
        if ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                ctx.memory_mut(|m| {
                    if let Some(id) = m.focused() {
                        m.surrender_focus(id);
                    }
                });
            }
            return;
        }
        let commands: Vec<Command> = ctx.input(|input| {
            let mut commands = Vec::new();
            for event in &input.raw.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } = event
                {
                    if let Some(command) = shortcuts::command_from(*modifiers, *key) {
                        if *repeat && !shortcuts::allows_repeat(command) {
                            continue;
                        }
                        commands.push(command);
                    }
                }
            }
            commands
        });
        for command in commands {
            self.run_command(ctx, command);
        }
    }

    fn run_command(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::New => self.request_new(),
            Command::Open => self.request_open(),
            Command::Save => self.request_save(),
            Command::SaveAs => self.open_path_dialog(PathKind::Save),
            Command::Quit => self.request_quit(ctx),
            Command::Undo => {
                self.editor.undo();
            }
            Command::Redo => {
                self.editor.redo();
            }
            Command::SelectAll => self.editor.select_all(),
            Command::Deselect => self.editor.deselect(),
            Command::Escape => {
                if self.playing {
                    self.playing = false;
                } else {
                    self.editor.cancel_gesture();
                    self.editor.deselect();
                }
            }
            Command::ZoomIn => self.view.zoom_center(1.1),
            Command::ZoomOut => self.view.zoom_center(1.0 / 1.1),
            Command::ZoomFit => {
                let w = self.editor.document().width();
                let h = self.editor.document().height();
                self.view.fit(w, h);
            }
            Command::BrushSmaller => self.editor.nudge_brush(-1),
            Command::BrushLarger => self.editor.nudge_brush(1),
            Command::SwapColors => {
                self.editor.swap_colors();
                self.hex = self.editor.primary().to_hex();
            }
            Command::Tool(tool) => self.editor.set_tool(tool),
            Command::PlayToggle => self.toggle_play(),
            Command::PrevFrame => self.step_frame(-1),
            Command::NextFrame => self.step_frame(1),
            Command::AddFrame => {
                self.apply(|editor| editor.add_frame());
            }
            Command::AddLayer => {
                self.apply(|editor| editor.add_layer());
            }
            Command::DeleteSelection => {
                self.apply(|editor| editor.clear_selection_pixels());
            }
        }
    }

    fn toggle_play(&mut self) {
        self.playing = !self.playing;
        self.play_acc = 0;
        if self.playing {
            self.editor.cancel_gesture();
        }
    }

    fn step_frame(&mut self, delta: i32) {
        self.playing = false;
        let count = self.editor.document().frame_count() as i32;
        if count == 0 {
            return;
        }
        let next =
            (self.editor.document().active_frame() as i32 + delta).rem_euclid(count) as usize;
        let _ = self.editor.set_active_frame(next);
    }

    fn request_new(&mut self) {
        if self.editor.is_dirty() {
            self.pending = Some(Pending::New);
            self.dialog = Some(DialogKind::Unsaved);
        } else {
            self.dialog_error = None;
            self.dialog = Some(DialogKind::New);
        }
    }

    fn request_open(&mut self) {
        if self.editor.is_dirty() {
            self.pending = Some(Pending::Open);
            self.dialog = Some(DialogKind::Unsaved);
        } else {
            self.open_path_dialog(PathKind::Open);
        }
    }

    fn request_import(&mut self) {
        if self.editor.is_dirty() {
            self.pending = Some(Pending::Import);
            self.dialog = Some(DialogKind::Unsaved);
        } else {
            self.open_path_dialog(PathKind::ImportDocument);
        }
    }

    fn request_example(&mut self) {
        if self.editor.is_dirty() {
            self.pending = Some(Pending::Example);
            self.dialog = Some(DialogKind::Unsaved);
        } else {
            self.load_example();
        }
    }

    fn request_quit(&mut self, ctx: &egui::Context) {
        if self.editor.is_dirty() {
            self.pending = Some(Pending::Quit);
            self.dialog = Some(DialogKind::Unsaved);
        } else {
            self.allow_quit = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn request_save(&mut self) {
        if let Some(path) = self.save_path.clone() {
            let pending = self.pending;
            if self.save_to(path) {
                if let Some(pending) = pending {
                    self.finish(pending);
                }
            }
        } else {
            self.open_path_dialog(PathKind::Save);
        }
    }

    fn load_example(&mut self) {
        match Editor::demo() {
            Ok(editor) => {
                let doc = editor.document().clone();
                self.editor.replace_document(doc);
                self.save_path = None;
                self.view.fit_next = true;
                self.playing = false;
                self.hex = self.editor.primary().to_hex();
                self.note_ok("Opened the built-in example.");
            }
            Err(err) => self.note_err(err),
        }
    }

    fn finish(&mut self, pending: Pending) {
        self.pending = None;
        self.dialog = None;
        match pending {
            Pending::New => {
                self.dialog_error = None;
                self.dialog = Some(DialogKind::New);
            }
            Pending::Open => self.open_path_dialog(PathKind::Open),
            Pending::Import => self.open_path_dialog(PathKind::ImportDocument),
            Pending::Example => self.load_example(),
            Pending::Quit => {
                self.allow_quit = true;
                self.pending = Some(Pending::Quit);
            }
        }
    }

    fn open_path_dialog(&mut self, kind: PathKind) {
        self.path_kind = kind;
        self.dialog_error = None;
        self.path_buf = self.suggest_path(kind);
        self.dialog = Some(DialogKind::Path);
    }

    fn suggest_path(&self, kind: PathKind) -> String {
        let name = match kind {
            PathKind::Open | PathKind::ImportDocument | PathKind::ImportLayer => String::new(),
            PathKind::Save => self
                .save_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "untitled.oxypix".to_string()),
            PathKind::ExportPng => "frame.png".to_string(),
            PathKind::ExportBmp => "frame.bmp".to_string(),
            PathKind::ExportGif => "animation.gif".to_string(),
            PathKind::ExportSheet => "sheet.png".to_string(),
        };
        if let Some(parent) = self.save_path.as_ref().and_then(|p| p.parent()) {
            if !name.is_empty() && kind != PathKind::Save {
                return parent.join(name).display().to_string();
            }
        }
        name
    }

    fn confirm_path(&mut self) {
        let path = PathBuf::from(self.path_buf.trim());
        if path.as_os_str().is_empty() {
            self.dialog_error = Some("Enter a file path.".to_string());
            return;
        }
        let kind = self.path_kind;
        let pending = self.pending;
        let ok = match kind {
            PathKind::Open => {
                self.open_path(path);
                self.note.is_none() || matches!(self.note, Some(Note::Info(_)))
            }
            PathKind::Save => self.save_to(path),
            PathKind::ImportDocument => self.import_document(&path),
            PathKind::ImportLayer => self.import_layer(&path),
            PathKind::ExportPng => self.export_frame(&path, export_png),
            PathKind::ExportBmp => self.export_frame(&path, export_bmp),
            PathKind::ExportGif => self.export_doc(&path, export_gif),
            PathKind::ExportSheet => self.export_doc(&path, export_spritesheet),
        };
        if ok {
            self.dialog = None;
            self.dialog_error = None;
            if matches!(kind, PathKind::Save) {
                if let Some(pending) = pending {
                    self.finish(pending);
                }
            } else {
                self.pending = None;
            }
        } else if self.dialog_error.is_none() {
            if let Some(Note::Error(text)) = &self.note {
                self.dialog_error = Some(text.clone());
            }
        }
    }

    fn import_document(&mut self, path: &Path) -> bool {
        match import_image(path) {
            Ok(doc) => {
                self.editor.replace_document(doc);
                self.save_path = None;
                self.view.fit_next = true;
                self.playing = false;
                self.note_ok(format!("Imported {}.", path.display()));
                true
            }
            Err(err) => {
                self.dialog_error = Some(err.to_string());
                false
            }
        }
    }

    fn import_layer(&mut self, path: &Path) -> bool {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.dialog_error = Some(err.to_string());
                return false;
            }
        };
        let (w, h, pixels) = match decode_first_rgba(&bytes) {
            Ok(value) => value,
            Err(err) => {
                self.dialog_error = Some(err.to_string());
                return false;
            }
        };
        let (fitted, fit) = match fit_into(
            w,
            h,
            &pixels,
            self.editor.document().width(),
            self.editor.document().height(),
        ) {
            Ok(value) => value,
            Err(err) => {
                self.dialog_error = Some(err.to_string());
                return false;
            }
        };
        if self
            .apply(|editor| editor.insert_layer_pixels("Imported", &fitted))
            .is_none()
        {
            return false;
        }
        self.note_ok(match fit {
            Fit::Exact => "Imported the image as a new layer.".to_string(),
            Fit::Cropped => {
                "Imported the image. Pixels outside the canvas were cropped.".to_string()
            }
            Fit::Padded => {
                "Imported the image. Empty canvas pixels stayed transparent.".to_string()
            }
            Fit::CroppedAndPadded => {
                "Imported the image, cropped on one axis and padded on the other.".to_string()
            }
        });
        true
    }

    fn export_frame(
        &mut self,
        path: &Path,
        write: fn(&oxypixel_core::Document, usize, &Path) -> Result<(), oxypixel_core::Error>,
    ) -> bool {
        let frame = self.editor.document().active_frame();
        match write(self.editor.document(), frame, path) {
            Ok(()) => {
                self.note_ok(format!("Exported {}.", path.display()));
                true
            }
            Err(err) => {
                self.dialog_error = Some(err.to_string());
                false
            }
        }
    }

    fn export_doc(
        &mut self,
        path: &Path,
        write: fn(&oxypixel_core::Document, &Path) -> Result<(), oxypixel_core::Error>,
    ) -> bool {
        match write(self.editor.document(), path) {
            Ok(()) => {
                self.note_ok(format!("Exported {}.", path.display()));
                true
            }
            Err(err) => {
                self.dialog_error = Some(err.to_string());
                false
            }
        }
    }

    fn create_document(&mut self) {
        let width: u32 = match self.new_width.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                self.dialog_error = Some("Width must be a whole number.".to_string());
                return;
            }
        };
        let height: u32 = match self.new_height.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                self.dialog_error = Some("Height must be a whole number.".to_string());
                return;
            }
        };
        match Editor::new(width, height) {
            Ok(editor) => {
                let doc = editor.document().clone();
                self.editor.replace_document(doc);
                self.save_path = None;
                self.view.fit_next = true;
                self.playing = false;
                self.hex = self.editor.primary().to_hex();
                self.dialog = None;
                self.dialog_error = None;
                self.note_ok(format!("New {width}×{height} document."));
            }
            Err(err) => self.dialog_error = Some(err.to_string()),
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if menu_item(ui, "New…", "Ctrl+N") {
                        self.request_new();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Open…", "Ctrl+O") {
                        self.request_open();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Open example", "") {
                        self.request_example();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Save", "Ctrl+S") {
                        self.request_save();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Save as…", "Ctrl+Shift+S") {
                        self.open_path_dialog(PathKind::Save);
                        ui.close_menu();
                    }
                    ui.separator();
                    if menu_item(ui, "Import image as document…", "") {
                        self.request_import();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Import image as layer…", "") {
                        self.open_path_dialog(PathKind::ImportLayer);
                        ui.close_menu();
                    }
                    if menu_item(ui, "Export PNG…", "") {
                        self.open_path_dialog(PathKind::ExportPng);
                        ui.close_menu();
                    }
                    if menu_item(ui, "Export BMP…", "") {
                        self.open_path_dialog(PathKind::ExportBmp);
                        ui.close_menu();
                    }
                    if menu_item(ui, "Export GIF…", "") {
                        self.open_path_dialog(PathKind::ExportGif);
                        ui.close_menu();
                    }
                    if menu_item(ui, "Export spritesheet…", "") {
                        self.open_path_dialog(PathKind::ExportSheet);
                        ui.close_menu();
                    }
                    ui.separator();
                    if menu_item(ui, "Quit", "Ctrl+Q") {
                        self.request_quit(ui.ctx());
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if menu_item_enabled(ui, "Undo", "Ctrl+Z", self.editor.can_undo()) {
                        self.editor.undo();
                        ui.close_menu();
                    }
                    if menu_item_enabled(ui, "Redo", "Ctrl+Y", self.editor.can_redo()) {
                        self.editor.redo();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Swap colors", "X") {
                        self.editor.swap_colors();
                        self.hex = self.editor.primary().to_hex();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Select all", "Ctrl+A") {
                        self.editor.select_all();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Deselect", "Ctrl+D") {
                        self.editor.deselect();
                        ui.close_menu();
                    }
                    if menu_item(ui, "Clear selection", "Delete") {
                        self.apply(|editor| editor.clear_selection_pixels());
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if menu_item(ui, "Zoom in", "Ctrl+Plus") {
                        self.view.zoom_center(1.1);
                        ui.close_menu();
                    }
                    if menu_item(ui, "Zoom out", "Ctrl+Minus") {
                        self.view.zoom_center(1.0 / 1.1);
                        ui.close_menu();
                    }
                    if menu_item(ui, "Fit", "Ctrl+0") {
                        self.view.fit(
                            self.editor.document().width(),
                            self.editor.document().height(),
                        );
                        ui.close_menu();
                    }
                    if ui
                        .button(if self.view.grid {
                            "Hide grid"
                        } else {
                            "Show grid"
                        })
                        .clicked()
                    {
                        self.view.grid = !self.view.grid;
                        ui.close_menu();
                    }
                    if ui
                        .button(if self.view.onion {
                            "Hide onion skin"
                        } else {
                            "Show onion skin"
                        })
                        .clicked()
                    {
                        self.view.onion = !self.view.onion;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Shortcuts").clicked() {
                        self.dialog = Some(DialogKind::Shortcuts);
                        ui.close_menu();
                    }
                    if ui.button("About").clicked() {
                        self.dialog = Some(DialogKind::About);
                        ui.close_menu();
                    }
                });
            });
            let banner = self.note.as_ref().map(|note| match note {
                Note::Info(text) => (text.clone(), AMBER),
                Note::Error(text) => (text.clone(), Color32::from_rgb(220, 100, 90)),
            });
            if let Some((text, color)) = banner {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).color(color));
                    if ui.small_button("dismiss").clicked() {
                        self.note = None;
                    }
                });
            }
        });
    }

    fn tool_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("tools")
            .exact_width(156.0)
            .show(ctx, |ui| {
                ui.heading("Tools");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.editor.can_undo(), egui::Button::new("Undo"))
                        .on_hover_text("Ctrl+Z")
                        .on_disabled_hover_text("Ctrl+Z")
                        .clicked()
                    {
                        self.editor.undo();
                    }
                    if ui
                        .add_enabled(self.editor.can_redo(), egui::Button::new("Redo"))
                        .on_hover_text("Ctrl+Y")
                        .on_disabled_hover_text("Ctrl+Y")
                        .clicked()
                    {
                        self.editor.redo();
                    }
                });
                self.tool_button(ui, Tool::Pencil, "Pencil", "B");
                self.tool_button(ui, Tool::Eraser, "Eraser", "E");
                self.tool_button(ui, Tool::Bucket, "Fill", "G");
                self.tool_button(ui, Tool::Eyedropper, "Pick", "I");
                self.tool_button(ui, Tool::Line, "Line", "L");
                self.tool_button(ui, Tool::Rectangle, "Rect", "R");
                self.tool_button(ui, Tool::Ellipse, "Ellipse", "O");
                self.tool_button(ui, Tool::Select, "Select", "M");
                ui.separator();
                let mut size = self.editor.brush();
                if ui
                    .add(
                        egui::Slider::new(&mut size, 1..=oxypixel_core::MAX_BRUSH)
                            .suffix(" px")
                            .text("Brush"),
                    )
                    .on_hover_text("Brush size. [ and ] change it too.")
                    .changed()
                {
                    self.editor.set_brush(size);
                }
                let shapes = matches!(self.editor.tool(), Tool::Rectangle | Tool::Ellipse);
                let mut filled = self.editor.shape_filled();
                let fill = ui
                    .add_enabled(shapes, egui::Checkbox::new(&mut filled, "Fill shape"))
                    .on_hover_text("Fill the rectangle or ellipse instead of drawing an outline.")
                    .on_disabled_hover_text("Switch to rectangle or ellipse to fill a shape.");
                if fill.changed() {
                    self.editor.set_shape_filled(filled);
                }
                ui.separator();
                ui.label(egui::RichText::new(tool_hint(self.editor.tool())).small());
                ui.label(egui::RichText::new("Alt picks a color. Space pans.").small());
            });
    }

    fn tool_button(&mut self, ui: &mut egui::Ui, tool: Tool, label: &str, shortcut: &str) {
        let selected = self.editor.tool() == tool;
        let mut button = egui::Button::new(format!("{label}  {shortcut}"));
        if selected {
            button = button.fill(Color32::from_rgb(78, 62, 32));
        }
        if ui
            .add_sized([140.0, 28.0], button)
            .on_hover_text(tool_hint(tool))
            .clicked()
        {
            self.editor.set_tool(tool);
        }
    }

    fn side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("side")
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Color");
                ui.horizontal(|ui| {
                    self.color_edit(ui, true);
                    self.color_edit(ui, false);
                    if ui.button("Swap").clicked() {
                        self.editor.swap_colors();
                        self.hex = self.editor.primary().to_hex();
                    }
                });
                let hex_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.hex)
                        .desired_width(120.0)
                        .hint_text("#RRGGBB"),
                );
                let submit_hex = hex_resp.lost_focus()
                    || (hex_resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if submit_hex {
                    if let Some(color) = Rgba::parse_hex(&self.hex) {
                        self.editor.set_primary(color);
                        self.hex = color.to_hex();
                    } else {
                        self.note_err("Enter a color as #RRGGBB or #RRGGBBAA.");
                        self.hex = self.editor.primary().to_hex();
                    }
                    hex_resp.surrender_focus();
                } else if !hex_resp.has_focus() {
                    self.hex = self.editor.primary().to_hex();
                }
                ui.label("Palette");
                let colors: Vec<Rgba> = self.editor.document().palette().to_vec();
                ui.horizontal_wrapped(|ui| {
                    for (index, color) in colors.iter().copied().enumerate() {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a),
                        );
                        if index == self.swatch {
                            ui.painter().rect_stroke(
                                rect,
                                2.0,
                                egui::Stroke::new(1.0_f32, AMBER),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let response = response.on_hover_text(format!(
                            "{}\nClick for the primary color. Right-click for the secondary color.",
                            color.to_hex()
                        ));
                        if response.clicked() {
                            self.editor.set_primary(color);
                            self.swatch = index;
                            self.hex = color.to_hex();
                        }
                        if response.secondary_clicked() {
                            self.editor.set_secondary(color);
                        }
                    }
                });
                ui.label(
                    egui::RichText::new("Right-click a swatch to set the second color.").small(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Add").clicked() {
                        let color = self.editor.primary();
                        self.apply(|editor| editor.add_palette_color(color));
                    }
                    if ui.button("Remove").clicked() {
                        let swatch = self.swatch;
                        self.apply(|editor| editor.remove_palette_color(swatch));
                        self.swatch = self
                            .swatch
                            .min(self.editor.document().palette().len().saturating_sub(1));
                    }
                });
                ui.separator();
                ui.heading("Layers");
                let count = self.editor.document().layer_count();
                let active = self.editor.document().active_layer();
                for index in (0..count).rev() {
                    let Some(layer) = self.editor.document().layer(index).cloned() else {
                        continue;
                    };
                    ui.horizontal(|ui| {
                        let mut visible = layer.visible;
                        if ui
                            .checkbox(&mut visible, "")
                            .on_hover_text("Show or hide this layer")
                            .changed()
                        {
                            let _ = self.editor.set_layer_visible_at(index, visible);
                        }
                        if index == active {
                            let mut name = layer.name.clone();
                            let resp =
                                ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0));
                            if resp.gained_focus() {
                                self.editor.end_coalesce();
                                self.editor.begin_coalesce();
                            }
                            if resp.changed() {
                                self.editor.rename_layer(&name);
                            }
                            if resp.lost_focus() {
                                self.editor.end_coalesce();
                            }
                        } else if ui
                            .selectable_label(false, layer.name)
                            .on_hover_text("Draw on this layer")
                            .clicked()
                        {
                            let _ = self.editor.set_active_layer(index);
                        }
                    });
                }
                if let Some(layer) = self.editor.document().layer(active) {
                    let mut opacity = layer.opacity;
                    let slider = ui.add(egui::Slider::new(&mut opacity, 0..=255).text("Opacity"));
                    if slider.drag_started() || slider.gained_focus() {
                        self.editor.begin_coalesce();
                    }
                    if slider.changed() {
                        self.editor.set_layer_opacity(opacity);
                    }
                    if slider.drag_stopped() || slider.lost_focus() {
                        self.editor.end_coalesce();
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button("Add")
                        .on_hover_text("Add a layer on top")
                        .clicked()
                    {
                        self.apply(|editor| editor.add_layer());
                    }
                    if ui
                        .button("Copy")
                        .on_hover_text("Duplicate this layer")
                        .clicked()
                    {
                        self.apply(|editor| editor.duplicate_layer());
                    }
                    if ui
                        .button("Delete")
                        .on_hover_text("Delete this layer")
                        .clicked()
                    {
                        self.apply(|editor| editor.delete_layer());
                    }
                    if ui
                        .button("Up")
                        .on_hover_text("Move this layer up")
                        .clicked()
                    {
                        self.apply(|editor| editor.move_layer(1));
                    }
                    if ui
                        .button("Down")
                        .on_hover_text("Move this layer down")
                        .clicked()
                    {
                        self.apply(|editor| editor.move_layer(-1));
                    }
                    if ui
                        .button("Merge")
                        .on_hover_text("Merge this layer into the one below")
                        .clicked()
                    {
                        self.apply(|editor| editor.merge_down());
                    }
                });
            });
    }

    fn color_edit(&mut self, ui: &mut egui::Ui, primary: bool) {
        let current = if primary {
            self.editor.primary()
        } else {
            self.editor.secondary()
        };
        let mut color = Color32::from_rgba_unmultiplied(current.r, current.g, current.b, current.a);
        let label = if primary { "Primary" } else { "Secondary" };
        if ui
            .color_edit_button_srgba(&mut color)
            .on_hover_text(label)
            .changed()
        {
            let next = Rgba::new(color.r(), color.g(), color.b(), color.a());
            if primary {
                self.editor.set_primary(next);
                self.hex = next.to_hex();
            } else {
                self.editor.set_secondary(next);
            }
        }
    }

    fn bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(if self.playing { "Pause" } else { "Play" })
                    .on_hover_text("Enter")
                    .clicked()
                {
                    self.toggle_play();
                }
                if ui.button("Prev").on_hover_text("Previous frame").clicked() {
                    self.step_frame(-1);
                }
                if ui.button("Next").on_hover_text("Next frame").clicked() {
                    self.step_frame(1);
                }
                let count = self.editor.document().frame_count();
                let active = self.editor.document().active_frame();
                let strip = (ui.available_width() - 560.0).clamp(72.0, 320.0);
                egui::ScrollArea::horizontal()
                    .id_salt("frame-strip")
                    .max_width(strip)
                    .max_height(28.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for index in 0..count {
                                let label = format!("{}", index + 1);
                                let selected = index == active;
                                if ui
                                    .selectable_label(selected, label)
                                    .on_hover_text(format!("Frame {}", index + 1))
                                    .clicked()
                                {
                                    self.playing = false;
                                    let _ = self.editor.set_active_frame(index);
                                }
                            }
                        });
                    });
                if ui.button("Add frame").clicked() {
                    self.apply(|editor| editor.add_frame());
                }
                if ui.button("Copy frame").clicked() {
                    self.apply(|editor| editor.duplicate_frame());
                }
                if ui.button("Delete frame").clicked() {
                    self.apply(|editor| editor.delete_frame());
                }
                let mut ms = self.editor.document().frame_duration(active).unwrap_or(100);
                let slider = ui.add(egui::Slider::new(&mut ms, 20..=1000).text("ms"));
                if slider.drag_started() {
                    self.editor.begin_coalesce();
                }
                if slider.changed() {
                    self.editor.set_frame_duration(ms);
                }
                if slider.drag_stopped() {
                    self.editor.end_coalesce();
                }
                let mut onion = self.view.onion;
                if ui
                    .checkbox(&mut onion, "Onion")
                    .on_hover_text("Show the previous frame faintly under this one")
                    .changed()
                {
                    self.view.onion = onion;
                }
            });
            self.status_line(ui);
        });
    }

    fn status_line(&mut self, ui: &mut egui::Ui) {
        let width = self.editor.document().width();
        let height = self.editor.document().height();
        let layers = self.editor.document().layer_count();
        let layer = self.editor.document().active_layer() + 1;
        let frames = self.editor.document().frame_count();
        let frame = self.editor.document().active_frame() + 1;
        let tool = tool_name(self.editor.tool());
        let zoom = (self.view.zoom * 100.0).round() as i32;
        let hover = self.hovered_color();
        ui.horizontal(|ui| {
            ui.label(format!("{width}×{height}"));
            if ui.small_button("−").on_hover_text("Zoom out").clicked() {
                self.view.zoom_center(1.0 / 1.1);
            }
            ui.label(format!("{zoom}%"));
            if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                self.view.zoom_center(1.1);
            }
            if ui
                .small_button("Fit")
                .on_hover_text("Fit the canvas")
                .clicked()
            {
                self.view.fit(width, height);
            }
            let mut grid = self.view.grid;
            if ui
                .checkbox(&mut grid, "Grid")
                .on_hover_text("Show pixel grid when zoomed in")
                .changed()
            {
                self.view.grid = grid;
            }
            ui.separator();
            if let Some((x, y, color)) = hover {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    2.0,
                    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a),
                );
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.0_f32, Color32::from_gray(96)),
                    egui::StrokeKind::Inside,
                );
                let name = if color.a == 0 {
                    "clear".to_string()
                } else {
                    color.to_hex()
                };
                ui.label(format!("{x},{y}  {name}"));
            } else {
                ui.label("pixel —");
            }
            ui.separator();
            ui.label(format!(
                "{tool}   layer {layer}/{layers}   frame {frame}/{frames}"
            ));
        });
    }

    fn hovered_color(&self) -> Option<(i32, i32, Rgba)> {
        let (x, y) = self.hover?;
        if x < 0 || y < 0 {
            return None;
        }
        let doc = self.editor.document();
        let color = doc.composite_pixel(doc.active_frame(), x as u32, y as u32)?;
        Some((x, y, color))
    }

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.dialog else {
            return;
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("blocker"),
        ));
        painter.rect_filled(
            ctx.screen_rect(),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 120),
        );
        match kind {
            DialogKind::New => self.dialog_new(ctx),
            DialogKind::Path => self.dialog_path(ctx),
            DialogKind::Unsaved => self.dialog_unsaved(ctx),
            DialogKind::About => self.dialog_about(ctx),
            DialogKind::Shortcuts => self.dialog_shortcuts(ctx),
        }
        if self.allow_quit && matches!(self.pending, Some(Pending::Quit)) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn dialog_new(&mut self, ctx: &egui::Context) {
        let mut close = false;
        egui::Window::new("New document")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Width");
                    ui.add(egui::TextEdit::singleline(&mut self.new_width).desired_width(80.0));
                    ui.label("Height");
                    ui.add(egui::TextEdit::singleline(&mut self.new_height).desired_width(80.0));
                });
                ui.horizontal_wrapped(|ui| {
                    for preset in PRESETS {
                        if ui.button(preset.to_string()).clicked() {
                            self.new_width = preset.to_string();
                            self.new_height = preset.to_string();
                        }
                    }
                });
                if let Some(error) = &self.dialog_error {
                    ui.colored_label(Color32::from_rgb(220, 100, 90), error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        self.create_document();
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.dialog = None;
            self.dialog_error = None;
        }
    }

    fn dialog_path(&mut self, ctx: &egui::Context) {
        let title = match self.path_kind {
            PathKind::Open => "Open .oxypix",
            PathKind::Save => "Save .oxypix",
            PathKind::ImportDocument => "Import image as a new document",
            PathKind::ImportLayer => "Import image as a layer",
            PathKind::ExportPng => "Export PNG",
            PathKind::ExportBmp => "Export BMP",
            PathKind::ExportGif => "Export GIF",
            PathKind::ExportSheet => "Export spritesheet",
        };
        let save = matches!(
            self.path_kind,
            PathKind::Save
                | PathKind::ExportPng
                | PathKind::ExportBmp
                | PathKind::ExportGif
                | PathKind::ExportSheet
        );
        let mut close = false;
        let mut confirm = false;
        let mut browsed = None;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.label("Path");
                ui.add(egui::TextEdit::singleline(&mut self.path_buf).desired_width(420.0));
                ui.horizontal(|ui| {
                    if ui.button("Browse…").clicked() {
                        browsed = native::pick_path(save, title, filter_name(self.path_kind), filter_ext(self.path_kind));
                    }
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
                if !cfg!(windows) {
                    ui.label("On this build, type a full path. The Windows installer uses the system file dialog.");
                }
                if let Some(error) = &self.dialog_error {
                    ui.colored_label(Color32::from_rgb(220, 100, 90), error);
                }
            });
        if let Some(path) = browsed {
            self.path_buf = path.display().to_string();
        }
        if confirm {
            self.confirm_path();
        } else if close {
            self.dialog = None;
            self.dialog_error = None;
            self.pending = None;
        }
    }

    fn dialog_unsaved(&mut self, ctx: &egui::Context) {
        let mut action = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.label("This document has unsaved changes.");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        action = Some(0);
                    }
                    if ui.button("Discard").clicked() {
                        action = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some(2);
                    }
                });
            });
        match action {
            Some(0) => self.request_save(),
            Some(1) => {
                if let Some(pending) = self.pending {
                    self.finish(pending);
                }
            }
            Some(2) => {
                self.dialog = None;
                self.pending = None;
            }
            _ => {}
        }
    }

    fn dialog_about(&mut self, ctx: &egui::Context) {
        let mut close = false;
        egui::Window::new("About OxyPixel")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.label(format!("OxyPixel {}", env!("CARGO_PKG_VERSION")));
                ui.label("An original pixel-art editor.");
                ui.label("MIT License. Documents stay on this computer.");
                ui.label("OxyPixel does not send files or usage data anywhere.");
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        if close {
            self.dialog = None;
        }
    }

    fn dialog_shortcuts(&mut self, ctx: &egui::Context) {
        let mut close = false;
        egui::Window::new("Shortcuts")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for binding in shortcuts::BINDINGS {
                        ui.label(format!("{}: {}", binding.keys, binding.action));
                    }
                });
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        if close {
            self.dialog = None;
        }
    }
}

fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    menu_item_enabled(ui, label, shortcut, true)
}

fn menu_item_enabled(ui: &mut egui::Ui, label: &str, shortcut: &str, enabled: bool) -> bool {
    let mut button = egui::Button::new(label);
    if !shortcut.is_empty() {
        button = button.shortcut_text(shortcut);
    }
    ui.add_enabled(enabled, button).clicked()
}

fn tool_hint(tool: Tool) -> &'static str {
    match tool {
        Tool::Pencil => "Left-drag draws. Right-drag uses the second color.",
        Tool::Eraser => "Drag to erase pixels on this layer.",
        Tool::Bucket => "Click a connected area on this layer to fill it.",
        Tool::Eyedropper => {
            "Click the canvas to copy that color. Alt+click picks without switching tools."
        }
        Tool::Line => "Drag a straight line. Hold Shift for 45 degrees.",
        Tool::Rectangle => "Drag an outline. Hold Shift for a square.",
        Tool::Ellipse => "Drag an outline. Hold Shift for a circle.",
        Tool::Select => "Drag a rectangle. Later strokes stay inside it.",
    }
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Pencil => "Pencil",
        Tool::Eraser => "Eraser",
        Tool::Bucket => "Fill",
        Tool::Eyedropper => "Pick",
        Tool::Line => "Line",
        Tool::Rectangle => "Rectangle",
        Tool::Ellipse => "Ellipse",
        Tool::Select => "Select",
    }
}

fn filter_name(kind: PathKind) -> &'static str {
    match kind {
        PathKind::Open | PathKind::Save => "OxyPixel document",
        PathKind::ImportDocument | PathKind::ImportLayer => "Image",
        PathKind::ExportPng | PathKind::ExportSheet => "PNG",
        PathKind::ExportBmp => "BMP",
        PathKind::ExportGif => "GIF",
    }
}

fn filter_ext(kind: PathKind) -> &'static [&'static str] {
    match kind {
        PathKind::Open | PathKind::Save => &["oxypix"],
        PathKind::ImportDocument | PathKind::ImportLayer => &["png", "bmp", "gif"],
        PathKind::ExportPng | PathKind::ExportSheet => &["png"],
        PathKind::ExportBmp => &["bmp"],
        PathKind::ExportGif => &["gif"],
    }
}

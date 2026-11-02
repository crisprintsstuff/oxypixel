//! Which keys run which commands, and the presets they start from.
//!
//! Most commands can be bound to other keys in the Keyboard shortcuts window.
//! A few stay where they are: copy and paste follow the events egui emits
//! (see [`crate::shortcuts`]), and Esc, Delete, and the arrow keys are kept
//! for cancelling, clearing, and nudging. The choice is saved to `keys.txt`
//! in the data folder as a preset plus the actions changed from it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{Key, Modifiers};
use oxypixel_core::{Tool, Transform};

use crate::shortcuts::Command;

/// One key with the modifiers held for it. `command` is Ctrl, or Cmd on a Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Chord {
    pub const fn key(key: Key) -> Self {
        Self {
            command: false,
            shift: false,
            alt: false,
            key,
        }
    }

    pub const fn ctrl(key: Key) -> Self {
        Self {
            command: true,
            ..Self::key(key)
        }
    }

    pub const fn shift(key: Key) -> Self {
        Self {
            shift: true,
            ..Self::key(key)
        }
    }

    pub const fn ctrl_shift(key: Key) -> Self {
        Self {
            command: true,
            shift: true,
            ..Self::key(key)
        }
    }

    pub const fn alt(key: Key) -> Self {
        Self {
            alt: true,
            ..Self::key(key)
        }
    }

    pub fn of(modifiers: Modifiers, key: Key) -> Self {
        Self {
            command: modifiers.command || modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            key,
        }
    }

    /// How the chord is shown, such as `Ctrl+Shift+S` (`Cmd+Shift+S` on a Mac).
    pub fn label(&self) -> String {
        let mut out = String::new();
        if self.command {
            out.push_str(if cfg!(target_os = "macos") {
                "Cmd+"
            } else {
                "Ctrl+"
            });
        }
        if self.alt {
            out.push_str(if cfg!(target_os = "macos") {
                "Option+"
            } else {
                "Alt+"
            });
        }
        if self.shift {
            out.push_str("Shift+");
        }
        out.push_str(match self.key {
            // A bare "+" or "-" after "Ctrl+" is hard to read.
            Key::Plus | Key::Minus | Key::Equals => self.key.name(),
            Key::Escape => "Esc",
            key => key.symbol_or_name(),
        });
        out
    }

    /// The chord as `keys.txt` stores it: modifier names and the key's name.
    fn to_text(self) -> String {
        let mut out = String::new();
        if self.command {
            out.push_str("Ctrl+");
        }
        if self.alt {
            out.push_str("Alt+");
        }
        if self.shift {
            out.push_str("Shift+");
        }
        out.push_str(self.key.name());
        out
    }

    fn parse(text: &str) -> Option<Self> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = Key::from_name(parts.pop()?)?;
        let mut chord = Self::key(key);
        for part in parts {
            match part {
                "Ctrl" | "Cmd" => chord.command = true,
                "Shift" => chord.shift = true,
                "Alt" | "Option" => chord.alt = true,
                _ => return None,
            }
        }
        Some(chord)
    }
}

/// A command that can be bound to keys.
pub struct Action {
    /// The name in `keys.txt`. Never change one: saved files use it.
    pub id: &'static str,
    pub label: &'static str,
    pub command: Command,
    /// OxyPixel's own keys for it.
    pub keys: &'static [Chord],
}

const fn action(
    id: &'static str,
    label: &'static str,
    command: Command,
    keys: &'static [Chord],
) -> Action {
    Action {
        id,
        label,
        command,
        keys,
    }
}

pub const ACTIONS: &[Action] = &[
    action(
        "tool.pencil",
        "Pencil",
        Command::Tool(Tool::Pencil),
        &[Chord::key(Key::B)],
    ),
    action(
        "tool.eraser",
        "Eraser",
        Command::Tool(Tool::Eraser),
        &[Chord::key(Key::E)],
    ),
    action(
        "tool.fill",
        "Fill",
        Command::Tool(Tool::Bucket),
        &[Chord::key(Key::G)],
    ),
    action(
        "tool.eyedropper",
        "Eyedropper",
        Command::Tool(Tool::Eyedropper),
        &[Chord::key(Key::I)],
    ),
    action(
        "tool.line",
        "Line",
        Command::Tool(Tool::Line),
        &[Chord::key(Key::L)],
    ),
    action(
        "tool.rectangle",
        "Rectangle",
        Command::Tool(Tool::Rectangle),
        &[Chord::key(Key::R)],
    ),
    action(
        "tool.ellipse",
        "Ellipse",
        Command::Tool(Tool::Ellipse),
        &[Chord::key(Key::O)],
    ),
    action(
        "tool.select",
        "Rectangle select",
        Command::Tool(Tool::Select),
        &[Chord::key(Key::M)],
    ),
    action(
        "tool.wand",
        "Magic wand",
        Command::Tool(Tool::Wand),
        &[Chord::key(Key::W)],
    ),
    action(
        "swap_colors",
        "Swap primary and second colors",
        Command::SwapColors,
        &[Chord::key(Key::X)],
    ),
    action(
        "brush.smaller",
        "Smaller brush",
        Command::BrushSmaller,
        &[Chord::key(Key::OpenBracket)],
    ),
    action(
        "brush.larger",
        "Larger brush",
        Command::BrushLarger,
        &[Chord::key(Key::CloseBracket)],
    ),
    action(
        "pixel_perfect",
        "Pixel-perfect pencil",
        Command::TogglePixelPerfect,
        &[Chord::key(Key::P)],
    ),
    action(
        "shade",
        "Shade along the palette",
        Command::ToggleShade,
        &[Chord::key(Key::D)],
    ),
    action(
        "wrap",
        "Wrap past the edges",
        Command::ToggleWrap,
        &[Chord::key(Key::T)],
    ),
    action(
        "tiled",
        "Tiled view",
        Command::ToggleTiled,
        &[Chord::shift(Key::T)],
    ),
    action(
        "snap",
        "Snap to the palette",
        Command::ToggleSnap,
        &[Chord::key(Key::S)],
    ),
    action("new", "New document", Command::New, &[Chord::ctrl(Key::N)]),
    action("open", "Open", Command::Open, &[Chord::ctrl(Key::O)]),
    action("save", "Save", Command::Save, &[Chord::ctrl(Key::S)]),
    action(
        "save_as",
        "Save as",
        Command::SaveAs,
        &[Chord::ctrl_shift(Key::S)],
    ),
    action(
        "settings",
        "Settings",
        Command::Settings,
        &[Chord::ctrl(Key::Comma)],
    ),
    action("quit", "Quit", Command::Quit, &[Chord::ctrl(Key::Q)]),
    action("undo", "Undo", Command::Undo, &[Chord::ctrl(Key::Z)]),
    action(
        "redo",
        "Redo",
        Command::Redo,
        &[Chord::ctrl(Key::Y), Chord::ctrl_shift(Key::Z)],
    ),
    action(
        "select_all",
        "Select all",
        Command::SelectAll,
        &[Chord::ctrl(Key::A)],
    ),
    action(
        "deselect",
        "Deselect",
        Command::Deselect,
        &[Chord::ctrl(Key::D)],
    ),
    action(
        "zoom.in",
        "Zoom in",
        Command::ZoomIn,
        &[Chord::ctrl(Key::Plus), Chord::ctrl(Key::Equals)],
    ),
    action(
        "zoom.out",
        "Zoom out",
        Command::ZoomOut,
        &[Chord::ctrl(Key::Minus)],
    ),
    action(
        "zoom.fit",
        "Fit the canvas",
        Command::ZoomFit,
        &[Chord::ctrl(Key::Num0)],
    ),
    action(
        "play",
        "Play or pause",
        Command::PlayToggle,
        &[Chord::key(Key::Enter)],
    ),
    action(
        "frame.previous",
        "Previous frame",
        Command::PrevFrame,
        &[Chord::key(Key::Comma)],
    ),
    action(
        "frame.next",
        "Next frame",
        Command::NextFrame,
        &[Chord::key(Key::Period)],
    ),
    action(
        "frame.add",
        "New frame",
        Command::AddFrame,
        &[Chord::ctrl_shift(Key::N)],
    ),
    action(
        "layer.add",
        "New layer",
        Command::AddLayer,
        &[Chord::ctrl_shift(Key::L)],
    ),
    action(
        "flip.horizontal",
        "Flip horizontal",
        Command::Transform(Transform::FlipHorizontal),
        &[Chord::shift(Key::H)],
    ),
    action(
        "flip.vertical",
        "Flip vertical",
        Command::Transform(Transform::FlipVertical),
        &[Chord::shift(Key::V)],
    ),
    action(
        "rotate.cw",
        "Rotate 90 clockwise",
        Command::Transform(Transform::Rotate90Cw),
        &[Chord::shift(Key::Period)],
    ),
    action(
        "rotate.ccw",
        "Rotate 90 counter-clockwise",
        Command::Transform(Transform::Rotate90Ccw),
        &[Chord::shift(Key::Comma)],
    ),
    action(
        "rotate.180",
        "Rotate 180",
        Command::Transform(Transform::Rotate180),
        &[Chord::ctrl_shift(Key::Period)],
    ),
];

/// Keys that cannot be bound to another action, and what they do.
pub const FIXED: &[(Chord, Command, &str)] = &[
    (Chord::ctrl(Key::C), Command::Copy, "Copy"),
    (Chord::ctrl(Key::V), Command::Paste, "Paste"),
    (
        Chord::key(Key::Escape),
        Command::Escape,
        "Stop playback, cancel a shape, or deselect",
    ),
    (
        Chord::key(Key::Delete),
        Command::DeleteSelection,
        "Clear the selection",
    ),
    (
        Chord::key(Key::Backspace),
        Command::DeleteSelection,
        "Clear the selection",
    ),
    (
        Chord::key(Key::ArrowLeft),
        Command::Nudge(-1, 0),
        "Nudge left",
    ),
    (
        Chord::key(Key::ArrowRight),
        Command::Nudge(1, 0),
        "Nudge right",
    ),
    (Chord::key(Key::ArrowUp), Command::Nudge(0, -1), "Nudge up"),
    (
        Chord::key(Key::ArrowDown),
        Command::Nudge(0, 1),
        "Nudge down",
    ),
];

/// Where a key map starts. Each changes only the keys that differ from
/// OxyPixel's for commands OxyPixel has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    OxyPixel,
    Aseprite,
    Photoshop,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::OxyPixel, Preset::Aseprite, Preset::Photoshop];

    pub fn label(self) -> &'static str {
        match self {
            Preset::OxyPixel => "OxyPixel",
            Preset::Aseprite => "Aseprite",
            Preset::Photoshop => "Photoshop",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Preset::OxyPixel => "oxypixel",
            Preset::Aseprite => "aseprite",
            Preset::Photoshop => "photoshop",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The keys that differ from OxyPixel's, by action id.
    fn changes(self) -> &'static [(&'static str, &'static [Chord])] {
        match self {
            Preset::OxyPixel => &[],
            Preset::Aseprite => ASEPRITE,
            Preset::Photoshop => PHOTOSHOP,
        }
    }

    /// The keys this preset gives `action`.
    pub fn keys(self, action: usize) -> Vec<Chord> {
        let action = &ACTIONS[action];
        self.changes()
            .iter()
            .find(|(id, _)| *id == action.id)
            .map_or(action.keys, |(_, keys)| *keys)
            .to_vec()
    }
}

/// Aseprite's default keys where they differ from OxyPixel's.
const ASEPRITE: &[(&str, &[Chord])] = &[
    ("tool.rectangle", &[Chord::key(Key::U)]),
    ("tool.ellipse", &[Chord::shift(Key::U)]),
    ("frame.add", &[Chord::alt(Key::N)]),
    ("layer.add", &[Chord::shift(Key::N)]),
    ("settings", &[Chord::ctrl(Key::K)]),
    (
        "zoom.in",
        &[
            Chord::key(Key::Plus),
            Chord::key(Key::Equals),
            Chord::ctrl(Key::Plus),
        ],
    ),
    (
        "zoom.out",
        &[Chord::key(Key::Minus), Chord::ctrl(Key::Minus)],
    ),
];

/// Photoshop's default keys where they differ from OxyPixel's. Its
/// Ctrl+Shift+N makes a layer, so a new frame has no key.
const PHOTOSHOP: &[(&str, &[Chord])] = &[
    ("tool.rectangle", &[Chord::key(Key::U)]),
    ("tool.ellipse", &[Chord::shift(Key::U)]),
    ("redo", &[Chord::ctrl_shift(Key::Z)]),
    ("layer.add", &[Chord::ctrl_shift(Key::N)]),
    ("frame.add", &[]),
    ("settings", &[Chord::ctrl(Key::K)]),
];

/// The keys in use: a preset, with some actions bound to other keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    preset: Preset,
    /// The keys of each action in [`ACTIONS`], in that order.
    keys: Vec<Vec<Chord>>,
    table: HashMap<Chord, Command>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::from_preset(Preset::OxyPixel)
    }
}

/// What [`Keymap::bind`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bound {
    /// The key now runs the action. It was taken from these actions.
    Done { taken_from: Vec<&'static str> },
    /// The key cannot be bound; it is kept for this.
    Fixed(&'static str),
}

impl Keymap {
    pub fn from_preset(preset: Preset) -> Self {
        let keys = (0..ACTIONS.len()).map(|i| preset.keys(i)).collect();
        let mut map = Self {
            preset,
            keys,
            table: HashMap::new(),
        };
        map.rebuild();
        map
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    /// The command a key runs, if any.
    pub fn command(&self, modifiers: Modifiers, key: Key) -> Option<Command> {
        self.table.get(&Chord::of(modifiers, key)).copied()
    }

    pub fn keys(&self, action: usize) -> &[Chord] {
        &self.keys[action]
    }

    /// The first key for a command, as shown in menus and tooltips, or "".
    pub fn hint(&self, command: Command) -> String {
        ACTIONS
            .iter()
            .position(|a| a.command == command)
            .and_then(|i| self.keys[i].first())
            .or_else(|| {
                FIXED
                    .iter()
                    .find(|(_, c, _)| *c == command)
                    .map(|(chord, _, _)| chord)
            })
            .map(Chord::label)
            .unwrap_or_default()
    }

    /// Every key for an action, joined for display, or "None".
    pub fn keys_label(&self, action: usize) -> String {
        let keys = &self.keys[action];
        if keys.is_empty() {
            "None".to_string()
        } else {
            keys.iter()
                .map(Chord::label)
                .collect::<Vec<_>>()
                .join(" or ")
        }
    }

    /// Make `chord` the only key for `action`, taking it from any other action.
    pub fn bind(&mut self, action: usize, chord: Chord) -> Bound {
        if let Some((_, _, what)) = FIXED.iter().find(|(c, _, _)| *c == chord) {
            return Bound::Fixed(what);
        }
        let mut taken_from = Vec::new();
        for (i, keys) in self.keys.iter_mut().enumerate() {
            if i != action && keys.contains(&chord) {
                keys.retain(|k| *k != chord);
                taken_from.push(ACTIONS[i].label);
            }
        }
        self.keys[action] = vec![chord];
        self.rebuild();
        Bound::Done { taken_from }
    }

    pub fn clear(&mut self, action: usize) {
        self.keys[action].clear();
        self.rebuild();
    }

    /// Put one action back to the preset's keys, taking them from others.
    pub fn reset(&mut self, action: usize) {
        let keys = self.preset.keys(action);
        for (i, other) in self.keys.iter_mut().enumerate() {
            if i != action {
                other.retain(|k| !keys.contains(k));
            }
        }
        self.keys[action] = keys;
        self.rebuild();
    }

    /// Whether any action differs from the preset.
    pub fn is_changed(&self) -> bool {
        (0..ACTIONS.len()).any(|i| self.keys[i] != self.preset.keys(i))
    }

    fn rebuild(&mut self) {
        self.table.clear();
        for (chord, command, _) in FIXED {
            self.table.insert(*chord, *command);
        }
        for (i, keys) in self.keys.iter().enumerate() {
            for chord in keys {
                self.table.entry(*chord).or_insert(ACTIONS[i].command);
            }
        }
    }

    /// `keys.txt`: the preset, then one line per action changed from it.
    pub fn to_text(&self) -> String {
        let mut out = format!("preset={}\n", self.preset.id());
        for (i, action) in ACTIONS.iter().enumerate() {
            if self.keys[i] != self.preset.keys(i) {
                let keys: Vec<String> = self.keys[i].iter().map(|c| c.to_text()).collect();
                out.push_str(&format!("{}={}\n", action.id, keys.join(", ")));
            }
        }
        out
    }

    /// Read `keys.txt`. Lines that do not parse are skipped.
    pub fn from_text(text: &str) -> Self {
        let mut lines = text.lines().filter_map(|l| l.split_once('='));
        let preset = text
            .lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == "preset")
            .and_then(|(_, v)| Preset::from_id(v.trim()))
            .unwrap_or_default();
        let mut map = Self::from_preset(preset);
        for (id, value) in lines.by_ref() {
            let Some(i) = ACTIONS.iter().position(|a| a.id == id.trim()) else {
                continue;
            };
            let keys: Vec<Chord> = value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .filter_map(Chord::parse)
                .filter(|c| !FIXED.iter().any(|(f, _, _)| f == c))
                .collect();
            for (j, other) in map.keys.iter_mut().enumerate() {
                if j != i {
                    other.retain(|k| !keys.contains(k));
                }
            }
            map.keys[i] = keys;
        }
        map.rebuild();
        map
    }

    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| Self::from_text(&text))
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = path() else {
            return Ok(());
        };
        write_file(&path, &self.to_text())
    }
}

fn path() -> Option<PathBuf> {
    crate::recent::data_dir().map(|dir| dir.join("keys.txt"))
}

fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(id: &str) -> usize {
        ACTIONS.iter().position(|a| a.id == id).unwrap()
    }

    #[test]
    fn action_ids_are_unique_and_defaults_do_not_collide() {
        for (i, a) in ACTIONS.iter().enumerate() {
            assert!(ACTIONS[i + 1..].iter().all(|b| b.id != a.id), "{}", a.id);
        }
        for preset in Preset::ALL {
            let map = Keymap::from_preset(preset);
            let mut seen = HashMap::new();
            for (i, keys) in map.keys.iter().enumerate() {
                for chord in keys {
                    assert!(
                        !FIXED.iter().any(|(f, _, _)| f == chord),
                        "{preset:?} binds a fixed key"
                    );
                    if let Some(other) = seen.insert(*chord, i) {
                        panic!(
                            "{preset:?}: {} is on {} and {}",
                            chord.label(),
                            ACTIONS[other].id,
                            ACTIONS[i].id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn presets_change_only_their_keys() {
        let ase = Keymap::from_preset(Preset::Aseprite);
        let n = |key| (Modifiers::NONE, key);
        let (m, k) = n(Key::U);
        assert_eq!(ase.command(m, k), Some(Command::Tool(Tool::Rectangle)));
        assert_eq!(ase.command(Modifiers::ALT, Key::N), Some(Command::AddFrame));
        assert_eq!(
            ase.command(Modifiers::SHIFT, Key::N),
            Some(Command::AddLayer)
        );
        assert_eq!(
            ase.command(Modifiers::NONE, Key::B),
            Some(Command::Tool(Tool::Pencil))
        );
        assert_eq!(
            ase.command(Modifiers::NONE, Key::R),
            None,
            "R left rectangle"
        );
        let ps = Keymap::from_preset(Preset::Photoshop);
        assert_eq!(ps.command(Modifiers::COMMAND, Key::Y), None);
        assert_eq!(
            ps.command(Modifiers::COMMAND | Modifiers::SHIFT, Key::N),
            Some(Command::AddLayer)
        );
        assert_eq!(
            ps.command(Modifiers::COMMAND, Key::K),
            Some(Command::Settings)
        );
        assert_eq!(ps.hint(Command::AddFrame), "");
        // Copy and the arrows are the same everywhere.
        for map in [&ase, &ps] {
            assert_eq!(map.command(Modifiers::COMMAND, Key::C), Some(Command::Copy));
            assert_eq!(
                map.command(Modifiers::NONE, Key::ArrowUp),
                Some(Command::Nudge(0, -1))
            );
        }
    }

    #[test]
    fn binding_takes_the_key_from_another_action() {
        let mut map = Keymap::default();
        let pencil = index("tool.pencil");
        assert_eq!(
            map.bind(pencil, Chord::key(Key::E)),
            Bound::Done {
                taken_from: vec!["Eraser"]
            }
        );
        assert_eq!(
            map.command(Modifiers::NONE, Key::E),
            Some(Command::Tool(Tool::Pencil))
        );
        assert_eq!(map.command(Modifiers::NONE, Key::B), None);
        assert_eq!(map.keys_label(index("tool.eraser")), "None");
        assert_eq!(map.bind(pencil, Chord::ctrl(Key::C)), Bound::Fixed("Copy"));
        assert!(map.is_changed());
        map.reset(pencil);
        assert_eq!(
            map.command(Modifiers::NONE, Key::B),
            Some(Command::Tool(Tool::Pencil))
        );
        map.reset(index("tool.eraser"));
        assert!(!map.is_changed());
        map.clear(index("undo"));
        assert_eq!(map.command(Modifiers::COMMAND, Key::Z), None);
        assert_eq!(map.hint(Command::Undo), "");
    }

    #[test]
    fn keys_txt_round_trips_and_skips_bad_lines() {
        let mut map = Keymap::from_preset(Preset::Aseprite);
        map.bind(index("save_as"), Chord::ctrl_shift(Key::Comma));
        map.clear(index("snap"));
        let text = map.to_text();
        assert!(text.starts_with("preset=aseprite\n"), "{text}");
        assert!(text.contains("save_as=Ctrl+Shift+Comma\n"), "{text}");
        assert!(text.contains("snap=\n"), "{text}");
        assert_eq!(text.lines().count(), 3, "only changes are written: {text}");
        assert_eq!(Keymap::from_text(&text), map);
        let messy =
            "preset=nothing\nbogus=Ctrl+B\nundo=Ctrl+Bogus, Ctrl+U\nredo=Ctrl+C\n\nnot a line";
        let read = Keymap::from_text(messy);
        assert_eq!(read.preset(), Preset::OxyPixel);
        assert_eq!(
            read.command(Modifiers::COMMAND, Key::U),
            Some(Command::Undo)
        );
        assert_eq!(
            read.keys(index("redo")),
            &[] as &[Chord],
            "a fixed key is not taken"
        );
        assert_eq!(
            read.command(Modifiers::COMMAND, Key::C),
            Some(Command::Copy)
        );
    }

    #[test]
    fn chords_read_and_print() {
        let c = Chord::parse("Ctrl+Shift+Period").unwrap();
        assert_eq!(c, Chord::ctrl_shift(Key::Period));
        assert_eq!(Chord::parse("Alt+N"), Some(Chord::alt(Key::N)));
        assert_eq!(Chord::parse("Hyper+N"), None);
        assert_eq!(Chord::parse(""), None);
        if !cfg!(target_os = "macos") {
            assert_eq!(c.label(), "Ctrl+Shift+.");
            assert_eq!(Chord::ctrl(Key::Plus).label(), "Ctrl+Plus");
        }
        assert_eq!(Chord::shift(Key::T).label(), "Shift+T");
    }
}

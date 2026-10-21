//! Keyboard shortcut map. Plain letter shortcuts are ignored while a text field is focused.
//! The UI checks that before it runs the commands from [`commands_from_events`].
//!
//! egui reports Ctrl+C as [`egui::Event::Copy`] and keeps that key-down. An image-only
//! Ctrl+V still produces a key release. Paste runs on that release so the chord paints once.

use eframe::egui::{self, Key, Modifiers};
use oxypixel_core::Tool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    New,
    Open,
    Save,
    SaveAs,
    Quit,
    Undo,
    Redo,
    SelectAll,
    Deselect,
    Escape,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    BrushSmaller,
    BrushLarger,
    SwapColors,
    Tool(Tool),
    PlayToggle,
    PrevFrame,
    NextFrame,
    AddFrame,
    AddLayer,
    DeleteSelection,
    Copy,
    Paste,
    Nudge(i32, i32),
}

pub struct Binding {
    pub keys: &'static str,
    pub action: &'static str,
}

pub const BINDINGS: &[Binding] = &[
    Binding {
        keys: "B E G I L R O M",
        action: "Pencil, eraser, fill, eyedropper, line, rectangle, ellipse, select",
    },
    Binding {
        keys: "X",
        action: "Swap primary and secondary colors",
    },
    Binding {
        keys: "[ ]",
        action: "Smaller or larger brush",
    },
    Binding {
        keys: "Ctrl+Z",
        action: "Undo",
    },
    Binding {
        keys: "Ctrl+Y or Ctrl+Shift+Z",
        action: "Redo",
    },
    Binding {
        keys: "Ctrl+N",
        action: "New document",
    },
    Binding {
        keys: "Ctrl+O",
        action: "Open",
    },
    Binding {
        keys: "Ctrl+S",
        action: "Save",
    },
    Binding {
        keys: "Ctrl+Shift+S",
        action: "Save as",
    },
    Binding {
        keys: "Ctrl+Shift+N",
        action: "New frame",
    },
    Binding {
        keys: "Ctrl+Shift+L",
        action: "New layer",
    },
    Binding {
        keys: "Ctrl+A",
        action: "Select all",
    },
    Binding {
        keys: "Ctrl+D",
        action: "Deselect",
    },
    Binding {
        keys: "Esc",
        action: "Stop playback, cancel a shape, or deselect",
    },
    Binding {
        keys: "Delete",
        action: "Clear pixels inside the selection",
    },
    Binding {
        keys: "Ctrl+Plus / Ctrl+Minus / Ctrl+0",
        action: "Zoom in, zoom out, fit",
    },
    Binding {
        keys: "Enter",
        action: "Play or pause",
    },
    Binding {
        keys: "Ctrl+C / Ctrl+V",
        action: "Copy or paste the selection image",
    },
    Binding {
        keys: "Arrow keys",
        action: "Nudge the selection by one pixel",
    },
    Binding {
        keys: "Comma / Period",
        action: "Previous or next frame",
    },
    Binding {
        keys: "Space+drag or middle-drag",
        action: "Pan",
    },
    Binding {
        keys: "Alt+click",
        action: "Pick a color without changing tools",
    },
    Binding {
        keys: "Shift+drag",
        action: "Constrain lines to 45 degrees and shapes to squares",
    },
    Binding {
        keys: "Ctrl+Q",
        action: "Quit",
    },
];

pub fn command_from(modifiers: Modifiers, key: Key) -> Option<Command> {
    let ctrl = modifiers.command || modifiers.ctrl;
    let shift = modifiers.shift;
    if modifiers.alt {
        return None;
    }
    match (ctrl, shift, key) {
        (true, true, Key::S) => Some(Command::SaveAs),
        (true, true, Key::Z) => Some(Command::Redo),
        (true, true, Key::N) => Some(Command::AddFrame),
        (true, true, Key::L) => Some(Command::AddLayer),
        (true, false, Key::S) => Some(Command::Save),
        (true, false, Key::Z) => Some(Command::Undo),
        (true, false, Key::Y) => Some(Command::Redo),
        (true, false, Key::N) => Some(Command::New),
        (true, false, Key::O) => Some(Command::Open),
        (true, false, Key::A) => Some(Command::SelectAll),
        (true, false, Key::D) => Some(Command::Deselect),
        (true, false, Key::Num0) => Some(Command::ZoomFit),
        (true, false, Key::Equals | Key::Plus) => Some(Command::ZoomIn),
        (true, false, Key::Minus) => Some(Command::ZoomOut),
        (true, false, Key::Q) => Some(Command::Quit),
        (true, false, Key::C) => Some(Command::Copy),
        (true, false, Key::V) => Some(Command::Paste),
        (false, false, Key::B) => Some(Command::Tool(Tool::Pencil)),
        (false, false, Key::E) => Some(Command::Tool(Tool::Eraser)),
        (false, false, Key::G) => Some(Command::Tool(Tool::Bucket)),
        (false, false, Key::I) => Some(Command::Tool(Tool::Eyedropper)),
        (false, false, Key::L) => Some(Command::Tool(Tool::Line)),
        (false, false, Key::R) => Some(Command::Tool(Tool::Rectangle)),
        (false, false, Key::O) => Some(Command::Tool(Tool::Ellipse)),
        (false, false, Key::M) => Some(Command::Tool(Tool::Select)),
        (false, false, Key::X) => Some(Command::SwapColors),
        (false, false, Key::OpenBracket) => Some(Command::BrushSmaller),
        (false, false, Key::CloseBracket) => Some(Command::BrushLarger),
        (false, false, Key::Enter) => Some(Command::PlayToggle),
        (false, false, Key::Comma) => Some(Command::PrevFrame),
        (false, false, Key::Period) => Some(Command::NextFrame),
        (false, false, Key::ArrowLeft) => Some(Command::Nudge(-1, 0)),
        (false, false, Key::ArrowRight) => Some(Command::Nudge(1, 0)),
        (false, false, Key::ArrowUp) => Some(Command::Nudge(0, -1)),
        (false, false, Key::ArrowDown) => Some(Command::Nudge(0, 1)),
        (false, false, Key::Delete | Key::Backspace) => Some(Command::DeleteSelection),
        (false, false, Key::Escape) => Some(Command::Escape),
        _ => None,
    }
}

pub fn allows_repeat(command: Command) -> bool {
    matches!(
        command,
        Command::BrushSmaller
            | Command::BrushLarger
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::Nudge(_, _)
    )
}

/// Which clipboard chord is held down. Key repeats must not copy or paste again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipboardChord {
    pub copy: bool,
    pub paste: bool,
}

/// Shortcuts in one frame of egui events.
///
/// Copy is the copy event (or a copy key-down, when one arrives). Paste is the
/// paste event when egui emits one, and otherwise the paste key-up. An image on
/// the clipboard produces that key-up with no paste event.
pub fn commands_from_events(events: &[egui::Event], chord: &mut ClipboardChord) -> Vec<Command> {
    let mut commands = Vec::new();
    for event in events {
        match event {
            egui::Event::Copy => note_copy(&mut commands, chord),
            egui::Event::Paste(_) => note_paste(&mut commands, chord),
            egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } => {
                if *pressed {
                    if let Some(command) = command_from(*modifiers, *key) {
                        if command == Command::Copy {
                            note_copy(&mut commands, chord);
                            continue;
                        }
                        if command == Command::Paste {
                            note_paste(&mut commands, chord);
                            continue;
                        }
                        if *repeat && !allows_repeat(command) {
                            continue;
                        }
                        commands.push(command);
                    }
                } else {
                    if is_copy_key(*key) {
                        chord.copy = false;
                    }
                    if is_paste_key(*key) {
                        if !chord.paste && is_paste_release(*modifiers, *key) {
                            commands.push(Command::Paste);
                        }
                        chord.paste = false;
                    }
                }
            }
            _ => {}
        }
    }
    commands
}

fn note_copy(commands: &mut Vec<Command>, chord: &mut ClipboardChord) {
    if !chord.copy {
        commands.push(Command::Copy);
    }
    chord.copy = true;
}

fn note_paste(commands: &mut Vec<Command>, chord: &mut ClipboardChord) {
    if !chord.paste {
        commands.push(Command::Paste);
    }
    chord.paste = true;
}

fn is_copy_key(key: Key) -> bool {
    matches!(key, Key::C | Key::Insert | Key::Copy)
}

fn is_paste_key(key: Key) -> bool {
    matches!(key, Key::V | Key::Insert | Key::Paste)
}

/// Paste chords that egui may consume on the way down. The matching key-up still arrives.
fn is_paste_release(modifiers: Modifiers, key: Key) -> bool {
    if modifiers.alt {
        return false;
    }
    let ctrl = modifiers.command || modifiers.ctrl;
    if key == Key::Paste {
        return true;
    }
    if ctrl && !modifiers.shift && key == Key::V {
        return true;
    }
    !ctrl && modifiers.shift && key == Key::Insert
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_as_is_not_save() {
        let mods = Modifiers::COMMAND | Modifiers::SHIFT;
        assert_eq!(command_from(mods, Key::S), Some(Command::SaveAs));
        assert_eq!(
            command_from(Modifiers::COMMAND, Key::S),
            Some(Command::Save)
        );
        assert_eq!(
            command_from(Modifiers::COMMAND, Key::Z),
            Some(Command::Undo)
        );
        assert_eq!(command_from(mods, Key::Z), Some(Command::Redo));
        assert_eq!(
            command_from(Modifiers::NONE, Key::ArrowLeft),
            Some(Command::Nudge(-1, 0))
        );
        assert_eq!(
            command_from(Modifiers::NONE, Key::Comma),
            Some(Command::PrevFrame)
        );
        assert_eq!(
            command_from(Modifiers::COMMAND, Key::C),
            Some(Command::Copy)
        );
        assert_eq!(
            command_from(Modifiers::COMMAND, Key::V),
            Some(Command::Paste)
        );
    }

    #[test]
    fn alt_does_not_change_tools() {
        let mut mods = Modifiers::NONE;
        mods.alt = true;
        assert_eq!(command_from(mods, Key::B), None);
        assert_eq!(
            command_from(Modifiers::NONE, Key::B),
            Some(Command::Tool(Tool::Pencil))
        );
    }

    #[test]
    fn copy_and_paste_follow_the_events_egui_actually_emits() {
        let ctrl = Modifiers::COMMAND;
        let mut chord = ClipboardChord::default();
        assert_eq!(
            commands_from_events(&[egui::Event::Copy], &mut chord),
            vec![Command::Copy]
        );
        assert!(chord.copy);
        assert_eq!(
            commands_from_events(&[egui::Event::Copy], &mut chord),
            Vec::<Command>::new()
        );
        assert_eq!(
            commands_from_events(&[key_event(Key::C, false, ctrl)], &mut chord),
            Vec::<Command>::new()
        );
        assert!(!chord.copy);

        let image_paste = commands_from_events(&[key_event(Key::V, false, ctrl)], &mut chord);
        assert_eq!(image_paste, vec![Command::Paste]);
        assert!(!chord.paste);

        let text_and_release = commands_from_events(
            &[
                egui::Event::Paste("plain text".to_string()),
                key_event(Key::V, false, ctrl),
            ],
            &mut chord,
        );
        assert_eq!(text_and_release, vec![Command::Paste]);
        assert!(!chord.paste);

        assert_eq!(
            commands_from_events(&[egui::Event::Paste("again".to_string())], &mut chord),
            vec![Command::Paste]
        );
        assert!(chord.paste);
        assert_eq!(
            commands_from_events(&[egui::Event::Paste("repeat".to_string())], &mut chord),
            Vec::<Command>::new()
        );
        assert_eq!(
            commands_from_events(&[key_event(Key::V, false, Modifiers::NONE)], &mut chord),
            Vec::<Command>::new()
        );
        assert!(!chord.paste);

        let mut shift_insert = Modifiers::NONE;
        shift_insert.shift = true;
        assert_eq!(
            commands_from_events(
                &[key_event(Key::Insert, false, shift_insert)],
                &mut ClipboardChord::default()
            ),
            vec![Command::Paste]
        );

        let mut alt = ctrl;
        alt.alt = true;
        assert_eq!(
            commands_from_events(
                &[key_event(Key::V, false, alt)],
                &mut ClipboardChord::default()
            ),
            Vec::<Command>::new()
        );

        assert_eq!(
            commands_from_events(
                &[key_event(Key::A, true, ctrl)],
                &mut ClipboardChord::default()
            ),
            vec![Command::SelectAll]
        );
    }

    fn key_event(key: Key, pressed: bool, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn help_table_mentions_core_actions() {
        let text = BINDINGS
            .iter()
            .map(|b| b.action)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Undo"));
        assert!(text.contains("Pencil"));
    }
}

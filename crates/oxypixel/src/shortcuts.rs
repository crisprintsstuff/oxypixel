//! Keyboard shortcut map. Plain letter shortcuts are ignored while a text field is focused.
//! The UI checks that before calling [`command_from`].

use eframe::egui::{Key, Modifiers};
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
        action: "Copy or paste the selection",
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

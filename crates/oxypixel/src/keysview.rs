//! The Keyboard shortcuts window (Help, then Keyboard shortcuts…, or Edit,
//! then Keyboard shortcuts…).
//!
//! It shows every key, lets each action take another key, and switches the
//! whole map to the keys of another program. Changes apply at once and are
//! saved to `keys.txt`.

use eframe::egui::{self, Color32, RichText};

use crate::keymap::{Bound, Chord, Keymap, Preset, ACTIONS, FIXED};
use crate::shortcuts::GESTURES;

#[derive(Default)]
pub struct KeysWindow {
    pub open: bool,
    /// The action waiting for its new key.
    capturing: Option<usize>,
    message: Option<String>,
}

impl KeysWindow {
    /// While a key is being chosen, the app must not run shortcuts.
    pub fn capturing(&self) -> bool {
        self.open && self.capturing.is_some()
    }

    /// Show the window. Returns true when the key map changed.
    pub fn show(&mut self, ctx: &egui::Context, map: &mut Keymap) -> bool {
        if !self.open {
            self.capturing = None;
            return false;
        }
        let mut changed = false;
        if let Some(action) = self.capturing {
            if let Some(chord) = pressed_chord(ctx) {
                self.capturing = None;
                if chord == Chord::key(egui::Key::Escape) {
                    self.message = None;
                } else {
                    self.message = Some(match map.bind(action, chord) {
                        Bound::Done { taken_from } if taken_from.is_empty() => {
                            changed = true;
                            format!("{} is now {}.", ACTIONS[action].label, chord.label())
                        }
                        Bound::Done { taken_from } => {
                            changed = true;
                            format!(
                                "{} is now {}. It was taken from {}.",
                                ACTIONS[action].label,
                                chord.label(),
                                taken_from.join(" and ")
                            )
                        }
                        Bound::Fixed(what) => {
                            format!("{} is kept for {what}. Pick another key.", chord.label())
                        }
                    });
                }
            }
        }
        let mut open = self.open;
        let mut close = false;
        egui::Window::new("Keyboard shortcuts")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.screen_rect().center())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Keys like");
                    let current = map.preset();
                    egui::ComboBox::from_id_salt("keys-preset")
                        .selected_text(current.label())
                        .show_ui(ui, |ui| {
                            for preset in Preset::ALL {
                                if ui
                                    .selectable_label(preset == current, preset.label())
                                    .clicked()
                                    && (preset != current || map.is_changed())
                                {
                                    *map = Keymap::from_preset(preset);
                                    self.capturing = None;
                                    self.message = Some(format!(
                                        "Using {}'s keys for the commands OxyPixel has.",
                                        preset.label()
                                    ));
                                    changed = true;
                                }
                            }
                        });
                    if ui
                        .add_enabled(map.is_changed(), egui::Button::new("Reset all"))
                        .on_hover_text("Put every action back to the preset's keys.")
                        .clicked()
                    {
                        *map = Keymap::from_preset(map.preset());
                        self.message = None;
                        changed = true;
                    }
                });
                ui.label(
                    RichText::new("Click a key to change it, then press the new key. Esc cancels.")
                        .small()
                        .weak(),
                );
                if let Some(message) = &self.message {
                    ui.colored_label(Color32::from_rgb(240, 180, 60), message);
                }
                let max_height = (ctx.screen_rect().height() - 220.0).max(160.0);
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .show(ui, |ui| {
                        egui::Grid::new("keys-grid")
                            .num_columns(3)
                            .striped(true)
                            .show(ui, |ui| {
                                for (i, action) in ACTIONS.iter().enumerate() {
                                    ui.label(action.label);
                                    let text = if self.capturing == Some(i) {
                                        "Press a key…".to_string()
                                    } else {
                                        map.keys_label(i)
                                    };
                                    if ui
                                        .selectable_label(self.capturing == Some(i), text)
                                        .on_hover_text("Click, then press the new key")
                                        .clicked()
                                    {
                                        self.capturing = Some(i);
                                        self.message = None;
                                    }
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_enabled(
                                                !map.keys(i).is_empty(),
                                                egui::Button::new("Clear").small(),
                                            )
                                            .clicked()
                                        {
                                            map.clear(i);
                                            changed = true;
                                        }
                                        let preset_keys = map.preset().keys(i);
                                        if ui
                                            .add_enabled(
                                                map.keys(i) != preset_keys.as_slice(),
                                                egui::Button::new("Reset").small(),
                                            )
                                            .clicked()
                                        {
                                            map.reset(i);
                                            changed = true;
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                        ui.add_space(8.0);
                        ui.label(RichText::new("Always").strong());
                        egui::Grid::new("keys-fixed").num_columns(2).show(ui, |ui| {
                            let mut shown: Vec<&str> = Vec::new();
                            for (_, _, what) in FIXED {
                                if !shown.contains(what) {
                                    let keys: Vec<String> = FIXED
                                        .iter()
                                        .filter(|(_, _, w)| w == what)
                                        .map(|(c, _, _)| c.label())
                                        .collect();
                                    ui.label(*what);
                                    ui.label(keys.join(" or "));
                                    ui.end_row();
                                    shown.push(what);
                                }
                            }
                            for (keys, what) in GESTURES {
                                ui.label(*what);
                                ui.label(*keys);
                                ui.end_row();
                            }
                        });
                    });
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        self.open = open && !close;
        if !self.open {
            self.capturing = None;
        }
        changed
    }
}

/// The first key pressed this frame, with its modifiers.
fn pressed_chord(ctx: &egui::Context) -> Option<Chord> {
    ctx.input(|input| {
        input.raw.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => Some(Chord::of(*modifiers, *key)),
            _ => None,
        })
    })
}

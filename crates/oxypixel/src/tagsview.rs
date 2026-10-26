//! The Tags window: named frame ranges for animation.
//!
//! Tags are saved in the `.oxypix` file (version 2), written to a
//! spritesheet's JSON as `meta.frameTags`, and read by OxyPack. Each change is
//! one undo step: names and numbers are applied when the field is left or a
//! drag ends, not on every keystroke.

use eframe::egui::{self, Color32};
use oxypixel_core::{Editor, FrameTag, LoopDirection};

/// What a row's fields show while they are being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Draft {
    /// The tag this draft started from. A different tag in the document
    /// (after an undo, say) resets the draft.
    source: FrameTag,
    name: String,
    /// 1-based, as shown.
    from: usize,
    to: usize,
    repeat: u16,
}

impl Draft {
    fn of(tag: &FrameTag) -> Self {
        Self {
            source: tag.clone(),
            name: tag.name.clone(),
            from: tag.from + 1,
            to: tag.to + 1,
            repeat: tag.repeat,
        }
    }

    /// The tag these fields describe.
    fn tag(&self) -> FrameTag {
        FrameTag {
            name: self.name.clone(),
            from: self.from.saturating_sub(1),
            to: self.to.saturating_sub(1),
            direction: self.source.direction,
            repeat: self.repeat,
        }
    }
}

#[derive(Default)]
pub struct TagsWindow {
    pub open: bool,
    drafts: Vec<Draft>,
    /// The row whose field has focus or is being dragged; its draft is kept.
    editing: Option<usize>,
    error: Option<String>,
}

/// A name no tag has yet: "Tag 1", "Tag 2", and so on.
pub(crate) fn new_tag_name(tags: &[FrameTag]) -> String {
    (1..)
        .map(|n| format!("Tag {n}"))
        .find(|name| tags.iter().all(|tag| &tag.name != name))
        .unwrap_or_else(|| "Tag".into())
}

/// The names of the tags that cover `frame`, joined for a tooltip.
pub(crate) fn names_at(tags: &[FrameTag], frame: usize) -> Option<String> {
    let names: Vec<&str> = tags
        .iter()
        .filter(|tag| tag.contains(frame))
        .map(|tag| tag.name.as_str())
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

impl TagsWindow {
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Tags")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| self.contents(ui, editor));
        self.open = open;
    }

    fn sync(&mut self, tags: &[FrameTag]) {
        self.drafts.truncate(tags.len());
        for (index, tag) in tags.iter().enumerate() {
            match self.drafts.get_mut(index) {
                Some(draft) if self.editing == Some(index) && draft.source == *tag => {}
                Some(draft) => *draft = Draft::of(tag),
                None => self.drafts.push(Draft::of(tag)),
            }
        }
    }

    fn commit(&mut self, editor: &mut Editor, index: usize, tag: FrameTag) {
        match editor.set_tag(index, tag) {
            Ok(()) => self.error = None,
            Err(err) => self.error = Some(err.to_string()),
        }
        if let Some(fresh) = editor.document().tags().get(index) {
            self.drafts[index] = Draft::of(fresh);
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let frame_count = editor.document().frame_count();
        let tags = editor.document().tags().to_vec();
        self.sync(&tags);
        ui.label(
            "Named frame ranges for animation. Exported in a spritesheet's JSON and by OxyPack.",
        );
        let mut editing = None;
        let mut remove = None;
        let mut go_to = None;
        if tags.is_empty() {
            ui.weak("No tags yet.");
        } else {
            egui::Grid::new("tags-grid").num_columns(7).show(ui, |ui| {
                for header in ["Name", "From", "To", "Direction", "Repeat", "", ""] {
                    ui.strong(header);
                }
                ui.end_row();
                for index in 0..self.drafts.len() {
                    let mut commit = false;
                    let draft = &mut self.drafts[index];
                    let name =
                        ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(110.0));
                    let from = ui.add(egui::DragValue::new(&mut draft.from).range(1..=frame_count));
                    let to =
                        ui.add(egui::DragValue::new(&mut draft.to).range(draft.from..=frame_count));
                    let mut direction = draft.source.direction;
                    egui::ComboBox::from_id_salt(("tag-direction", index))
                        .selected_text(direction.label())
                        .show_ui(ui, |ui| {
                            for choice in LoopDirection::ALL {
                                ui.selectable_value(&mut direction, choice, choice.label());
                            }
                        });
                    let repeat = ui
                        .add(
                            egui::DragValue::new(&mut draft.repeat)
                                .range(0..=u16::MAX)
                                .custom_formatter(|n, _| {
                                    if n == 0.0 {
                                        "forever".into()
                                    } else {
                                        format!("{n}×")
                                    }
                                }),
                        )
                        .on_hover_text("How many times the range plays. 0 is forever.");
                    if ui
                        .small_button("Go")
                        .on_hover_text("Show the first frame")
                        .clicked()
                    {
                        go_to = Some(draft.from.saturating_sub(1));
                    }
                    if ui
                        .small_button("−")
                        .on_hover_text("Delete this tag")
                        .clicked()
                    {
                        remove = Some(index);
                    }
                    ui.end_row();
                    for field in [&name, &from, &to, &repeat] {
                        if field.has_focus() || field.dragged() {
                            editing = Some(index);
                        }
                        // A drag commits when it ends, typing when the field is left,
                        // and a click on the arrows of a number right away.
                        if field.drag_stopped()
                            || field.lost_focus()
                            || (field.changed() && !field.dragged() && !field.has_focus())
                        {
                            commit = true;
                        }
                    }
                    if direction != draft.source.direction {
                        draft.source.direction = direction;
                        commit = true;
                    }
                    if commit {
                        let tag = draft.tag();
                        self.commit(editor, index, tag);
                    }
                }
            });
        }
        self.editing = editing;
        if let Some(index) = remove {
            match editor.remove_tag(index) {
                Ok(()) => self.error = None,
                Err(err) => self.error = Some(err.to_string()),
            }
        }
        if let Some(frame) = go_to {
            let _ = editor.set_active_frame(frame);
        }
        ui.horizontal(|ui| {
            if ui
                .button("Add tag")
                .on_hover_text("A new tag over the current frame. Change its range here.")
                .clicked()
            {
                let frame = editor.document().active_frame();
                let tag = FrameTag {
                    name: new_tag_name(editor.document().tags()),
                    from: frame,
                    to: frame,
                    direction: LoopDirection::Forward,
                    repeat: 0,
                };
                match editor.add_tag(tag) {
                    Ok(_) => self.error = None,
                    Err(err) => self.error = Some(err.to_string()),
                }
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(Color32::from_rgb(220, 100, 90), error);
        }
        ui.weak("A file with tags is saved as .oxypix version 2, which OxyPixel 0.8.5 and earlier cannot open.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, from: usize, to: usize) -> FrameTag {
        FrameTag {
            name: name.into(),
            from,
            to,
            direction: LoopDirection::Forward,
            repeat: 0,
        }
    }

    #[test]
    fn new_names_skip_ones_in_use() {
        assert_eq!(new_tag_name(&[]), "Tag 1");
        assert_eq!(
            new_tag_name(&[tag("Tag 1", 0, 0), tag("Tag 3", 0, 0)]),
            "Tag 2"
        );
    }

    #[test]
    fn drafts_show_frames_from_one_and_give_them_back_from_zero() {
        let original = tag("walk", 2, 4);
        let draft = Draft::of(&original);
        assert_eq!((draft.from, draft.to), (3, 5));
        assert_eq!(draft.tag(), original);
    }

    #[test]
    fn frame_tooltips_name_every_tag_on_the_frame() {
        let tags = [tag("walk", 0, 2), tag("hit", 2, 2)];
        assert_eq!(names_at(&tags, 2).as_deref(), Some("walk, hit"));
        assert_eq!(names_at(&tags, 3), None);
    }
}

//! Composited frames for the canvas, the frame strip, and the preview.
//!
//! A frame is composited at most once per document generation. A stroke only
//! throws away the active frame. Playback that does not edit the document
//! keeps every frame it has already built.

use oxypixel_core::Editor;
use oxypixel_core::Rgba;

struct Slot {
    generation: u64,
    pixels: Vec<Rgba>,
}

#[derive(Default)]
pub struct FrameComposites {
    generation: u64,
    width: u32,
    height: u32,
    frames: Vec<Option<Slot>>,
    #[cfg(test)]
    built: u32,
}

impl FrameComposites {
    #[cfg(test)]
    fn built(&self) -> u32 {
        self.built
    }

    /// Generation stored with this frame's pixels. `None` if it is not built.
    pub(crate) fn stamp(&self, index: usize) -> Option<u64> {
        self.frames
            .get(index)
            .and_then(|slot| slot.as_ref())
            .map(|slot| slot.generation)
    }

    pub(crate) fn pixels(&self, index: usize) -> Option<&[Rgba]> {
        self.frames
            .get(index)
            .and_then(|slot| slot.as_ref())
            .map(|slot| slot.pixels.as_slice())
    }

    /// Fill any requested frame that is missing. Leaves the others alone.
    pub(crate) fn prepare(&mut self, editor: &Editor, frames: &[usize]) {
        self.sync(editor);
        let generation = self.generation;
        for &index in frames {
            let Some(slot) = self.frames.get(index) else {
                continue;
            };
            if slot.is_some() {
                continue;
            }
            let Ok(pixels) = editor.document().composite(index) else {
                continue;
            };
            #[cfg(test)]
            {
                self.built = self.built.saturating_add(1);
            }
            self.frames[index] = Some(Slot { generation, pixels });
        }
    }

    fn sync(&mut self, editor: &Editor) {
        let doc = editor.document();
        let generation = editor.generation();
        let width = doc.width();
        let height = doc.height();
        let count = doc.frame_count();
        let same_shape = self.width == width && self.height == height && self.frames.len() == count;
        if generation == self.generation && same_shape {
            return;
        }
        if same_shape && editor.gesture_active() {
            let active = doc.active_frame();
            if let Some(slot) = self.frames.get_mut(active) {
                *slot = None;
            }
            self.generation = generation;
            return;
        }
        self.generation = generation;
        self.width = width;
        self.height = height;
        self.frames = (0..count).map(|_| None).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxypixel_core::MouseButton;

    #[test]
    fn a_stroke_recomposites_only_the_active_frame() {
        let mut editor = Editor::new(4, 4).expect("canvas");
        editor.add_frame().expect("frame");
        let mut cache = FrameComposites::default();
        cache.prepare(&editor, &[0, 1]);
        assert_eq!(cache.built(), 2);
        let stamp0 = cache.stamp(0).expect("frame 0");
        cache.prepare(&editor, &[0, 1]);
        assert_eq!(cache.built(), 2);

        editor.pointer_down(0, 0, MouseButton::Primary, false);
        assert!(editor.gesture_active());
        cache.prepare(&editor, &[0, 1]);
        assert_eq!(cache.built(), 3);
        assert_eq!(cache.stamp(0), Some(stamp0));
        assert_ne!(cache.stamp(1), Some(stamp0));
    }

    #[test]
    fn asking_twice_does_not_composite_again() {
        let editor = Editor::new(2, 2).expect("canvas");
        let mut cache = FrameComposites::default();
        cache.prepare(&editor, &[0]);
        cache.prepare(&editor, &[0]);
        assert_eq!(cache.built(), 1);
        assert!(cache.pixels(0).is_some());
    }
}

use crate::document::Document;
use crate::draw::PixelRect;
use crate::limits::{MAX_UNDO, MAX_UNDO_BYTES};

/// Document plus the selection rectangle, so undo puts the marquee back too.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub doc: Document,
    pub selection: Option<PixelRect>,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl History {
    pub(crate) fn push(&mut self, snapshot: Snapshot) {
        self.undo.push(snapshot);
        self.redo.clear();
        trim(&mut self.undo);
    }

    pub(crate) fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        trim(&mut self.redo);
        Some(previous)
    }

    pub(crate) fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        trim(&mut self.undo);
        Some(next)
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

fn trim(stack: &mut Vec<Snapshot>) {
    while stack.len() > MAX_UNDO {
        stack.remove(0);
    }
    while stack.len() > 1 && byte_size(stack) > MAX_UNDO_BYTES {
        stack.remove(0);
    }
}

fn byte_size(stack: &[Snapshot]) -> usize {
    stack.iter().map(|snap| snap.doc.pixel_bytes()).sum()
}

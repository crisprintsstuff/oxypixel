use crate::document::Document;
use crate::limits::{MAX_UNDO, MAX_UNDO_BYTES};

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Document>,
    redo: Vec<Document>,
}

impl History {
    pub(crate) fn push(&mut self, snapshot: Document) {
        self.undo.push(snapshot);
        self.redo.clear();
        trim(&mut self.undo);
    }

    pub(crate) fn undo(&mut self, current: Document) -> Option<Document> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        trim(&mut self.redo);
        Some(previous)
    }

    pub(crate) fn redo(&mut self, current: Document) -> Option<Document> {
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

fn trim(stack: &mut Vec<Document>) {
    while stack.len() > MAX_UNDO {
        stack.remove(0);
    }
    while stack.len() > 1 && byte_size(stack) > MAX_UNDO_BYTES {
        stack.remove(0);
    }
}

fn byte_size(stack: &[Document]) -> usize {
    stack.iter().map(Document::pixel_bytes).sum()
}

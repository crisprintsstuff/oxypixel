//! Animation tags: named frame ranges, saved in `.oxypix` version 2.

use crate::error::{Error, Result};
use crate::limits::{clamp_name, MAX_NAME_BYTES};

/// How a tag's frames play, as Aseprite names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LoopDirection {
    #[default]
    Forward,
    Reverse,
    PingPong,
    PingPongReverse,
}

impl LoopDirection {
    pub const ALL: [Self; 4] = [
        Self::Forward,
        Self::Reverse,
        Self::PingPong,
        Self::PingPongReverse,
    ];

    /// The word Aseprite writes in a JSON sheet's `frameTags`.
    pub fn json_name(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Reverse => "reverse",
            Self::PingPong => "pingpong",
            Self::PingPongReverse => "pingpong_reverse",
        }
    }

    /// The name shown in the editor.
    pub fn label(self) -> &'static str {
        match self {
            Self::Forward => "Forward",
            Self::Reverse => "Reverse",
            Self::PingPong => "Ping-pong",
            Self::PingPongReverse => "Ping-pong reverse",
        }
    }

    /// The byte stored in the file and in Aseprite files.
    pub fn code(self) -> u8 {
        match self {
            Self::Forward => 0,
            Self::Reverse => 1,
            Self::PingPong => 2,
            Self::PingPongReverse => 3,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }
}

/// A named range of frames, both ends included.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FrameTag {
    pub name: String,
    pub from: usize,
    pub to: usize,
    pub direction: LoopDirection,
    /// How many times the range plays. 0 is forever.
    pub repeat: u16,
}

impl FrameTag {
    pub fn contains(&self, frame: usize) -> bool {
        (self.from..=self.to).contains(&frame)
    }
}

/// The tag as it will be stored: the name trimmed of surrounding spaces and
/// cut to the name limit. A blank name, an empty range, or a frame past the
/// end is an error.
pub(crate) fn checked(mut tag: FrameTag, frame_count: usize) -> Result<FrameTag> {
    tag.name = clamp_name(tag.name.trim());
    if tag.name.is_empty() || tag.name.len() > MAX_NAME_BYTES {
        return Err(Error::BadTag("a tag needs a name".into()));
    }
    if tag.from > tag.to || tag.to >= frame_count {
        return Err(Error::BadTag(format!(
            "tag {} covers frames {} to {}, but there are {} frames",
            tag.name,
            tag.from + 1,
            tag.to + 1,
            frame_count
        )));
    }
    Ok(tag)
}

/// A frame was inserted at `index`. Tags at or after it move along; a tag
/// that already spans `index` grows by one.
pub(crate) fn frame_inserted(tags: &mut [FrameTag], index: usize) {
    for tag in tags {
        if tag.from >= index {
            tag.from += 1;
        }
        if tag.to >= index {
            tag.to += 1;
        }
    }
}

/// The frame at `index` was removed. A tag that covered only that frame is
/// dropped; the others shrink or move back.
pub(crate) fn frame_removed(tags: &mut Vec<FrameTag>, index: usize) {
    tags.retain(|tag| !(tag.from == index && tag.to == index));
    for tag in tags {
        if tag.from > index {
            tag.from -= 1;
        }
        if tag.to >= index {
            tag.to -= 1;
        }
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
    fn checked_trims_names_and_rejects_bad_ranges() {
        assert_eq!(checked(tag("  walk ", 0, 1), 2).unwrap().name, "walk");
        assert!(checked(tag("   ", 0, 0), 2).is_err());
        assert!(checked(tag("x", 1, 0), 2).is_err());
        assert!(checked(tag("x", 0, 2), 2).is_err());
        let long = "é".repeat(200);
        assert!(checked(tag(&long, 0, 0), 1).unwrap().name.len() <= MAX_NAME_BYTES);
    }

    #[test]
    fn inserting_and_removing_frames_moves_tags() {
        let mut tags = vec![tag("a", 0, 1), tag("b", 2, 3), tag("c", 4, 4)];
        // A frame inserted at 2: a ends before it, b and c move.
        frame_inserted(&mut tags, 2);
        assert_eq!(
            tags.iter().map(|t| (t.from, t.to)).collect::<Vec<_>>(),
            [(0, 1), (3, 4), (5, 5)]
        );
        // Inserted inside b, which grows.
        frame_inserted(&mut tags, 4);
        assert_eq!((tags[1].from, tags[1].to), (3, 5));
        // Removing c's only frame drops c; removing inside b shrinks it.
        frame_removed(&mut tags, 6);
        assert_eq!(tags.len(), 2);
        frame_removed(&mut tags, 3);
        assert_eq!((tags[1].from, tags[1].to), (3, 4));
        frame_removed(&mut tags, 0);
        assert_eq!((tags[0].from, tags[0].to), (0, 0));
    }

    #[test]
    fn direction_codes_round_trip() {
        for direction in LoopDirection::ALL {
            assert_eq!(LoopDirection::from_code(direction.code()), Some(direction));
        }
        assert_eq!(LoopDirection::from_code(4), None);
        assert_eq!(
            LoopDirection::PingPongReverse.json_name(),
            "pingpong_reverse"
        );
    }
}

use std::fmt;

/// Recoverable failure from a document edit or a file operation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the document needs at least one frame")]
    LastFrame,
    #[error("each frame needs at least one layer")]
    LastLayer,
    #[error("the bottom layer cannot be merged down")]
    BottomLayer,
    #[error("that {what} does not exist")]
    Index { what: &'static str },
    #[error("canvas size {width}×{height} is outside 1×1 … {max}×{max}")]
    Dimensions { width: u32, height: u32, max: u32 },
    #[error("{what} is {value}, and the limit is {max}")]
    Limit {
        what: &'static str,
        value: u64,
        max: u64,
    },
    #[error("this file is not an OxyPixel document")]
    BadMagic,
    #[error("unsupported OxyPixel file version {0}")]
    BadVersion(u16),
    #[error("the file is truncated or corrupt")]
    Truncated,
    #[error("a layer name in the file is not valid text")]
    BadName,
    #[error("invalid {0} in the file")]
    BadFlag(&'static str),
    #[error("the file has unexpected trailing data")]
    TrailingBytes,
    #[error("the palette needs at least one color")]
    EmptyPalette,
    #[error("the palette already has {0} colors")]
    PaletteFull(usize),
    #[error("{0}")]
    BadPalette(String),
    #[error("nothing is selected")]
    NoSelection,
    #[error("the turned pixels fall outside the canvas")]
    TransformOutside,
    #[error("the clipboard is empty")]
    EmptyClipboard,
    #[error("unsupported image format (import PNG, BMP, or GIF)")]
    UnsupportedFormat,
    #[error("could not decode the image: {0}")]
    Image(String),
    #[error("file error: {0}")]
    Io(String),
}

impl Error {
    pub(crate) fn image(err: impl fmt::Display) -> Self {
        Self::Image(err.to_string())
    }

    pub(crate) fn io(err: impl fmt::Display) -> Self {
        Self::Io(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

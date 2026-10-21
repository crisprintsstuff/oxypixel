//! System clipboard images for selection copy and paste.
//!
//! `oxypixel-core` stays free of this. A missing display, a refused
//! connection, or a clipboard that does not answer in time returns without
//! panicking, so the editor can paste its in-memory patch instead.
//!
//! The clipboard value stays on one background thread for the life of the
//! process. Linux clipboard owners have to keep serving the image; dropping
//! the value clears it for other programs. The set call does not wait for
//! another program to copy, because that wait would block until then.

use std::ffi::OsStr;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::OnceLock;
use std::time::Duration;

use oxypixel_core::{MAX_CEL_PIXELS, MAX_DIMENSION};

/// How long the window waits for the clipboard thread. The first connection
/// can be slow. A missing or stuck clipboard must not freeze the editor.
const CLIPBOARD_WAIT: Duration = Duration::from_millis(750);

#[derive(Debug)]
pub(crate) enum ClipboardRead {
    /// No clipboard, no image, or the call did not finish in time.
    Empty,
    /// An image was present and its size is outside the canvas caps.
    Oversize { width: u32, height: u32 },
    Image {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

/// Publish straight-alpha RGBA8. `false` means the in-memory patch is the copy.
pub(crate) fn publish_image(width: u32, height: u32, rgba: Vec<u8>) -> bool {
    if !buffer_fits(width, height, rgba.len()) || !session_available() {
        return false;
    }
    match exchange(Op::Publish {
        width,
        height,
        rgba,
    }) {
        Some(Outcome::Published(ok)) => ok,
        _ => false,
    }
}

pub(crate) fn read_image() -> ClipboardRead {
    if !session_available() {
        return ClipboardRead::Empty;
    }
    match exchange(Op::Read) {
        Some(Outcome::Read(value)) => value,
        _ => ClipboardRead::Empty,
    }
}

fn buffer_fits(width: u32, height: u32, len: usize) -> bool {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return false;
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_CEL_PIXELS {
        return false;
    }
    pixels.checked_mul(4).is_some_and(|n| n == len as u64)
}

fn session_available() -> bool {
    session_from(
        std::env::var_os("DISPLAY").as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
    )
}

/// `display` is `DISPLAY`, `wayland` is `WAYLAND_DISPLAY`. Empty values do not count.
fn session_from(display: Option<&OsStr>, wayland: Option<&OsStr>) -> bool {
    #[cfg(windows)]
    {
        let _ = (display, wayland);
        true
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        nonempty(display) || nonempty(wayland)
    }
    #[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
    {
        let _ = (display, wayland);
        false
    }
}

#[cfg(any(
    all(unix, not(target_os = "macos")),
    not(any(windows, all(unix, not(target_os = "macos"))))
))]
fn nonempty(value: Option<&OsStr>) -> bool {
    value.is_some_and(|text| !text.is_empty())
}

fn exchange(op: Op) -> Option<Outcome> {
    let tx = sender()?;
    let (reply_tx, reply_rx) = mpsc::channel();
    let failure = op.failure();
    if tx
        .try_send(Job {
            op,
            reply: reply_tx,
        })
        .is_err()
    {
        return Some(failure);
    }
    reply_rx.recv_timeout(CLIPBOARD_WAIT).ok()
}

fn sender() -> Option<&'static SyncSender<Job>> {
    static TX: OnceLock<SyncSender<Job>> = OnceLock::new();
    Some(TX.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel(1);
        // A failed spawn drops `rx`. Later sends fail immediately and the
        // editor keeps the in-memory patch.
        let _ = std::thread::Builder::new()
            .name("oxypixel-clipboard".to_string())
            .spawn(move || worker(rx));
        tx
    }))
}

enum Op {
    Publish {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Read,
}

impl Op {
    fn failure(&self) -> Outcome {
        match self {
            Op::Publish { .. } => Outcome::Published(false),
            Op::Read => Outcome::Read(ClipboardRead::Empty),
        }
    }
}

enum Outcome {
    Published(bool),
    Read(ClipboardRead),
}

struct Job {
    op: Op,
    reply: Sender<Outcome>,
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn worker(rx: Receiver<Job>) {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let mut clipboard = None;
    let mut failed = false;
    while let Ok(job) = rx.recv() {
        let outcome = if failed {
            job.op.failure()
        } else {
            let panicked = catch_unwind(AssertUnwindSafe(|| {
                dispatch(&mut clipboard, &mut failed, &job.op)
            }));
            match panicked {
                Ok(outcome) => outcome,
                Err(_) => {
                    failed = true;
                    job.op.failure()
                }
            }
        };
        let _ = job.reply.send(outcome);
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn worker(rx: Receiver<Job>) {
    while let Ok(job) = rx.recv() {
        let _ = job.reply.send(job.op.failure());
    }
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn dispatch(slot: &mut Option<arboard::Clipboard>, failed: &mut bool, op: &Op) -> Outcome {
    if !ensure(slot, failed) {
        return op.failure();
    }
    let Some(clipboard) = slot.as_mut() else {
        return op.failure();
    };
    match op {
        Op::Publish {
            width,
            height,
            rgba,
        } => Outcome::Published(publish(clipboard, *width, *height, rgba)),
        Op::Read => Outcome::Read(read(clipboard)),
    }
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn ensure(slot: &mut Option<arboard::Clipboard>, failed: &mut bool) -> bool {
    if *failed {
        return false;
    }
    if slot.is_some() {
        return true;
    }
    match arboard::Clipboard::new() {
        Ok(clipboard) => {
            *slot = Some(clipboard);
            true
        }
        Err(_) => {
            *failed = true;
            false
        }
    }
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn publish(clipboard: &mut arboard::Clipboard, width: u32, height: u32, rgba: &[u8]) -> bool {
    use std::borrow::Cow;

    if !buffer_fits(width, height, rgba.len()) {
        return false;
    }
    let image = arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: Cow::Borrowed(rgba),
    };
    // Do not call `wait`: on Linux that blocks until another program copies.
    clipboard.set_image(image).is_ok()
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
fn read(clipboard: &mut arboard::Clipboard) -> ClipboardRead {
    let image = match clipboard.get_image() {
        Ok(image) => image,
        Err(_) => return ClipboardRead::Empty,
    };
    let width = u32::try_from(image.width).unwrap_or(u32::MAX);
    let height = u32::try_from(image.height).unwrap_or(u32::MAX);
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_CEL_PIXELS
    {
        return ClipboardRead::Oversize { width, height };
    }
    let rgba = image.bytes.into_owned();
    if !buffer_fits(width, height, rgba.len()) {
        return ClipboardRead::Empty;
    }
    ClipboardRead::Image {
        width,
        height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_buffer_is_refused_before_any_clipboard_call() {
        let started = std::time::Instant::now();
        assert!(!publish_image(0, 0, Vec::new()));
        assert!(!publish_image(u32::MAX, u32::MAX, vec![0, 0, 0, 255]));
        assert!(!publish_image(1, 1, vec![0, 0, 0]));
        assert!(started.elapsed() < Duration::from_millis(250));
    }

    #[test]
    fn missing_display_is_not_a_clipboard_session() {
        #[cfg(windows)]
        assert!(session_from(None, None));

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            assert!(!session_from(None, None));
            assert!(!session_from(Some(OsStr::new("")), Some(OsStr::new(""))));
            assert!(session_from(Some(OsStr::new(":0")), None));
            assert!(session_from(None, Some(OsStr::new("wayland-0"))));
        }
    }
}

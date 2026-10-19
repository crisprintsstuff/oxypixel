//! Native file dialogs and error boxes.
//!
//! Windows uses the system dialog. Linux uses the XDG desktop portal when a
//! desktop provides one. If that returns no path, the in-app path field stays.

use std::path::{Path, PathBuf};

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
pub fn pick_path(
    save: bool,
    title: &str,
    filter: &str,
    extensions: &[&str],
    directory: Option<&Path>,
) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter, extensions);
    if let Some(dir) = directory {
        if dir.is_dir() {
            dialog = dialog.set_directory(dir);
        }
    }
    if save {
        dialog.save_file()
    } else {
        dialog.pick_file()
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
pub fn pick_path(
    save: bool,
    title: &str,
    filter: &str,
    extensions: &[&str],
    directory: Option<&Path>,
) -> Option<PathBuf> {
    let _ = (save, title, filter, extensions, directory);
    None
}

/// Error box for when the editor window itself cannot start.
///
/// Headless commands must not call this. On Windows, `show` blocks in
/// `MessageBoxW` until the box is dismissed.
pub fn alert(message: &str) {
    eprintln!("oxypixel: {message}");
    #[cfg(windows)]
    {
        let _ = rfd::MessageDialog::new()
            .set_title("OxyPixel")
            .set_description(message)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}

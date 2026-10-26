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

/// Several files at once. Empty when cancelled or when there is no dialog.
#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
pub fn pick_files(title: &str, filter: &str, extensions: &[&str]) -> Vec<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter, extensions)
        .pick_files()
        .unwrap_or_default()
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
pub fn pick_files(title: &str, filter: &str, extensions: &[&str]) -> Vec<PathBuf> {
    let _ = (title, filter, extensions);
    Vec::new()
}

/// One folder. `None` when cancelled or when there is no dialog.
#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    let _ = title;
    None
}

/// Set to anything but empty or `0`, a startup failure stays on stderr.
/// The CI launch check sets it, because nothing there can click a box.
pub const NO_DIALOG_ENV: &str = "OXYPIXEL_NO_DIALOG";

/// Error box for when the editor window itself cannot start.
///
/// Headless commands must not call this. On Windows, `show` blocks in
/// `MessageBoxW` until the box is dismissed.
pub fn alert(message: &str) {
    eprintln!("oxypixel: {message}");
    let allowed = dialog_allowed(std::env::var_os(NO_DIALOG_ENV).as_deref());
    #[cfg(windows)]
    if allowed {
        let _ = rfd::MessageDialog::new()
            .set_title("OxyPixel")
            .set_description(message)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
    #[cfg(not(windows))]
    let _ = allowed;
}

fn dialog_allowed(no_dialog: Option<&std::ffi::OsStr>) -> bool {
    match no_dialog {
        None => true,
        Some(value) => value.is_empty() || value == "0",
    }
}

#[cfg(test)]
mod alert_tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn no_dialog_env_keeps_the_error_on_stderr() {
        assert!(dialog_allowed(None));
        assert!(dialog_allowed(Some(OsStr::new(""))));
        assert!(dialog_allowed(Some(OsStr::new("0"))));
        assert!(!dialog_allowed(Some(OsStr::new("1"))));
        assert!(!dialog_allowed(Some(OsStr::new("yes"))));
    }
}

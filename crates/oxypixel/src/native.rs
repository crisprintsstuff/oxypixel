//! Native file dialogs and error boxes.
//!
//! Windows uses the system dialog. Linux uses the XDG desktop portal when a
//! desktop provides one. macOS runs the system's own `osascript` with
//! AppleScript's choose file dialogs, so no dialog library is needed there.
//! If a dialog returns no path, the in-app path field stays.

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

#[cfg(target_os = "macos")]
pub fn pick_path(
    save: bool,
    title: &str,
    filter: &str,
    extensions: &[&str],
    directory: Option<&Path>,
) -> Option<PathBuf> {
    let _ = filter;
    let dir = directory
        .filter(|dir| dir.is_dir())
        .map(|dir| dir.display().to_string())
        .unwrap_or_default();
    let script = if save { SAVE_SCRIPT } else { OPEN_SCRIPT };
    let mut args = vec![title.to_string(), dir];
    args.extend(extensions.iter().map(|ext| ext.to_string()));
    let out = osascript(script, &args)?;
    first_path(&out)
}

#[cfg(not(any(windows, unix)))]
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

#[cfg(target_os = "macos")]
pub fn pick_files(title: &str, filter: &str, extensions: &[&str]) -> Vec<PathBuf> {
    let _ = filter;
    let mut args = vec![title.to_string(), String::new()];
    args.extend(extensions.iter().map(|ext| ext.to_string()));
    osascript(OPEN_MANY_SCRIPT, &args)
        .map(|out| paths(&out))
        .unwrap_or_default()
}

#[cfg(not(any(windows, unix)))]
pub fn pick_files(title: &str, filter: &str, extensions: &[&str]) -> Vec<PathBuf> {
    let _ = (title, filter, extensions);
    Vec::new()
}

/// One folder. `None` when cancelled or when there is no dialog.
#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

#[cfg(target_os = "macos")]
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    first_path(&osascript(FOLDER_SCRIPT, &[title.to_string()])?)
}

#[cfg(not(any(windows, unix)))]
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    let _ = title;
    None
}

// AppleScript run by `osascript`. The title, folder, and extensions arrive
// as arguments (`argv`), never pasted into the script, so no text needs
// quoting. `activate` brings the dialog in front of the editor window.
// Cancel makes `osascript` exit with an error, which reads as no path.
#[cfg(any(test, target_os = "macos"))]
const OPEN_SCRIPT: &str = r#"on run argv
activate
set exts to rest of rest of argv
if item 2 of argv is "" then
set f to choose file with prompt (item 1 of argv) of type exts
else
set f to choose file with prompt (item 1 of argv) of type exts default location (POSIX file (item 2 of argv))
end if
return POSIX path of f
end run"#;

#[cfg(any(test, target_os = "macos"))]
const SAVE_SCRIPT: &str = r#"on run argv
activate
if item 2 of argv is "" then
set f to choose file name with prompt (item 1 of argv)
else
set f to choose file name with prompt (item 1 of argv) default location (POSIX file (item 2 of argv))
end if
return POSIX path of f
end run"#;

#[cfg(any(test, target_os = "macos"))]
const OPEN_MANY_SCRIPT: &str = r#"on run argv
activate
set exts to rest of rest of argv
set picked to choose file with prompt (item 1 of argv) of type exts with multiple selections allowed
set out to ""
repeat with f in picked
set out to out & (POSIX path of f) & linefeed
end repeat
return out
end run"#;

#[cfg(any(test, target_os = "macos"))]
const FOLDER_SCRIPT: &str = r#"on run argv
activate
return POSIX path of (choose folder with prompt (item 1 of argv))
end run"#;

#[cfg(any(test, target_os = "macos"))]
const ALERT_SCRIPT: &str = r#"on run argv
activate
display alert "OxyPixel" message (item 1 of argv) as critical
end run"#;

/// Run an AppleScript with arguments and return what it printed, or `None`
/// when it failed or was cancelled.
#[cfg(target_os = "macos")]
fn osascript(script: &str, args: &[String]) -> Option<String> {
    let mut command = std::process::Command::new("/usr/bin/osascript");
    for line in script.lines() {
        command.arg("-e").arg(line);
    }
    let output = command.args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Each non-empty line of `osascript` output as a path.
#[cfg(any(test, target_os = "macos"))]
fn paths(out: &str) -> Vec<PathBuf> {
    out.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(any(test, target_os = "macos"))]
fn first_path(out: &str) -> Option<PathBuf> {
    paths(out).into_iter().next()
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
    #[cfg(target_os = "macos")]
    if allowed {
        let _ = osascript(ALERT_SCRIPT, &[message.to_string()]);
    }
    #[cfg(not(any(windows, target_os = "macos")))]
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

    #[test]
    fn mac_dialog_scripts_take_their_text_as_arguments() {
        for script in [
            OPEN_SCRIPT,
            SAVE_SCRIPT,
            OPEN_MANY_SCRIPT,
            FOLDER_SCRIPT,
            ALERT_SCRIPT,
        ] {
            assert!(script.starts_with("on run argv\nactivate\n"), "{script}");
            assert!(script.ends_with("end run"), "{script}");
            // Nothing is formatted into a script, so a title cannot break out of it.
            assert!(!script.contains("{}"));
            assert_eq!(
                script.matches("\nif ").count(),
                script.matches("\nend if").count()
            );
        }
        assert_eq!(
            paths("/Users/a/one.png\n/Users/a/two words.png\n\n"),
            vec![
                PathBuf::from("/Users/a/one.png"),
                PathBuf::from("/Users/a/two words.png")
            ]
        );
        assert_eq!(first_path("\n"), None);
        assert_eq!(
            first_path("/tmp/x.oxypix\n"),
            Some(PathBuf::from("/tmp/x.oxypix"))
        );
    }
}

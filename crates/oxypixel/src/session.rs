//! Small window preferences stored next to recent files.

use std::path::PathBuf;

use oxypixel_core::MAX_DIMENSION;

use crate::canvas::{CheckerStyle, ONION_FRAME_CAP};
use crate::recent;

const FILE_NAME: &str = "view.txt";

/// Seconds between automatic update checks.
pub const UPDATE_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Interface scale choices, in percent.
pub const UI_SCALES: [u16; 6] = [75, 90, 100, 125, 150, 200];
/// Export scale choices.
pub const EXPORT_SCALES: [u32; 3] = [1, 2, 4];

/// The window preferences in `view.txt`, one `key=value` per line. The
/// Settings window edits them; a missing or bad line keeps the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// Whether the preview panel was left open.
    pub preview_open: bool,
    /// Check GitHub at startup and download a newer release. Off unless
    /// the user turns it on.
    pub auto_update: bool,
    /// When the last automatic check started, in seconds since 1970.
    pub last_update_check: u64,
    /// Interface scale in percent, one of [`UI_SCALES`].
    pub ui_scale: u16,
    /// The size File, then New starts with.
    pub new_width: u32,
    pub new_height: u32,
    /// Whether the pixel grid starts on.
    pub grid: bool,
    /// What shows behind clear pixels.
    pub checker: CheckerStyle,
    /// Onion skin settings it starts with. Onion skin itself starts off.
    pub onion_previous: u32,
    pub onion_next: u32,
    pub onion_opacity: u8,
    pub onion_tint: bool,
    /// The PNG and spritesheet export scale it starts with, one of [`EXPORT_SCALES`].
    pub export_scale: u32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            preview_open: true,
            auto_update: false,
            last_update_check: 0,
            ui_scale: 100,
            new_width: 64,
            new_height: 64,
            grid: true,
            checker: CheckerStyle::Dark,
            onion_previous: 1,
            onion_next: 1,
            onion_opacity: 80,
            onion_tint: true,
            export_scale: 1,
        }
    }
}

impl Prefs {
    /// Every setting back to its default. The update check time is kept, so
    /// a reset does not start an extra check.
    pub fn reset(&mut self) {
        *self = Self {
            last_update_check: self.last_update_check,
            ..Self::default()
        };
    }
}

/// The saved preferences. A missing file or line keeps the default.
pub fn load() -> Prefs {
    view_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

pub fn save(prefs: &Prefs) {
    let Some(path) = view_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, format(prefs));
}

pub(crate) fn parse(text: &str) -> Prefs {
    let mut prefs = Prefs::default();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let flag = match value.trim() {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        };
        let number = value.trim().parse::<u64>().ok();
        let d = Prefs::default();
        match key.trim() {
            "preview" => prefs.preview_open = flag.unwrap_or(prefs.preview_open),
            "auto_update" => prefs.auto_update = flag.unwrap_or(prefs.auto_update),
            "last_update_check" => prefs.last_update_check = number.unwrap_or(0),
            "ui_scale" => {
                prefs.ui_scale = number
                    .and_then(|n| u16::try_from(n).ok())
                    .filter(|n| UI_SCALES.contains(n))
                    .unwrap_or(d.ui_scale)
            }
            "new_width" => prefs.new_width = side(number).unwrap_or(d.new_width),
            "new_height" => prefs.new_height = side(number).unwrap_or(d.new_height),
            "grid" => prefs.grid = flag.unwrap_or(d.grid),
            "checker" => prefs.checker = CheckerStyle::from_name(value.trim()).unwrap_or(d.checker),
            "onion_previous" => prefs.onion_previous = frames(number).unwrap_or(d.onion_previous),
            "onion_next" => prefs.onion_next = frames(number).unwrap_or(d.onion_next),
            "onion_opacity" => {
                prefs.onion_opacity = number
                    .and_then(|n| u8::try_from(n).ok())
                    .unwrap_or(d.onion_opacity)
            }
            "onion_tint" => prefs.onion_tint = flag.unwrap_or(d.onion_tint),
            "export_scale" => {
                prefs.export_scale = number
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| EXPORT_SCALES.contains(n))
                    .unwrap_or(d.export_scale)
            }
            _ => {}
        }
    }
    prefs
}

fn side(number: Option<u64>) -> Option<u32> {
    number
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| (1..=MAX_DIMENSION).contains(n))
}

fn frames(number: Option<u64>) -> Option<u32> {
    number
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n <= ONION_FRAME_CAP)
}

pub(crate) fn format(prefs: &Prefs) -> String {
    let flag = |on: bool| u8::from(on);
    [
        format!("preview={}", flag(prefs.preview_open)),
        format!("auto_update={}", flag(prefs.auto_update)),
        format!("last_update_check={}", prefs.last_update_check),
        format!("ui_scale={}", prefs.ui_scale),
        format!("new_width={}", prefs.new_width),
        format!("new_height={}", prefs.new_height),
        format!("grid={}", flag(prefs.grid)),
        format!("checker={}", prefs.checker.name()),
        format!("onion_previous={}", prefs.onion_previous),
        format!("onion_next={}", prefs.onion_next),
        format!("onion_opacity={}", prefs.onion_opacity),
        format!("onion_tint={}", flag(prefs.onion_tint)),
        format!("export_scale={}", prefs.export_scale),
    ]
    .iter()
    .map(|line| format!("{line}\n"))
    .collect()
}

/// Whether an automatic update check should run at `now`. Never when the
/// user has not turned it on; otherwise once a day, or again if the clock
/// went backwards.
pub fn update_check_due(prefs: &Prefs, now: u64) -> bool {
    prefs.auto_update
        && (now < prefs.last_update_check || now - prefs.last_update_check >= UPDATE_INTERVAL_SECS)
}

/// Seconds since 1970, or 0 if the clock is before it.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn view_path() -> Option<PathBuf> {
    recent::data_dir().map(|dir| dir.join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvVar {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVar {
        fn set(key: &'static str, value: &std::ffi::OsStr) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVar {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn prefs_parse_and_round_trip() {
        assert!(parse("preview=1\n").preview_open);
        assert!(!parse("preview=0\n").preview_open);
        assert!(!parse("preview=false\n").preview_open);
        assert_eq!(parse("other=1\n"), Prefs::default());
        assert_eq!(parse("preview=maybe\nauto_update=yes\n"), Prefs::default());
        // A file from an older version has only the preview line.
        let old = parse("preview=0\n");
        assert!(!old.auto_update);
        let prefs = Prefs {
            preview_open: false,
            auto_update: true,
            last_update_check: 1_800_000_000,
            ui_scale: 150,
            new_width: 32,
            new_height: 48,
            grid: false,
            checker: CheckerStyle::Light,
            onion_previous: 3,
            onion_next: 0,
            onion_opacity: 200,
            onion_tint: false,
            export_scale: 4,
        };
        assert_eq!(parse(&format(&prefs)), prefs);
        assert_eq!(parse("last_update_check=-5\n").last_update_check, 0);

        let root = std::env::temp_dir().join("oxypixel-view-pref-test");
        let _ = std::fs::remove_dir_all(&root);
        #[cfg(windows)]
        let key = "APPDATA";
        #[cfg(not(windows))]
        let key = "XDG_DATA_HOME";
        let _restore = EnvVar::set(key, root.as_os_str());
        assert_eq!(load(), Prefs::default());
        save(&prefs);
        assert_eq!(load(), prefs);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn out_of_range_settings_fall_back_to_defaults() {
        let d = Prefs::default();
        let bad = parse(
            "ui_scale=137\nnew_width=0\nnew_height=5000\ngrid=perhaps\nchecker=plaid\n\
             onion_previous=9\nonion_next=-1\nonion_opacity=256\nexport_scale=3\n",
        );
        assert_eq!(bad, d);
        let edge = parse("new_width=1\nnew_height=1024\nonion_previous=8\nonion_opacity=0\n");
        assert_eq!((edge.new_width, edge.new_height), (1, 1024));
        assert_eq!((edge.onion_previous, edge.onion_opacity), (8, 0));
    }

    #[test]
    fn reset_keeps_the_update_check_time() {
        let mut prefs = parse("grid=0\nui_scale=200\nauto_update=1\nlast_update_check=99\n");
        prefs.reset();
        assert_eq!(
            prefs,
            Prefs {
                last_update_check: 99,
                ..Prefs::default()
            }
        );
    }

    #[test]
    fn automatic_checks_are_opt_in_and_daily() {
        let mut prefs = Prefs::default();
        assert!(!update_check_due(&prefs, 1_000_000));
        prefs.auto_update = true;
        assert!(update_check_due(&prefs, 1_000_000));
        prefs.last_update_check = 1_000_000;
        assert!(!update_check_due(
            &prefs,
            1_000_000 + UPDATE_INTERVAL_SECS - 1
        ));
        assert!(update_check_due(&prefs, 1_000_000 + UPDATE_INTERVAL_SECS));
        // A clock set back does not stop checks for good.
        assert!(update_check_due(&prefs, 5));
    }
}

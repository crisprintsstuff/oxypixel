//! Small window preferences stored next to recent files.

use std::path::PathBuf;

use crate::recent;

const FILE_NAME: &str = "view.txt";

/// Seconds between automatic update checks.
pub const UPDATE_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// The window preferences in `view.txt`, one `key=value` per line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// Whether the preview panel was left open.
    pub preview_open: bool,
    /// Check GitHub at startup and download a newer release. Off unless
    /// the user turns it on.
    pub auto_update: bool,
    /// When the last automatic check started, in seconds since 1970.
    pub last_update_check: u64,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            preview_open: true,
            auto_update: false,
            last_update_check: 0,
        }
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
        match key.trim() {
            "preview" => prefs.preview_open = flag.unwrap_or(prefs.preview_open),
            "auto_update" => prefs.auto_update = flag.unwrap_or(prefs.auto_update),
            "last_update_check" => {
                prefs.last_update_check = value.trim().parse().unwrap_or(0);
            }
            _ => {}
        }
    }
    prefs
}

pub(crate) fn format(prefs: &Prefs) -> String {
    format!(
        "preview={}\nauto_update={}\nlast_update_check={}\n",
        u8::from(prefs.preview_open),
        u8::from(prefs.auto_update),
        prefs.last_update_check
    )
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

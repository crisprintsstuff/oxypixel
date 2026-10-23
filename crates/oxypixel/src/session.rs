//! Small window preferences stored next to recent files.

use std::path::PathBuf;

use crate::recent;

const FILE_NAME: &str = "view.txt";

/// Whether the preview panel was left open. Missing file means open.
pub fn load_preview_open() -> bool {
    let Some(path) = view_path() else {
        return true;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return true;
    };
    parse_preview_open(&text).unwrap_or(true)
}

pub fn save_preview_open(open: bool) {
    let Some(path) = view_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = if open { "preview=1\n" } else { "preview=0\n" };
    let _ = std::fs::write(path, text);
}

pub(crate) fn parse_preview_open(text: &str) -> Option<bool> {
    for line in text.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("preview=") else {
            continue;
        };
        return match value.trim() {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        };
    }
    None
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
    fn preview_flag_parses_and_round_trips() {
        assert_eq!(parse_preview_open("preview=1\n"), Some(true));
        assert_eq!(parse_preview_open("preview=0\n"), Some(false));
        assert_eq!(parse_preview_open("preview=false\n"), Some(false));
        assert_eq!(parse_preview_open("other=1\n"), None);
        assert_eq!(parse_preview_open("preview=maybe\n"), None);

        let root = std::env::temp_dir().join("oxypixel-view-pref-test");
        let _ = std::fs::remove_dir_all(&root);
        #[cfg(windows)]
        let key = "APPDATA";
        #[cfg(not(windows))]
        let key = "XDG_DATA_HOME";
        let _restore = EnvVar::set(key, root.as_os_str());
        assert!(load_preview_open());
        save_preview_open(false);
        assert!(!load_preview_open());
        save_preview_open(true);
        assert!(load_preview_open());
        let _ = std::fs::remove_dir_all(&root);
    }
}

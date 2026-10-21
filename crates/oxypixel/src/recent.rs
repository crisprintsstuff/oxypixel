//! Recent `.oxypix` paths. A short text file under the user data directory.

use std::path::{Path, PathBuf};

const LIMIT: usize = 5;

pub fn load() -> Vec<PathBuf> {
    let Some(path) = store_path() else {
        return Vec::new();
    };
    read(&path)
}

pub fn save(paths: &[PathBuf]) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = write(&path, paths);
}

pub fn push(paths: &mut Vec<PathBuf>, path: &Path) {
    paths.retain(|existing| existing != path);
    paths.insert(0, path.to_path_buf());
    paths.truncate(LIMIT);
}

fn store_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("recent.txt"))
}

fn data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("OxyPixel"))
    }
    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            let path = PathBuf::from(xdg);
            if path.components().next().is_some() {
                return Some(path.join("oxypixel"));
            }
        }
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/oxypixel"))
    }
}

fn read(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .take(LIMIT)
        .collect()
}

fn write(path: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
    let mut text = String::new();
    for path in paths.iter().take(LIMIT) {
        if let Some(line) = path.to_str() {
            text.push_str(line);
            text.push('\n');
        }
    }
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_keeps_the_newest_five_and_drops_duplicates() {
        let mut paths = Vec::new();
        for name in ["a", "b", "c", "d", "e", "f"] {
            push(&mut paths, Path::new(name));
        }
        push(&mut paths, Path::new("c"));
        assert_eq!(paths.len(), 5);
        assert_eq!(paths[0], PathBuf::from("c"));
        assert!(!paths.iter().any(|p| p == Path::new("a")));
    }

    /// Puts an environment variable back when the test returns.
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
    fn data_dir_uses_the_platform_folder() {
        let root = std::env::temp_dir().join("oxypixel-recent-dir-test");
        #[cfg(windows)]
        let (key, leaf) = ("APPDATA", "OxyPixel");
        #[cfg(not(windows))]
        let (key, leaf) = ("XDG_DATA_HOME", "oxypixel");
        let _restore = EnvVar::set(key, root.as_os_str());
        let dir = data_dir().expect("data directory");
        assert_eq!(dir.file_name().and_then(|name| name.to_str()), Some(leaf));
        assert_eq!(dir.parent(), Some(root.as_path()));
    }
}

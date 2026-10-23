//! Recent `.oxypix` paths. A short text file under the user data directory.

use std::path::{Path, PathBuf};

const LIMIT: usize = 10;

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

pub fn clear(paths: &mut Vec<PathBuf>) {
    paths.clear();
}

/// Menu label. Duplicate file names gain a parent-folder hint. A path that is not a file is marked missing.
pub fn row_label(path: &Path, paths: &[PathBuf]) -> String {
    let mut label = disambiguated_name(path, paths);
    if !path.is_file() {
        label.push_str(" (missing)");
    }
    label
}

fn disambiguated_name(path: &Path, paths: &[PathBuf]) -> String {
    let name = file_name(path);
    let siblings: Vec<&Path> = paths
        .iter()
        .map(PathBuf::as_path)
        .filter(|other| file_name(other) == name)
        .collect();
    if siblings.len() <= 1 {
        return name;
    }
    let max_parents = path.components().count();
    for parents in 1..max_parents {
        let Some(mine) = parent_hint(path, parents) else {
            continue;
        };
        let unique = siblings
            .iter()
            .filter(|other| parent_hint(other, parents).as_deref() == Some(mine.as_str()))
            .count()
            == 1;
        if unique {
            return format!("{name} — {mine}");
        }
    }
    path.display().to_string()
}

fn parent_hint(path: &Path, parents: usize) -> Option<String> {
    let comps: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parents == 0 || comps.len() < parents + 1 {
        return None;
    }
    let start = comps.len() - 1 - parents;
    Some(comps[start..comps.len() - 1].join("/"))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn store_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("recent.txt"))
}

pub fn data_dir() -> Option<PathBuf> {
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
    fn push_keeps_the_newest_ten_and_drops_duplicates() {
        let mut paths = Vec::new();
        for name in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"] {
            push(&mut paths, Path::new(name));
        }
        push(&mut paths, Path::new("c"));
        assert_eq!(paths.len(), 10);
        assert_eq!(paths[0], PathBuf::from("c"));
        assert!(!paths.iter().any(|p| p == Path::new("a")));
        assert!(!paths.iter().any(|p| p == Path::new("b")));
    }

    #[test]
    fn clear_drops_every_path() {
        let mut paths = vec![PathBuf::from("a"), PathBuf::from("b")];
        clear(&mut paths);
        assert!(paths.is_empty());
    }

    #[test]
    fn duplicate_names_show_the_parent_folder() {
        let paths = vec![
            PathBuf::from("work").join("art").join("sprite.oxypix"),
            PathBuf::from("work").join("tiles").join("sprite.oxypix"),
            PathBuf::from("work").join("art").join("hero.oxypix"),
        ];
        assert_eq!(disambiguated_name(&paths[0], &paths), "sprite.oxypix — art");
        assert_eq!(
            disambiguated_name(&paths[1], &paths),
            "sprite.oxypix — tiles"
        );
        assert_eq!(disambiguated_name(&paths[2], &paths), "hero.oxypix");
    }

    #[test]
    fn same_parent_folder_name_uses_a_longer_hint() {
        let paths = vec![
            PathBuf::from("a").join("proj").join("sprite.oxypix"),
            PathBuf::from("b").join("proj").join("sprite.oxypix"),
        ];
        assert_eq!(
            disambiguated_name(&paths[0], &paths),
            "sprite.oxypix — a/proj"
        );
        assert_eq!(
            disambiguated_name(&paths[1], &paths),
            "sprite.oxypix — b/proj"
        );
    }

    #[test]
    fn missing_file_is_marked_and_a_real_file_is_not() {
        let dir = std::env::temp_dir().join("oxypixel-recent-label-test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let live = dir.join("live.oxypix");
        std::fs::write(&live, b"ok").expect("temp file");
        let gone = dir.join("gone.oxypix");
        let _ = std::fs::remove_file(&gone);
        let paths = vec![live.clone(), gone.clone()];
        assert!(!row_label(&live, &paths).contains("missing"));
        assert!(row_label(&gone, &paths).ends_with(" (missing)"));
        let _ = std::fs::remove_file(&live);
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

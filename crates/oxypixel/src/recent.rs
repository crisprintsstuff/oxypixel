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
        return std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("OxyPixel"));
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
}

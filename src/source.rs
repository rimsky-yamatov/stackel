//! Where source files come from: the real file system, or memory (for tests).

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The file system operations the module loader needs.
pub trait Fs {
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
}

/// The real file system.
pub struct RealFs;

impl Fs for RealFs {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

/// An in-memory file system.
#[derive(Default)]
pub struct MemFs {
    files: HashMap<PathBuf, String>,
}

impl MemFs {
    pub fn new() -> Self {
        MemFs::default()
    }

    /// Adds a file (builder style).
    pub fn with(mut self, path: &str, content: &str) -> Self {
        self.files
            .insert(normalize(Path::new(path)), content.to_string());
        self
    }
}

impl Fs for MemFs {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.files
            .get(&normalize(path))
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.contains_key(&normalize(path))
    }

    fn is_dir(&self, path: &Path) -> bool {
        let p = normalize(path);
        self.files.keys().any(|k| k != &p && k.starts_with(&p))
    }
}

/// Removes `.` components and resolves `..` lexically (it does not touch the file system).
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(out.components().next_back(), Some(Component::Normal(_)));
                if can_pop {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_dots() {
        assert_eq!(normalize(Path::new("a/./b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalize(Path::new("app/../mathx")), PathBuf::from("mathx"));
        assert_eq!(normalize(Path::new("../x")), PathBuf::from("../x"));
        assert_eq!(normalize(Path::new("a/../..")), PathBuf::from(".."));
        assert_eq!(normalize(Path::new(".")), PathBuf::new());
    }

    #[test]
    fn memory_file_system() {
        let fs = MemFs::new().with("a/b.stk", "x");
        assert!(fs.is_file(Path::new("a/b.stk")));
        assert!(fs.is_file(Path::new("./a/../a/b.stk")));
        assert!(fs.is_dir(Path::new("a")));
        assert!(!fs.is_dir(Path::new("a/b.stk")));
        assert!(!fs.is_file(Path::new("a")));
        assert_eq!(fs.read_to_string(Path::new("a/b.stk")).unwrap(), "x");
        assert!(fs.read_to_string(Path::new("missing")).is_err());
    }
}

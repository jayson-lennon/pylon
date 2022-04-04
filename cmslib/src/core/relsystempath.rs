use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use serde::Serialize;

/// Relative path to a resource on the system.
#[derive(Clone, Debug, Default, Serialize, Hash, Eq, PartialEq)]
pub struct RelSystemPath {
    base: PathBuf,
    target: PathBuf,
}

impl RelSystemPath {
    #[must_use]
    pub fn new<P: Into<PathBuf> + std::fmt::Debug>(base: P, target: P) -> Self {
        let base = base.into();
        let target = target.into();
        debug_assert!(target.file_name().is_some());
        Self { base, target }
    }

    #[must_use]
    pub fn with_base<P: Into<PathBuf>>(&self, base: P) -> Self {
        let base = base.into();
        Self {
            base,
            target: self.target.clone(),
        }
    }

    #[must_use]
    pub fn with_extension<S: AsRef<str>>(&self, extension: S) -> Self {
        let extension: &str = extension.as_ref();
        let mut target = self.target.clone();
        target.set_extension(extension);
        Self {
            base: self.base.clone(),
            target,
        }
    }

    #[must_use]
    pub fn with_file_name<S: AsRef<str>>(&self, name: S) -> Self {
        let name: &str = name.as_ref();
        let mut target = self.target.clone();
        target.set_file_name(name);
        Self {
            target,
            base: self.base.clone(),
        }
    }

    #[must_use]
    pub fn add_parent<P: AsRef<Path>>(&self, parent: P) -> Self {
        let name = self.target.clone();
        let name = name.file_name().unwrap();

        let mut target = self.target.clone();
        target.pop();
        target.push(parent);
        target.push(name);
        Self {
            base: self.base.clone(),
            target,
        }
    }

    #[must_use]
    pub fn remove_parent(&self) -> Self {
        let name = self.target.clone();
        let name = name.file_name().unwrap();

        let mut target = self.target.clone();
        target.pop();
        target.pop();
        target.push(name);
        Self {
            base: self.base.clone(),
            target,
        }
    }

    #[must_use]
    pub fn with_file_stem<S: AsRef<Path>>(&self, stem: S) -> Self {
        let extension = self.target.clone();
        let extension = extension.extension().unwrap();

        let mut target = self.target.clone();
        target.pop();
        target.push(stem);
        target.set_extension(extension);
        Self {
            target,
            base: self.base.clone(),
        }
    }

    #[must_use]
    pub fn pop(&self) -> Self {
        let mut target = self.target.clone();
        target.pop();
        Self {
            target,
            base: self.base.clone(),
        }
    }

    pub fn file_name(&self) -> &OsStr {
        debug_assert!(self.target.file_name().is_some());
        self.target.file_name().unwrap()
    }

    pub fn file_stem(&self) -> &OsStr {
        debug_assert!(self.target.file_stem().is_some());
        self.target.file_stem().unwrap()
    }

    pub fn target(&self) -> &Path {
        self.target.as_path()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self)
    }
}

impl From<RelSystemPath> for PathBuf {
    fn from(path: RelSystemPath) -> Self {
        let mut new_path = path.base;
        new_path.push(path.target);
        new_path
    }
}

impl From<&RelSystemPath> for PathBuf {
    fn from(path: &RelSystemPath) -> Self {
        let mut new_path = path.base.clone();
        new_path.push(path.target.clone());
        new_path
    }
}

impl std::fmt::Display for RelSystemPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_path_buf().display())
    }
}

#[cfg(test)]
mod test {
    use super::RelSystemPath;
    use std::path::PathBuf;

    #[test]
    fn changes_base() {
        let path = RelSystemPath::new("base", "target");
        let updated = path.with_base("changed");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("changed/target"));
    }

    #[test]
    fn changes_extension() {
        let path = RelSystemPath::new("base", "target.ext");
        let updated = path.with_extension("new");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/target.new"));
    }

    #[test]
    fn removes_extension() {
        let path = RelSystemPath::new("base", "target.ext");
        let updated = path.with_extension("");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/target"));
    }

    #[test]
    fn changes_file_name() {
        let path = RelSystemPath::new("base", "original_file");
        let updated = path.with_file_name("new_file");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/new_file"));
    }

    #[test]
    fn removes_file_name() {
        let path = RelSystemPath::new("base", "original_file");
        let updated = path.with_file_name("");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base"));
    }

    #[test]
    fn changes_file_stem() {
        let path = RelSystemPath::new("base", "original_file.ext");
        let updated = path.with_file_stem("new_file");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/new_file.ext"));
    }

    #[test]
    fn adds_parent() {
        let path = RelSystemPath::new("base", "a/b/file.test");
        let updated = path.add_parent("t");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/a/b/t/file.test"));
    }

    #[test]
    fn adding_blank_parent_is_noop() {
        let path = RelSystemPath::new("base", "a/b/file.test");
        let updated = path.add_parent("");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/a/b/file.test"));
    }

    #[test]
    fn removes_parent() {
        let path = RelSystemPath::new("base", "a/b/file.test");
        let updated = path.remove_parent();
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/a/file.test"));
    }

    #[test]
    fn gets_file_name() {
        let path = RelSystemPath::new("base", "a/b/file.ext");
        assert_eq!(path.file_name(), "file.ext");
    }

    #[test]
    fn gets_file_stem() {
        let path = RelSystemPath::new("base", "a/b/file.ext");
        assert_eq!(path.file_stem(), "file");
    }

    #[test]
    fn display_impl() {
        let path = RelSystemPath::new("base", "a/b/file");
        assert_eq!(path.to_string(), "base/a/b/file");

        let path = RelSystemPath::new("", "a/b/file");
        assert_eq!(path.to_string(), "a/b/file");
    }

    #[test]
    fn to_path_buf() {
        let path = RelSystemPath::new("base", "a/b/file");
        let buf = path.to_path_buf();

        assert_eq!(buf, PathBuf::from("base/a/b/file"));
    }

    #[test]
    fn from_rel_system_path_ref() {
        let path = RelSystemPath::new("base", "a/b/file");
        let buf = PathBuf::from(&path);

        assert_eq!(buf, PathBuf::from("base/a/b/file"));
    }
}

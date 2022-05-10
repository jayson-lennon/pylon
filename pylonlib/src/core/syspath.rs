use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use crate::Result;
use anyhow::anyhow;
use serde::Serialize;

/// Relative path to a resource on the system.
#[derive(Clone, Debug, Default, Serialize, Hash, Eq, PartialEq)]
pub struct SysPath {
    base: PathBuf,
    target: PathBuf,
}

impl SysPath {
    pub fn new<B, T>(base: B, target: T) -> Result<Self>
    where
        B: Into<PathBuf> + std::fmt::Debug,
        T: AsRef<Path> + std::fmt::Debug,
    {
        let base = base.into();
        let target = target.as_ref();

        if target.file_name().is_none() {
            return Err(anyhow!("target must have filename"));
        }

        let target = {
            if target.has_root() {
                target.strip_prefix(&base)?.to_path_buf()
            } else {
                target.to_path_buf()
            }
        };

        Ok(Self { base, target })
    }

    pub fn with_base<P: Into<PathBuf>>(&self, base: P) -> Self {
        let base = base.into();
        Self {
            base,
            target: self.target.clone(),
        }
    }

    pub fn with_extension<S: AsRef<str>>(&self, extension: S) -> Self {
        let extension: &str = extension.as_ref();
        let mut target = self.target.clone();
        target.set_extension(extension);
        Self {
            base: self.base.clone(),
            target,
        }
    }

    pub fn with_file_name<S: AsRef<str>>(&self, name: S) -> Self {
        let name: &str = name.as_ref();
        let mut target = self.target.clone();
        target.set_file_name(name);
        Self {
            target,
            base: self.base.clone(),
        }
    }

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

    pub fn pop(&self) -> Option<Self> {
        let mut target = self.target.clone();
        if target.pop() {
            Self {
                target,
                base: self.base.clone(),
            }
        } else {
            None
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

    pub fn base(&self) -> &Path {
        self.base.as_path()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self)
    }
}

impl From<SysPath> for PathBuf {
    fn from(path: SysPath) -> Self {
        let mut new_path = path.base;
        new_path.push(path.target);
        new_path
    }
}

impl From<&SysPath> for PathBuf {
    fn from(path: &SysPath) -> Self {
        let mut new_path = path.base.clone();
        new_path.push(path.target.clone());
        new_path
    }
}

impl std::fmt::Display for SysPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_path_buf().display())
    }
}

#[cfg(test)]
mod test {
    
    #![allow(warnings, unused)]
    use super::SysPath;
    use std::path::PathBuf;

    #[test]
    fn changes_base() {
        let path = SysPath::new("base", "target").unwrap();
        let updated = path.with_base("changed");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("changed/target"));
    }

    #[test]
    fn changes_extension() {
        let path = SysPath::new("base", "target.ext").unwrap();
        let updated = path.with_extension("new");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/target.new"));
    }

    #[test]
    fn removes_extension() {
        let path = SysPath::new("base", "target.ext").unwrap();
        let updated = path.with_extension("");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/target"));
    }

    #[test]
    fn changes_file_name() {
        let path = SysPath::new("base", "original_file").unwrap();
        let updated = path.with_file_name("new_file");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/new_file"));
    }

    #[test]
    fn removes_file_name() {
        let path = SysPath::new("base", "original_file").unwrap();
        let updated = path.with_file_name("");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base"));
    }

    #[test]
    fn changes_file_stem() {
        let path = SysPath::new("base", "original_file.ext").unwrap();
        let updated = path.with_file_stem("new_file");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/new_file.ext"));
    }

    #[test]
    fn adds_parent() {
        let path = SysPath::new("base", "a/b/file.test").unwrap();
        let updated = path.add_parent("t");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/a/b/t/file.test"));
    }

    #[test]
    fn adding_blank_parent_is_noop() {
        let path = SysPath::new("base", "a/b/file.test").unwrap();
        let updated = path.add_parent("");
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/a/b/file.test"));
    }

    #[test]
    fn removes_parent() {
        let path = SysPath::new("base", "a/b/file.test").unwrap();
        let updated = path.remove_parent();
        let buf = PathBuf::from(updated);
        assert_eq!(buf, PathBuf::from("base/a/file.test"));
    }

    #[test]
    fn gets_file_name() {
        let path = SysPath::new("base", "a/b/file.ext").unwrap();
        assert_eq!(path.file_name(), "file.ext");
    }

    #[test]
    fn gets_file_stem() {
        let path = SysPath::new("base", "a/b/file.ext").unwrap();
        assert_eq!(path.file_stem(), "file");
    }

    #[test]
    fn display_impl() {
        let path = SysPath::new("base", "a/b/file").unwrap();
        assert_eq!(path.to_string(), "base/a/b/file");

        let path = SysPath::new("", "a/b/file").unwrap();
        assert_eq!(path.to_string(), "a/b/file");
    }

    #[test]
    fn to_path_buf() {
        let path = SysPath::new("base", "a/b/file").unwrap();
        let buf = path.to_path_buf();

        assert_eq!(buf, PathBuf::from("base/a/b/file"));
    }

    #[test]
    fn from_rel_system_path_ref() {
        let path = SysPath::new("base", "a/b/file").unwrap();
        let buf = PathBuf::from(&path);

        assert_eq!(buf, PathBuf::from("base/a/b/file"));
    }
}

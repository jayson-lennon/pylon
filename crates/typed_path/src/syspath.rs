use derivative::Derivative;
use eyre::WrapErr;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::{AbsPath, PathMarker, RelPath};
use crate::{ConfirmedPath, Result, TypedPath};

use serde::Serialize;

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct SysPath {
    root: PathBuf,
    base: PathBuf,
    target: PathBuf,
}

impl Eq for SysPath {}

impl SysPath {
    pub fn new(root: &AbsPath, base: &RelPath, target: &RelPath) -> Self {
        Self {
            root: root.to_path_buf(),
            base: base.to_path_buf(),
            target: target.to_path_buf(),
        }
    }

    pub fn with_root(&self, root: &AbsPath) -> Self {
        Self {
            base: self.base.clone(),
            root: root.to_path_buf(),
            target: self.target.clone(),
        }
    }

    pub fn with_base(&self, base: &RelPath) -> Self {
        Self {
            base: base.to_path_buf(),
            root: self.root.clone(),
            target: self.target.clone(),
        }
    }

    pub fn with_extension<S: AsRef<str>>(&self, extension: S) -> Self {
        let extension: &str = extension.as_ref();
        let mut target = self.target.clone();
        target.set_extension(extension);
        Self {
            base: self.base.clone(),
            root: self.root.clone(),
            target,
        }
    }

    pub fn with_file_name<S: AsRef<str>>(&self, name: S) -> Self {
        let name: &str = name.as_ref();
        let mut target = self.target.clone();
        target.set_file_name(name);
        Self {
            base: self.base.clone(),
            root: self.root.clone(),
            target,
        }
    }

    pub fn without_file_name(&self) -> Self {
        self.pop()
    }

    pub fn file_name(&self) -> &OsStr {
        debug_assert!(self.target.file_name().is_some());
        self.target.file_name().unwrap()
    }

    pub fn pop(&self) -> Self {
        let mut target = self.target.clone();
        target.pop();
        Self {
            base: self.base.clone(),
            root: self.root.clone(),
            target,
        }
    }

    pub fn push(&self, path: &RelPath) -> Self {
        let mut target = self.target.clone();
        target.push(path);
        Self {
            base: self.base.clone(),
            root: self.root.clone(),
            target,
        }
    }

    pub fn to_relative_path(&self) -> RelPath {
        let mut buf = PathBuf::from(&self.base);
        buf.push(&self.target);
        // already relative
        RelPath::new(buf).unwrap()
    }

    pub fn to_absolute_path(&self) -> AbsPath {
        let mut buf = self.root().as_path_buf().clone();
        buf.push(&self.base);
        buf.push(&self.target);
        // already absolute
        AbsPath::new(buf).unwrap()
    }

    pub fn base(&self) -> RelPath {
        // already relative
        RelPath::new(&self.base).unwrap()
    }

    pub fn root(&self) -> AbsPath {
        // already absolute
        AbsPath::new(&self.root).unwrap()
    }

    pub fn target(&self) -> &Path {
        self.target.as_path()
    }

    pub fn exists(&self) -> bool {
        self.to_absolute_path().exists()
    }

    pub fn is_dir(&self) -> bool {
        self.to_absolute_path().is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.to_absolute_path().is_file()
    }

    pub fn display(&self) -> String {
        self.to_string()
    }

    pub fn extension(&self) -> Option<&OsStr> {
        self.target.extension()
    }

    pub fn from_abs_path(abs_path: &AbsPath, root: &AbsPath, base: &RelPath) -> Result<Self> {
        let target = abs_path
            .strip_prefix(root)
            .wrap_err("Failed stripping root prefix from AbsPath while creating SysPath")?
            .strip_prefix(base)
            .wrap_err("Failed stripping base prefix from AbsPath while creating SysPath")?;
        Ok(Self::new(root, base, &target))
    }

    pub fn to_typed_path<T: PathMarker>(&self, marker: T) -> TypedPath<T> {
        TypedPath::new(self, marker)
    }

    pub fn to_confirmed_path<T: PathMarker>(&self, marker: T) -> Result<ConfirmedPath<T>> {
        self.to_typed_path(marker).confirm()
    }
}

impl fmt::Display for SysPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_absolute_path().display(), f)
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use super::*;
    use crate::test::{abs, rel};
    use temptree::temptree;

    #[test]
    fn makes_new() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        assert_eq!(sys_path.to_string(), "/1/2/3");
    }

    #[test]
    fn with_base() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let sys_path = sys_path.with_base(rel!("a"));
        assert_eq!(sys_path.to_string(), "/1/a/3");
    }

    #[test]
    fn with_extension() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let sys_path = sys_path.with_extension("ext");
        assert_eq!(sys_path.to_string(), "/1/2/3.ext");
    }

    #[test]
    fn with_file_name() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let sys_path = sys_path.with_file_name("test");
        assert_eq!(sys_path.to_string(), "/1/2/test");
    }

    #[test]
    fn without_file_name() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let sys_path = sys_path.without_file_name();
        assert_eq!(sys_path.to_string(), "/1/2/");
    }

    #[test]
    fn file_name() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let name = sys_path.file_name();
        assert_eq!(name, OsStr::new("3"));
    }

    #[test]
    fn pop() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let sys_path = sys_path.pop();
        assert_eq!(sys_path.to_string(), "/1/2/");
    }

    #[test]
    fn to_relative_path() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let rel_path = sys_path.to_relative_path();
        assert_eq!(rel_path.to_string(), "2/3");
    }

    #[test]
    fn to_absolute_path() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let abs_path = sys_path.to_absolute_path();
        assert_eq!(abs_path.to_string(), "/1/2/3");
    }

    #[test]
    fn base() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let base = sys_path.base();
        assert_eq!(base.to_string(), "2");
    }

    #[test]
    fn root() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let root = sys_path.root();
        assert_eq!(root.to_string(), "/1");
    }

    #[test]
    fn target() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        let target = sys_path.target();
        assert_eq!(target.display().to_string(), "3");
    }

    #[test]
    fn exists() {
        let tree = temptree! {
            "test": {},
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        assert!(sys_path.exists());
    }

    #[test]
    fn exists_fails_when_path_is_missing() {
        let tree = temptree! {
            "test": {},
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("missing"), rel!(""));
        assert!(!sys_path.exists());
    }

    #[test]
    fn is_dir() {
        let tree = temptree! {
            "test": {},
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        assert!(sys_path.is_dir());
    }

    #[test]
    fn is_dir_fails_when_not_a_dir() {
        let tree = temptree! {
            "test": "",
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        assert!(!sys_path.is_dir());
    }

    #[test]
    fn is_dir_fails_when_missing() {
        let tree = temptree! {
            "test": "",
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("missing"), rel!(""));
        assert!(!sys_path.is_dir());
    }

    #[test]
    fn is_file() {
        let tree = temptree! {
            "test": "",
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test"));
        assert!(sys_path.is_file());
    }

    #[test]
    fn is_file_fails_when_not_a_file() {
        let tree = temptree! {
            "test": {}
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        assert!(!sys_path.is_file());
    }

    #[test]
    fn is_file_fails_when_missing() {
        let tree = temptree! {
            "test": "",
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("missing"), rel!(""));
        assert!(!sys_path.is_file());
    }

    #[test]
    fn display() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        assert_eq!(sys_path.to_string(), "/1/2/3");
    }

    #[test]
    fn extension() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3.ext"));
        assert_eq!(sys_path.extension(), Some(OsStr::new("ext")));
    }

    #[test]
    fn extension_returns_none_when_no_extension_persent() {
        let sys_path = SysPath::new(abs!("/1"), rel!("2"), rel!("3"));
        assert!(sys_path.extension().is_none());
    }

    #[test]
    fn from_abs_path() {
        let path = abs!("/1/2/3/file.ext");
        let sys_path = SysPath::from_abs_path(path, abs!("/1/2"), rel!("3"))
            .expect("failed to make sys path from abs path");
        assert_eq!(sys_path.file_name(), OsStr::new("file.ext"));
    }

    #[test]
    fn from_abs_path_fails_with_root_mismatch() {
        let path = abs!("/1/2/3/file.ext");
        let sys_path = SysPath::from_abs_path(path, abs!("/root/1/2"), rel!("3"));
        assert!(sys_path.is_err());
    }

    #[test]
    fn from_abs_path_fails_with_base_mismatch() {
        let path = abs!("/1/2/3/file.ext");
        let sys_path = SysPath::from_abs_path(path, abs!("/1/2"), rel!("missing"));
        assert!(sys_path.is_err());
    }
}

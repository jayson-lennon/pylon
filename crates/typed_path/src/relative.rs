use derivative::Derivative;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::Result;
use eyre::{eyre, WrapErr};
use serde::Serialize;

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct RelPath(pub(in crate) PathBuf);

impl RelPath {
    pub fn new<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let path = path.into();
        if path.has_root() {
            return Err(eyre!(
                "relative path must not have a root component: '{}'",
                path.display()
            ));
        }
        Ok(Self(path))
    }

    pub fn from_relative<P: Into<PathBuf>>(path: P) -> Self {
        let path = path.into();
        assert!(!path.has_root());
        Self(path)
    }

    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    pub fn strip_prefix<P: AsRef<Path>>(&self, base: P) -> Result<Self> {
        let base = base.as_ref();
        self
            .0
            .strip_prefix(base)
            .map(|path| Self(path.to_path_buf())).wrap_err_with(||format!("Failed stripping prefix from rel path. Base '{}' doesn't exist on rel path '{}'", base.display(), self.0.display()))
    }

    pub fn display(&self) -> std::path::Display {
        self.0.display()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.into()
    }

    pub fn as_path_buf(&self) -> &PathBuf {
        self.into()
    }

    pub fn file_name(&self) -> Option<&OsStr> {
        self.0.file_name()
    }

    pub fn extension(&self) -> Option<&OsStr> {
        self.0.extension()
    }

    pub fn starts_with<P: AsRef<Path>>(&self, base: P) -> bool {
        self.0.starts_with(base)
    }

    #[must_use]
    pub fn join(&self, path: &RelPath) -> Self {
        RelPath(self.0.join(path.0.clone()))
    }
}

impl Eq for RelPath {}

impl From<&RelPath> for PathBuf {
    fn from(path: &RelPath) -> Self {
        path.0.clone()
    }
}

impl From<RelPath> for PathBuf {
    fn from(path: RelPath) -> Self {
        path.0
    }
}

impl<'a> From<&'a RelPath> for &'a PathBuf {
    fn from(path: &'a RelPath) -> Self {
        &path.0
    }
}

impl AsRef<Path> for RelPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

crate::helper::impl_try_from!(OsString => RelPath);
crate::helper::impl_try_from!(&OsStr => RelPath);
crate::helper::impl_try_from!(&str => RelPath);
crate::helper::impl_try_from!(String => RelPath);
crate::helper::impl_try_from!(&String => RelPath);
crate::helper::impl_try_from!(&Path => RelPath);
crate::helper::impl_try_from!(PathBuf => RelPath);
crate::helper::impl_try_from!(&PathBuf => RelPath);

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.display(), f)
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use super::*;
    use crate::test::rel;

    #[test]
    fn makes_new_from_rel_path() {
        let path =
            RelPath::new("test").expect("should be able to make an RelPath with an relative path");
    }

    #[test]
    fn make_new_fails_with_absolute_path() {
        let path = RelPath::new("/test");
        assert!(path.is_err());
    }

    #[test]
    fn as_path() {
        let rel = RelPath::new("dir1/dir2").unwrap();
        let path = rel.as_path();
        assert_eq!(path, PathBuf::from("dir1/dir2").as_path());
    }

    #[test]
    fn strips_prefix() {
        let rel = RelPath::new("dir1/dir2").unwrap();
        let stripped = rel
            .strip_prefix("dir1")
            .expect("should be able to strip prefix when it matches");
        assert_eq!(&stripped, rel!("dir2"));
    }

    #[test]
    fn strips_prefix_fails_when_appropriate() {
        let rel = RelPath::new("dir1/dir2").unwrap();
        let stripped = rel.strip_prefix("test");
        assert!(stripped.is_err());
    }

    #[test]
    fn displays_properly() {
        let rel = RelPath::new("dir1/dir2").unwrap();
        let display = rel.display();
        assert_eq!(display.to_string(), "dir1/dir2");
    }

    #[test]
    fn to_path_buf() {
        let rel = RelPath::new("dir1/dir2").unwrap();
        let buf = rel.to_path_buf();
        assert_eq!(buf, PathBuf::from("dir1/dir2"));
    }

    #[test]
    fn as_path_buf() {
        let rel = RelPath::new("dir1/dir2").unwrap();
        let buf = rel.as_path_buf();
        assert_eq!(buf, &PathBuf::from("dir1/dir2"));
    }

    #[test]
    fn gets_file_name() {
        let rel = RelPath::new("dir1/file").unwrap();
        let file_name = rel.file_name();
        assert_eq!(file_name, Some(OsStr::new("file")));
    }

    #[test]
    fn gets_extension() {
        let rel = RelPath::new("dir1/file.ext").unwrap();
        let ext = rel.extension();
        assert_eq!(ext, Some(OsStr::new("ext")));
    }

    #[test]
    fn starts_with() {
        let rel = RelPath::new("dir1/file.ext").unwrap();
        let starts_with_dir1 = rel.starts_with("dir1");
        assert!(starts_with_dir1);
    }

    #[test]
    fn starts_with_fails_when_doesnt_start_with() {
        let rel = RelPath::new("dir1/file.ext").unwrap();
        let starts_with_dir1 = rel.starts_with("test");
        assert!(!starts_with_dir1);
    }

    #[test]
    fn joins() {
        let rel = RelPath::new("dir1/file.ext").unwrap();
        let joined = rel.join(rel!("test"));
        assert_eq!(joined.to_path_buf(), PathBuf::from("dir1/file.ext/test"));
    }
}

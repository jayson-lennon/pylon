use derivative::Derivative;
use std::ffi::OsStr;

use std::fmt;
use std::path::{Path, PathBuf};

use crate::RelPath;
use crate::Result;
use eyre::{eyre, WrapErr};
use serde::Serialize;

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct AbsPath(pub(in crate) PathBuf);

impl AbsPath {
    pub fn new<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let path = path.into();
        if !path.has_root() {
            return Err(eyre!(
                "absolute path must have a root component: '{}'",
                path.display()
            ));
        }
        Ok(Self(path))
    }

    pub fn from_absolute<P: Into<PathBuf>>(path: P) -> Self {
        let path = path.into();
        assert!(path.has_root());
        Self(path)
    }

    pub fn to_relative(&self, base: &AbsPath) -> Result<RelPath> {
        let relative = self.0.strip_prefix(base.as_path()).wrap_err_with(||format!("Failed converting abs path to rel path. Base '{}' doesn't exist on absolute path '{}'", base, self.0.display()))?;
        RelPath::new(relative.to_path_buf())
    }

    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    pub fn strip_prefix<P: AsRef<Path>>(&self, base: P) -> Result<RelPath> {
        let base = base.as_ref();
        let stripped = self.0.strip_prefix(base).wrap_err_with(||format!("Failed stripping prefix from abs path. Base '{}' doesn't exist on absolute path '{}'", base.display(), self.0.display()))?;
        RelPath::new(stripped)
    }

    pub fn display(&self) -> std::path::Display {
        self.0.display()
    }

    pub fn as_path_buf(&self) -> &PathBuf {
        self.into()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.into()
    }

    pub fn file_name(&self) -> Option<&OsStr> {
        self.0.file_name()
    }

    pub fn extension(&self) -> Option<&OsStr> {
        self.0.extension()
    }

    #[must_use]
    pub fn join(&self, path: &RelPath) -> AbsPath {
        AbsPath(self.0.join(path.0.clone()))
    }

    pub fn exists(&self) -> bool {
        self.0.exists()
    }

    pub fn is_dir(&self) -> bool {
        self.0.is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.0.is_file()
    }

    pub fn starts_with<P: AsRef<Path>>(&self, base: P) -> bool {
        self.0.starts_with(base)
    }

    pub fn pop(&self) -> Self {
        let mut buf = self.0.clone();
        buf.pop();
        Self(buf)
    }
}

impl Eq for AbsPath {}

impl From<&AbsPath> for PathBuf {
    fn from(path: &AbsPath) -> Self {
        path.0.clone()
    }
}

impl From<AbsPath> for PathBuf {
    fn from(path: AbsPath) -> Self {
        path.0
    }
}

impl<'a> From<&'a AbsPath> for &'a PathBuf {
    fn from(path: &'a AbsPath) -> Self {
        &path.0
    }
}

impl AsRef<Path> for AbsPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl fmt::Display for AbsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.display(), f)
    }
}

crate::helper::impl_try_from!(&str => AbsPath);
crate::helper::impl_try_from!(String => AbsPath);
crate::helper::impl_try_from!(&String => AbsPath);
crate::helper::impl_try_from!(&Path => AbsPath);
crate::helper::impl_try_from!(PathBuf => AbsPath);
crate::helper::impl_try_from!(&PathBuf => AbsPath);

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use super::*;
    use crate::test::{abs, rel};
    use temptree::temptree;

    #[test]
    fn makes_new_from_abs_path() {
        let path =
            AbsPath::new("/test").expect("should be able to make an AbsPath with an absolute path");
    }

    #[test]
    fn make_new_fails_with_relative_path() {
        let path = AbsPath::new("test");
        assert!(path.is_err());
    }

    #[test]
    fn converts_to_relpath() {
        let abs = AbsPath::new("/test").unwrap();
        let rel = abs
            .to_relative(abs!("/"))
            .expect("should be able to make RelPath from AbsPath");
        assert_eq!(&rel, rel!("test"));
    }

    #[test]
    fn converts_to_relpath_fails_if_prefix_doesnt_exist() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        let rel = abs.to_relative(abs!("/nope"));
        assert!(rel.is_err());
    }

    #[test]
    fn strips_prefix() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        let stripped = abs
            .strip_prefix("/dir1")
            .expect("should strip prefix when it matches");
        assert_eq!(&stripped, rel!("dir2"));
    }

    #[test]
    fn strips_prefix_fails_when_prefix_doesnt_match() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        let stripped = abs.strip_prefix("/test");
        assert!(stripped.is_err());
    }

    #[test]
    fn converts_using_as_path() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        let path = abs.as_path();
        assert_eq!(path, PathBuf::from("/dir1/dir2").as_path());
    }

    #[test]
    fn displays_properly() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        let display = abs.display();
        assert_eq!(display.to_string(), "/dir1/dir2");
    }

    #[test]
    fn as_path_buf() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        let buf = abs.as_path_buf();
        assert_eq!(buf, &PathBuf::from("/dir1/dir2"));
    }

    #[test]
    fn to_path_buf() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        let buf = abs.to_path_buf();
        assert_eq!(buf, PathBuf::from("/dir1/dir2"));
    }

    #[test]
    fn gets_file_name() {
        let abs = AbsPath::new("/dir1/file").unwrap();
        let file_name = abs.file_name();
        assert_eq!(file_name, Some(OsStr::new("file")));
    }

    #[test]
    fn gets_extension() {
        let abs = AbsPath::new("/dir1/file.ext").unwrap();
        let ext = abs.extension();
        assert_eq!(ext, Some(OsStr::new("ext")));
    }

    #[test]
    fn joins() {
        let abs = AbsPath::new("/dir1/file.ext").unwrap();
        let joined = abs.join(rel!("test"));
        assert_eq!(joined.to_path_buf(), PathBuf::from("/dir1/file.ext/test"));
    }

    #[test]
    fn exists() {
        let tree = temptree! {
          "test": "",
        };
        let abs_path = tree.path().join("test");
        let abs = AbsPath::new(abs_path).unwrap();
        assert!(abs.exists());
    }

    #[test]
    fn exists_files_if_missing_file() {
        let abs = AbsPath::new("/this_should_never_exist").unwrap();
        assert!(!abs.exists());
    }

    #[test]
    fn is_dir() {
        let tree = temptree! {
          test: {},
        };
        let abs_path = tree.path().join("test");
        let abs = AbsPath::new(abs_path).unwrap();
        assert!(abs.is_dir());
    }

    #[test]
    fn is_dir_fails_if_not_a_dir() {
        let tree = temptree! {
          test: "",
        };
        let abs_path = tree.path().join("test");
        let abs = AbsPath::new(abs_path).unwrap();
        assert!(!abs.is_dir());
    }

    #[test]
    fn is_file() {
        let tree = temptree! {
          test: "",
        };
        let abs_path = tree.path().join("test");
        let abs = AbsPath::new(abs_path).unwrap();
        assert!(abs.is_file());
    }

    #[test]
    fn is_file_fails_if_not_a_file() {
        let tree = temptree! {
          test: {},
        };
        let abs_path = tree.path().join("test");
        let abs = AbsPath::new(abs_path).unwrap();
        assert!(!abs.is_file());
    }

    #[test]
    fn starts_with() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        assert!(abs.starts_with("/dir1"));
    }

    #[test]
    fn starts_with_returns_false_when_appropriate() {
        let abs = AbsPath::new("/dir1/dir2").unwrap();
        assert!(!abs.starts_with("dir1"));
    }

    // #[test]
    // fn to_sys_path() {
    //     let (engine_paths, tree) = crate::test::simple_init();
    //     let root = tree.path();
    //     let abs = AbsPath::new(tree.path().join("src/some_file.ext")).unwrap();
    //     let sys_path = abs
    //         .to_sys_path(engine_paths, rel!("src"))
    //         .expect("should be able to make sys path");
    //     assert_eq!(
    //         sys_path.to_string(),
    //         tree.path().join("src/some_file.ext").display().to_string()
    //     );
    // }
}

use derivative::Derivative;

use crate::Result;
use crate::{RelPath, SysPath};
use eyre::eyre;
use serde::Serialize;
use std::fmt;

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct CheckedDirPath<T> {
    inner: SysPath,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> CheckedDirPath<T> {
    pub fn new(sys_path: &SysPath) -> Result<Self> {
        if sys_path.is_dir() {
            Ok(Self {
                inner: sys_path.clone(),
                _phantom: std::marker::PhantomData,
            })
        } else {
            Err(eyre!("path is not a directory"))
        }
    }

    pub fn as_sys_path(&self) -> &SysPath {
        &self.inner
    }

    pub fn base(&self) -> RelPath {
        self.inner.base()
    }
}

impl<T> Eq for CheckedDirPath<T> {}

impl<T> TryFrom<SysPath> for CheckedDirPath<T> {
    type Error = eyre::Report;
    fn try_from(path: SysPath) -> Result<Self> {
        Self::new(&path)
    }
}

impl<T> TryFrom<&SysPath> for CheckedDirPath<T> {
    type Error = eyre::Report;
    fn try_from(path: &SysPath) -> Result<Self> {
        Self::new(path)
    }
}

impl<T> fmt::Display for CheckedDirPath<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner.display(), f)
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use super::*;
    use crate::pathmarker;
    use crate::{test::rel, AbsPath};
    use temptree::temptree;

    #[test]
    fn new() {
        let tree = temptree! {
            "test": {}
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        let dir_path = CheckedDirPath::<pathmarker::Any>::new(&sys_path)
            .expect("should be able to make checked dir path when dir exists");
    }

    #[test]
    fn base() {
        let tree = temptree! {
            "test": {}
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        let dir_path = CheckedDirPath::<pathmarker::Any>::new(&sys_path).unwrap();
        assert_eq!(dir_path.base(), sys_path.base());
    }

    #[test]
    fn as_sys_path() {
        let tree = temptree! {
            "test": {}
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        let dir_path = CheckedDirPath::<pathmarker::Any>::new(&sys_path).unwrap();
        assert_eq!(dir_path.as_sys_path(), &sys_path);
    }

    #[test]
    fn new_tryfrom_sys_path() {
        let tree = temptree! {
            "test": {}
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        let dir_path = CheckedDirPath::<pathmarker::Any>::try_from(sys_path)
            .expect("should be able to make checked dir path when dir exists");
    }

    #[test]
    fn new_tryfrom_sys_path_ref() {
        let tree = temptree! {
            "test": {}
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("test"), rel!(""));
        let dir_path = CheckedDirPath::<pathmarker::Any>::try_from(&sys_path)
            .expect("should be able to make checked dir path when dir exists");
    }
}

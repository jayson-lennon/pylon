use derivative::Derivative;

use std::fmt;

use crate::Result;
use crate::SysPath;
use anyhow::anyhow;
use serde::Serialize;

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct CheckedFilePath<T> {
    inner: SysPath,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> CheckedFilePath<T> {
    pub(in crate) fn new(sys_path: &SysPath) -> Result<Self> {
        if sys_path.is_file() {
            Ok(Self {
                inner: sys_path.clone(),
                _phantom: std::marker::PhantomData,
            })
        } else {
            Err(anyhow!("relative path is not a file"))
        }
    }

    pub fn as_sys_path(&self) -> &SysPath {
        &self.inner
    }

    pub fn display(&self) -> String {
        self.to_string()
    }
}

impl<T> Eq for CheckedFilePath<T> {}

impl<T> TryFrom<SysPath> for CheckedFilePath<T> {
    type Error = anyhow::Error;
    fn try_from(path: SysPath) -> Result<Self> {
        Self::new(&path)
    }
}

impl<T> TryFrom<&SysPath> for CheckedFilePath<T> {
    type Error = anyhow::Error;
    fn try_from(path: &SysPath) -> Result<Self> {
        Self::new(path)
    }
}

impl<T> fmt::Display for CheckedFilePath<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner.display(), f)
    }
}

#[cfg(test)]
mod test {
    #![allow(unused_variables)]

    use super::*;
    use crate::{pathmarker, test::rel, AbsPath};
    use temptree::temptree;

    #[test]
    fn new() {
        let tree = temptree! {
            "test": ""
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test"));
        let file_path = CheckedFilePath::<pathmarker::Any>::new(&sys_path)
            .expect("should be able to make checked file path when file exists");
    }

    #[test]
    fn new_tryfrom_sys_path() {
        let tree = temptree! {
            "test": ""
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test"));
        let file_path = CheckedFilePath::<pathmarker::Any>::try_from(sys_path)
            .expect("should be able to make checked file path when file exists");
    }

    #[test]
    fn new_tryfrom_sys_path_ref() {
        let tree = temptree! {
            "test": ""
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test"));
        let file_path = CheckedFilePath::<pathmarker::Any>::try_from(&sys_path)
            .expect("should be able to make checked file path when file exists");
    }
}

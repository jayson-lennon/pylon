use crate::{Result, TypedPath};
use serde::Serialize;
use std::fmt;
use std::hash::Hash;
use std::path::Path;

pub trait PathMarker: Copy + Clone + Serialize + PartialEq + Hash {
    fn confirm_typed<T: PathMarker>(&self, path: &TypedPath<T>) -> Result<bool> {
        self.confirm(path.as_sys_path().to_absolute_path().as_path())
    }
    fn confirm(&self, path: &Path) -> Result<bool>;
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Serialize)]
pub struct File;
impl PathMarker for File {
    fn confirm(&self, path: &Path) -> Result<bool> {
        if path.is_file() {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "File")
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Serialize)]
pub struct Dir;
impl PathMarker for Dir {
    fn confirm(&self, path: &Path) -> Result<bool> {
        if path.is_dir() {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl fmt::Display for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dir")
    }
}

#[cfg(test)]
mod test {

    use crate::test::rel;
    use crate::{AbsPath, SysPath, TypedPath};
    use temptree::temptree;

    use crate::marker;

    #[test]
    fn file_path_marker() {
        let tree = temptree! {
            "test_file": ""
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test_file"));
        let file = TypedPath::new(&sys_path, marker::File);
        file.confirm()
            .expect("should be able to confirm that a file exists");
    }

    #[test]
    fn file_path_marker_fails_when_not_a_file() {
        let tree = temptree! {
            test_dir: {}
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test_dir"));
        let file = TypedPath::new(&sys_path, marker::File);
        let confirmed = file.confirm();
        assert!(confirmed.is_err())
    }

    #[test]
    fn file_path_marker_fails_when_doesnt_exist() {
        let tree = temptree! {};
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("not_found"));
        let file = TypedPath::new(&sys_path, marker::File);
        let confirmed = file.confirm();
        assert!(confirmed.is_err())
    }

    #[test]
    fn dir_path_marker() {
        let tree = temptree! {
            test: {}
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test"));
        let dir = TypedPath::new(&sys_path, marker::Dir);
        dir.confirm()
            .expect("should be able to confirm that a dir exists");
    }

    #[test]
    fn dir_path_marker_fails_when_not_a_dir() {
        let tree = temptree! {
            "test_file": "",
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test_file"));
        let dir = TypedPath::new(&sys_path, marker::Dir);
        let confirmed = dir.confirm();
        assert!(confirmed.is_err())
    }

    #[test]
    fn dir_path_marker_fails_when_doesnt_exist() {
        let tree = temptree! {};
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("not_found"));
        let dir = TypedPath::new(&sys_path, marker::Dir);
        let confirmed = dir.confirm();
        assert!(confirmed.is_err())
    }
}

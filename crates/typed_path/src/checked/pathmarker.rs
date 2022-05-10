use eyre::eyre;
use std::ffi::OsStr;

use crate::{CheckedFile, CheckedFilePath, Result, SysPath};

pub struct Any;
pub struct Html;
pub struct Md;

impl CheckedFile<Any> for SysPath {
    fn to_checked_file(&self) -> Result<CheckedFilePath<Any>> {
        self.try_into()
    }
}

impl CheckedFile<Html> for SysPath {
    fn to_checked_file(&self) -> Result<CheckedFilePath<Html>> {
        if self.extension() == Some(OsStr::new("html")) {
            self.try_into()
        } else {
            Err(eyre!("html files must end in .html"))
        }
    }
}

impl CheckedFile<Md> for SysPath {
    fn to_checked_file(&self) -> Result<CheckedFilePath<Md>> {
        if self.extension() == Some(OsStr::new("md")) {
            self.try_into()
        } else {
            Err(eyre!("md files must end in .md"))
        }
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use super::*;
    use crate::{test::rel, AbsPath};
    use temptree::temptree;

    #[test]
    fn any() {
        let tree = temptree! {
            "base": {
                "1": "",
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("base"), rel!("1"));
        let checked: CheckedFilePath<Any> = sys_path
            .to_checked_file()
            .expect("should be able to make checked file from anything");
    }

    #[test]
    fn html() {
        let tree = temptree! {
            "base": {
                "test.html": "",
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("base"), rel!("test.html"));
        let checked: CheckedFilePath<Html> = sys_path
            .to_checked_file()
            .expect("should be able to make CheckedFile<Html> from file ending in .html");
    }

    #[test]
    fn html_fails_if_wrong_extension() {
        let tree = temptree! {
            "base": {
                "test.ext": "",
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("base"), rel!("test.ext"));
        let checked: Result<CheckedFilePath<Html>> = sys_path.to_checked_file();
        assert!(checked.is_err());
    }

    #[test]
    fn md() {
        let tree = temptree! {
            "base": {
                "test.md": "",
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("base"), rel!("test.md"));
        let checked: CheckedFilePath<Md> = sys_path
            .to_checked_file()
            .expect("should be able to make CheckedFile<Md> from file ending in .md");
    }

    #[test]
    fn md_fails_if_wrong_extension() {
        let tree = temptree! {
            "base": {
                "test.ext": "",
            },
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!("base"), rel!("test.ext"));
        let checked: Result<CheckedFilePath<Md>> = sys_path.to_checked_file();
        assert!(checked.is_err());
    }
}

use serde::Serialize;
use std::ffi::OsStr;
use std::fmt;

use typed_path::{PathMarker, TypedPath};

pub type Result<T> = eyre::Result<T>;

#[derive(Clone, Debug, Hash, Serialize, PartialEq)]
pub struct HtmlFile;
impl PathMarker for HtmlFile {
    fn confirm<T: PathMarker>(&self, path: &TypedPath<T>) -> Result<bool> {
        if path.as_sys_path().is_file()
            && path.as_sys_path().extension() == Some(OsStr::new("html"))
        {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl fmt::Display for HtmlFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HtmlFile")
    }
}

#[derive(Clone, Debug, Hash, Serialize, PartialEq)]
pub struct MdFile;
impl PathMarker for MdFile {
    fn confirm<T: PathMarker>(&self, path: &TypedPath<T>) -> Result<bool> {
        if path.as_sys_path().is_file() && path.as_sys_path().extension() == Some(OsStr::new("md"))
        {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl fmt::Display for MdFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MdFile")
    }
}

#[cfg(test)]
mod test {

    use temptree::temptree;
    use typed_path::{AbsPath, SysPath};

    use super::*;

    #[allow(warnings, unused)]
    macro_rules! abs {
        ($path:literal) => {{
            &typed_path::AbsPath::new($path).unwrap()
        }};
        ($path:expr) => {{
            &typed_path::AbsPath::new($path).unwrap()
        }};
    }

    macro_rules! rel {
        ($path:literal) => {{
            &typed_path::RelPath::new($path).unwrap()
        }};
        ($path:expr) => {{
            &typed_path::RelPath::new($path).unwrap()
        }};
    }

    #[test]
    fn html_file_marker() {
        let tree = temptree! {
            "test.html": ""
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test.html"));
        let file = TypedPath::new(&sys_path, super::HtmlFile);
        file.confirm()
            .expect("should be able to confirm an html file");
    }

    #[test]
    fn html_file_marker_fails_on_non_html_files() {
        let tree = temptree! {
            "test.ext": ""
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test.ext"));
        let file = TypedPath::new(&sys_path, super::HtmlFile);
        let confirmed = file.confirm();
        assert!(confirmed.is_err());
    }

    #[test]
    fn md_file_marker() {
        let tree = temptree! {
            "test.md": ""
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test.md"));
        let file = TypedPath::new(&sys_path, super::MdFile);
        file.confirm()
            .expect("should be able to confirm an md file");
    }

    #[test]
    fn md_file_marker_fails_on_non_md_files() {
        let tree = temptree! {
            "test.txt": ""
        };
        let root = AbsPath::new(tree.path()).unwrap();
        let sys_path = SysPath::new(&root, rel!(""), rel!("test.txt"));
        let file = TypedPath::new(&sys_path, super::MdFile);
        let confirmed = file.confirm();
        assert!(confirmed.is_err());
    }
}

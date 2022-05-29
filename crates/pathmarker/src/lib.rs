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

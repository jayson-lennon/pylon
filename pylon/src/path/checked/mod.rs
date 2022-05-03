mod dirpath;
mod filepath;

pub use dirpath::CheckedDirPath;
pub use filepath::CheckedFilePath;

use crate::Result;

pub trait CheckedPath {
    type Marker;
    fn to_checked_file(&self) -> Result<CheckedFilePath<Self::Marker>>;
    fn to_checked_dir(&self) -> Result<CheckedDirPath<Self::Marker>>;
}

pub mod pathmarker {
    pub struct Any;
    pub struct Html;
    pub struct Md;
}

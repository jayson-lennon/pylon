mod dirpath;
mod filepath;

pub use dirpath::CheckedDirPath;
pub use filepath::CheckedFilePath;

pub mod pathmarker;

use crate::Result;

pub trait CheckedFile<T> {
    fn to_checked_file(&self) -> Result<CheckedFilePath<T>>;
}

pub trait CheckedDir<T> {
    fn to_checked_dir(&self) -> Result<CheckedDirPath<T>>;
}

use std::path::{Path, PathBuf};

use crate::Result;
use anyhow::{anyhow, Context};

pub struct SysPath {
    base: PathBuf,
    target: PathBuf,
}

impl SysPath {
    pub fn new<B, P>(base: B, target: P) -> Result<Self>
    where
        B: AsRef<Path>,
        P: AsRef<Path>,
    {
        let base = base.as_ref();
        let target = target.as_ref();

        let target = base.strip_prefix(&target).with_context(|| {
            format!(
                "unable to create a sys path when base '{}' is not present in file path '{}'",
                base.display(),
                target.display()
            )
        })?;

        Ok(Self {
            base: base.to_path_buf(),
            target: target.to_path_buf(),
        })
    }
}

pub struct MarkdownPath {
    path: SysPath,
}

impl MarkdownPath {
    pub fn new<R, P>(src_root: R, file_path: P) -> Result<Self>
    where
        R: AsRef<Path>,
        P: AsRef<Path>,
    {
        let file_path = file_path.as_ref();

        if file_path.file_name().is_none() {
            return Err(anyhow!(
                "no file name present in file path: {}",
                file_path.display()
            ));
        }

        Ok(Self {
            path: SysPath::new(src_root, file_path)?,
        })
    }
}

impl TryFrom<HtmlPath> for MarkdownPath {
    type Error = anyhow::Error;
    fn try_from(path: HtmlPath) -> Result<Self> {
        todo!()
    }
}

pub struct HtmlPath {
    path: SysPath,
}

// impl From<MarkdownPath> for HtmlPath {
//     fn from(path: MarkdownPath) -> Self {}
// }

pub struct RelativeUri;

pub struct AbsoluteUri;

pub struct WorkingDir;

pub struct AssetTargetPath;

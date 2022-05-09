use derivative::Derivative;
use typed_uri::CheckedUri;

use crate::core::engine::GlobalEnginePaths;
use crate::{pathmarker, Result};
use crate::{AbsPath, CheckedFilePath, RelPath};

use serde::Serialize;

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct AssetPath {
    target: AbsPath,
    uri: CheckedUri,
}

impl AssetPath {
    pub fn new(engine_paths: GlobalEnginePaths, uri: &CheckedUri) -> Result<Self> {
        let target = RelPath::new(&uri.as_str()[1..])?;
        let target = engine_paths.absolute_output_dir().join(&target);

        Ok(Self {
            target,
            uri: uri.clone(),
        })
    }

    pub fn html_src_file(&self) -> &CheckedFilePath<pathmarker::Html> {
        self.uri.html_src()
    }

    pub fn uri(&self) -> &CheckedUri {
        &self.uri
    }

    pub fn target(&self) -> &AbsPath {
        &self.target
    }
}

impl Eq for AssetPath {}

#[cfg(test)]
mod test {
    #![allow(unused_variables)]
}

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct EngineConfig {
    pub src_root: PathBuf,
    pub output_root: PathBuf,
    pub template_root: PathBuf,
}

impl EngineConfig {
    pub fn new<P: AsRef<Path>>(src_root: P, target_root: P, template_root: P) -> Self {
        Self {
            src_root: src_root.as_ref().to_path_buf(),
            output_root: target_root.as_ref().to_path_buf(),
            template_root: template_root.as_ref().to_path_buf(),
        }
    }
}

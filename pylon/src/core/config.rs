use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct EngineConfig {
    pub src_root: PathBuf,
    pub target_root: PathBuf,
    pub template_root: PathBuf,
    pub rule_script: PathBuf,
}

impl EngineConfig {
    pub fn new<P: AsRef<Path>>(
        src_root: P,
        target_root: P,
        template_root: P,
        rule_script: P,
    ) -> Self {
        Self {
            src_root: src_root.as_ref().to_path_buf(),
            target_root: target_root.as_ref().to_path_buf(),
            template_root: template_root.as_ref().to_path_buf(),
            rule_script: rule_script.as_ref().to_path_buf(),
        }
    }

    pub fn src_root(&self) -> &Path {
        self.src_root.as_path()
    }

    pub fn target_root(&self) -> &Path {
        self.target_root.as_path()
    }
    pub fn tempalte_root(&self) -> &Path {
        self.template_root.as_path()
    }
    pub fn rule_script(&self) -> &Path {
        self.rule_script.as_path()
    }
}

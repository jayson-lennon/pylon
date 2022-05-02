use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct EngineConfig {
    pub rule_script: PathBuf,
    pub src_root: PathBuf,
    pub syntax_theme_root: PathBuf,
    pub target_root: PathBuf,
    pub template_root: PathBuf,
}

impl EngineConfig {
    pub fn rule_script(&self) -> &Path {
        self.rule_script.as_path()
    }
    pub fn src_root(&self) -> &Path {
        self.src_root.as_path()
    }
    pub fn syntax_theme_root(&self) -> &Path {
        self.rule_script.as_path()
    }
    pub fn target_root(&self) -> &Path {
        self.target_root.as_path()
    }
    pub fn template_root(&self) -> &Path {
        self.template_root.as_path()
    }
}

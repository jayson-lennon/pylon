use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FrontMatter {
    pub template_path: Option<String>,
    pub use_file_url: bool,
    pub meta: HashMap<String, serde_json::Value>,
}

impl FrontMatter {
    pub fn script_get_template_path(&mut self) -> String {
        match &self.template_path {
            Some(p) => p.clone(),
            None => format!(""),
        }
    }
}

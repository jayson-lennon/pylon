use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use script::rhai_module;

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

pub mod script {
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::frontmatter::FrontMatter;
        use rhai::serde::to_dynamic;

        #[rhai_fn(get = "template_path")]
        pub fn template_path(frontmatter: &mut FrontMatter) -> String {
            frontmatter
                .template_path
                .clone()
                .unwrap_or_else(|| "".into())
        }

        #[rhai_fn(get = "use_file_url")]
        pub fn use_file_url(frontmatter: &mut FrontMatter) -> bool {
            frontmatter.use_file_url
        }

        /// Returns all attached metadata.
        #[rhai_fn(get = "meta", return_raw)]
        pub fn all_meta(
            frontmatter: &mut FrontMatter,
        ) -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            to_dynamic(frontmatter.meta.clone())
        }

        /// Returns the value found at the provided key. Returns `()` if the key wasn't found.
        #[rhai_fn(name = "meta")]
        pub fn get_meta(frontmatter: &mut FrontMatter, key: &str) -> rhai::Dynamic {
            frontmatter
                .meta
                .get(key)
                .map(|v| to_dynamic(v).ok())
                .flatten()
                .unwrap_or_default()
        }
    }
}

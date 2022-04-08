use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use script::rhai_module;

use crate::render::template::TemplateName;

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FrontMatter {
    pub template_name: Option<TemplateName>,

    #[serde(default = "default_true")]
    pub use_index: bool,

    pub meta: HashMap<String, serde_json::Value>,
}

pub mod script {
    #[allow(clippy::wildcard_imports)]
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::core::page::FrontMatter;
        use crate::render::template::TemplateName;
        use rhai::serde::to_dynamic;

        #[rhai_fn(get = "template_name")]
        pub fn template_name(frontmatter: &mut FrontMatter) -> String {
            frontmatter
                .template_name
                .clone()
                .map_or_else(|| "".into(), TemplateName::into_string)
        }

        #[rhai_fn(get = "use_index")]
        pub fn use_index(frontmatter: &mut FrontMatter) -> bool {
            frontmatter.use_index
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
                .and_then(|v| to_dynamic(v).ok())
                .unwrap_or_default()
        }
    }
}

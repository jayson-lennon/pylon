use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use script::rhai_module;

use crate::render::template::TemplateName;

fn always_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FrontMatter {
    pub template_name: Option<TemplateName>,
    pub keywords: Vec<String>,

    pub use_breadcrumbs: bool,

    #[serde(default = "always_true")]
    pub published: bool,

    #[serde(default = "always_true")]
    pub searchable: bool,

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

    #[cfg(test)]
    mod test {

        #![allow(warnings, unused)]
        use std::collections::HashMap;

        use crate::core::page::FrontMatter;

        use super::*;

        #[test]
        fn get_template_name_is_present() {
            let mut frontmatter = FrontMatter {
                template_name: Some("test.tera".into()),
                ..Default::default()
            };

            let name = rhai_module::template_name(&mut frontmatter);
            assert_eq!(name.as_str(), "test.tera");
        }

        #[test]
        fn get_template_name_is_missing() {
            let mut frontmatter = FrontMatter::default();

            let name = rhai_module::template_name(&mut frontmatter);
            assert_eq!(name.as_str(), "");
        }

        #[test]
        pub fn get_all_meta() {
            let mut frontmatter = FrontMatter::default();

            let dynamic = rhai_module::all_meta(&mut frontmatter);
            assert!(dynamic.is_ok());

            assert_eq!(dynamic.unwrap().type_name(), "map");
        }

        #[test]
        fn get_existing_meta_item() {
            let mut meta = HashMap::new();
            meta.insert("test".into(), serde_json::to_value("sample").unwrap());

            let mut frontmatter = FrontMatter {
                meta,
                ..FrontMatter::default()
            };

            let meta = rhai_module::get_meta(&mut frontmatter, "test");
            assert_eq!(meta.into_string().unwrap().as_str(), "sample");
        }

        #[test]
        fn get_nonexistent_meta_item() {
            let mut meta = HashMap::new();
            meta.insert("test".into(), serde_json::to_value("sample").unwrap());

            let mut frontmatter = FrontMatter {
                meta,
                ..FrontMatter::default()
            };

            let meta = rhai_module::get_meta(&mut frontmatter, "nope");
            assert_eq!(meta.type_name(), "()");
        }
    }
}

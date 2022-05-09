pub mod frontmatter;
pub mod lint;
pub mod page;
pub mod render;

pub use frontmatter::FrontMatter;
pub use lint::{lint, LintLevel, LintResult};
pub use page::Page;
pub use render::{render, RenderedPage, RenderedPageCollection};
use serde::{Deserialize, Serialize};

slotmap::new_key_type! {
    pub struct PageKey;
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextItem {
    pub identifier: String,
    pub data: serde_json::Value,
}

impl ContextItem {
    pub fn new<S: AsRef<str>>(identifier: S, data: serde_json::Value) -> Self {
        Self {
            identifier: identifier.as_ref().to_string(),
            data,
        }
    }
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct RawMarkdown(String);

impl AsRef<str> for RawMarkdown {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

pub mod script {
    #[allow(clippy::wildcard_imports)]
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::core::page::{ContextItem, FrontMatter, Page};

        use tracing::instrument;

        #[rhai_fn(name = "uri")]
        pub fn uri(page: &mut Page) -> String {
            page.uri().to_string()
        }

        #[rhai_fn(get = "frontmatter")]
        pub fn frontmatter(page: &mut Page) -> FrontMatter {
            page.frontmatter.clone()
        }

        /// Returns all attached metadata.
        #[rhai_fn(get = "meta", return_raw)]
        pub fn all_meta(page: &mut Page) -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            crate::core::page::frontmatter::script::rhai_module::all_meta(&mut page.frontmatter)
        }

        /// Returns the value found at the provided key. Returns `()` if the key wasn't found.
        #[rhai_fn()]
        pub fn meta(page: &mut Page, key: &str) -> rhai::Dynamic {
            crate::core::page::frontmatter::script::rhai_module::get_meta(
                &mut page.frontmatter,
                key,
            )
        }

        /// Generates a new context for use within the page template.
        #[instrument(ret)]
        #[rhai_fn(return_raw)]
        pub fn new_context(map: rhai::Map) -> Result<Vec<ContextItem>, Box<EvalAltResult>> {
            let mut context_items = vec![];
            for (k, v) in map {
                let value: serde_json::Value = rhai::serde::from_dynamic(&v)?;
                let item = ContextItem::new(k, value);
                context_items.push(item);
            }
            Ok(context_items)
        }

        #[cfg(test)]
        mod test {
            use super::rhai_module;
            use crate::core::page::page::test::{doc::MINIMAL, new_page};

            #[test]
            fn uri_fn() {
                let mut page = new_page(MINIMAL, "src/test.md").unwrap();
                let uri = rhai_module::uri(&mut page);
                assert_eq!(uri, String::from("/test.html"));
            }

            #[test]
            fn get_frontmatter() {
                let mut page = new_page(MINIMAL, "src/test.md").unwrap();
                let frontmatter = rhai_module::frontmatter(&mut page);
                assert_eq!(frontmatter.template_name, Some("empty.tera".into()));
            }

            #[test]
            fn get_all_meta() {
                let mut page = new_page(MINIMAL, "src/test.md").unwrap();

                let dynamic = rhai_module::all_meta(&mut page);
                assert!(dynamic.is_ok());

                assert_eq!(dynamic.unwrap().type_name(), "map");
            }

            #[test]
            fn get_existing_meta_item() {
                let mut page = new_page(
                    r#"+++
                template_name = "empty.tera"

                [meta]
                test = "sample"
                +++"#,
                    "src/test.md",
                )
                .unwrap();

                let meta = rhai_module::meta(&mut page, "test");
                assert_eq!(meta.into_string().unwrap().as_str(), "sample");
            }

            #[test]
            fn get_nonexistent_meta_item() {
                let mut page = new_page(
                    r#"+++
                template_name = "empty.tera"

                [meta]
                test = "sample"
                +++"#,
                    "src/test.md",
                )
                .unwrap();

                let meta = rhai_module::meta(&mut page, "nope");
                assert_eq!(meta.type_name(), "()");
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new_context_item() {
        fn value(v: usize) -> serde_json::Value {
            serde_json::to_value(v).unwrap()
        }
        let ctx_item = ContextItem::new("test", value(1));
        assert_eq!(ctx_item.identifier.as_str(), "test");
        assert_eq!(ctx_item.data, value(1));
    }

    #[test]
    fn raw_markdown_as_ref() {
        let markdown = RawMarkdown("test".into());
        assert_eq!(markdown.as_ref(), "test");
    }
}

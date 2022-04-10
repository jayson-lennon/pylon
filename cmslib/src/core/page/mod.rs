pub mod frontmatter;
pub mod lint;
pub mod page;
pub mod render;

pub use frontmatter::FrontMatter;
pub use lint::{lint, LintLevel, LintMsg};
pub use page::Page;
pub use render::{render, RenderedPage, RenderedPageCollection};
use serde::{Deserialize, Serialize};

use self::lint::LintKey;

use super::rules::{GlobStore, RuleProcessor};

#[derive(Clone, Debug)]
pub struct LintProcessor<'a> {
    processor: &'a RuleProcessor,
    lints: &'a GlobStore<LintKey, rhai::FnPtr>,
}

impl<'a> LintProcessor<'a> {
    pub fn new(processor: &'a RuleProcessor, lints: &'a GlobStore<LintKey, rhai::FnPtr>) -> Self {
        Self { processor, lints }
    }
}

slotmap::new_key_type! {
    pub struct PageKey;
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextItem {
    pub identifier: String,
    pub data: serde_json::Value,
}

impl ContextItem {
    #[must_use]
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
        use rhai::serde::to_dynamic;

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
            to_dynamic(page.frontmatter.meta.clone())
        }

        /// Returns the value found at the provided key. Returns `()` if the key wasn't found.
        #[rhai_fn()]
        pub fn meta(page: &mut Page, key: &str) -> rhai::Dynamic {
            page.frontmatter
                .meta
                .get(key)
                .and_then(|v| to_dynamic(v).ok())
                .unwrap_or_default()
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
    }
}

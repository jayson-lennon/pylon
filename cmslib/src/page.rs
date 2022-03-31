use crate::frontmatter::FrontMatter;
use crate::util;
use crate::util::RetargetablePathBuf;
use crate::{CanonicalPath, Renderers};
use anyhow::anyhow;
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tracing::{instrument, trace, trace_span};

slotmap::new_key_type! {
    pub struct PageKey;
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Markdown(String);

#[derive(Clone, Debug, Default, Serialize)]
pub struct Page {
    pub system_path: RetargetablePathBuf,
    pub raw_document: String,
    pub page_key: PageKey,

    pub frontmatter: FrontMatter,
    pub markdown: Markdown,

    pub canonical_path: CanonicalPath,
}

impl Page {
    pub fn new<P: AsRef<Path>>(
        system_path: P,
        system_root: P,
        renderers: &Renderers,
    ) -> Result<Self, anyhow::Error> {
        let system_path = system_path.as_ref();
        let system_root = system_root.as_ref();

        let raw_document = std::fs::read_to_string(system_path)?;

        let (frontmatter, markdown) = {
            let (raw_frontmatter, raw_markdown) = split_document(&raw_document)?;
            let frontmatter: FrontMatter = toml::from_str(raw_frontmatter)?;
            let markdown = Markdown(renderers.markdown.render(raw_markdown));
            (frontmatter, markdown)
        };

        let relative_path = crate::util::strip_root(system_root, system_path);
        let canonical_path = CanonicalPath::new(&relative_path.to_string_lossy());
        let system_path = RetargetablePathBuf::new(system_root, relative_path);

        Ok(Self {
            system_path,
            raw_document,
            frontmatter,
            markdown,
            canonical_path,
            ..Default::default()
        })
    }

    pub fn set_page_key(&mut self, key: PageKey) {
        self.page_key = key;
    }

    #[instrument(skip_all, fields(page=%self.canonical_path.to_string()))]
    pub fn set_template(&mut self, template_paths: &HashSet<&str>) -> Result<(), anyhow::Error> {
        if self.frontmatter.template_path.is_none() {
            let _span = trace_span!("no template specified").entered();
            match get_default_template_path(template_paths, &self.canonical_path) {
                Some(template) => self.frontmatter.template_path = Some(template),
                None => {
                    return Err(anyhow!(
                        "no template provided and unable to find a default template for page {}",
                        self.canonical_path.as_str()
                    ))
                }
            }
        }

        Ok(())
    }

    pub fn canonical_path(&mut self) -> String {
        self.canonical_path.to_string()
    }
}

#[instrument(ret)]
fn get_default_template_path(
    default_template_paths: &HashSet<&str>,
    page_path: &CanonicalPath,
) -> Option<String> {
    // This function chomps the page path until no more components are remaining.
    let page_path = PathBuf::from(page_path.relative());
    let mut ancestors = page_path.ancestors();

    while let Some(path) = ancestors.next() {
        let template_path = {
            // Add the default page name ("page.tera") to the new path.
            let mut template_path = PathBuf::from(path);
            template_path.push("page.tera");
            template_path.to_string_lossy().to_string()
        };

        if default_template_paths.contains(&template_path.as_str()) {
            return Some(template_path);
        }
    }
    None
}

fn split_document(raw: &str) -> Result<(&str, &str), anyhow::Error> {
    let re = crate::util::static_regex!(
        r#"^[[:space:]]*\+\+\+[[:space:]]*\n((?s).*)\n[[:space:]]*\+\+\+[[:space:]]*((?s).*)"#
    );
    match re.captures(raw) {
        Some(captures) => {
            let frontmatter = captures
                .get(1)
                .map(|m| m.as_str())
                .ok_or_else(|| anyhow!("unable to read frontmatter"))?;

            let markdown = captures
                .get(2)
                .map(|m| m.as_str())
                .ok_or_else(|| anyhow!("unable to read markdown"))?;
            Ok((frontmatter, markdown))
        }
        None => Err(anyhow!("improperly formed document"))?,
    }
}

#[derive(Clone, Debug, Serialize, Default, Eq, PartialEq, Hash)]
pub struct LinkedAsset {
    target: PathBuf,
}

impl LinkedAsset {
    pub fn new<P: AsRef<Path>>(asset: P) -> Self {
        Self {
            target: asset.as_ref().to_path_buf(),
        }
    }

    pub fn target(&self) -> &Path {
        self.target.as_path()
    }
}

#[derive(Debug)]
pub struct LinkedAssets {
    assets: HashSet<LinkedAsset>,
}

impl LinkedAssets {
    pub fn new(assets: HashSet<LinkedAsset>) -> Self {
        Self { assets }
    }

    pub fn iter(&self) -> impl Iterator<Item = &LinkedAsset> {
        self.assets.iter()
    }
}

pub mod script {
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::core::rules::gctx::{ContextItem, Generators, Matcher};
        use crate::core::rules::Rules;
        use crate::frontmatter::FrontMatter;
        use crate::page::Page;
        use rhai::serde::to_dynamic;
        use rhai::FnPtr;
        use tracing::{instrument, trace};

        #[rhai_fn(name = "canonical_path")]
        pub fn canonical_path(page: &mut Page) -> String {
            page.canonical_path().to_string()
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
                .map(|v| to_dynamic(v).ok())
                .flatten()
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

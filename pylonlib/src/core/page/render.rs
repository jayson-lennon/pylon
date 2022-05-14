use eyre::{eyre, WrapErr};
use itertools::Itertools;
use std::collections::HashSet;

use tracing::{error, instrument, trace};

use crate::{
    core::{
        engine::Engine,
        page::{ContextItem, PageKey},
        rules::{ContextKey, GlobStore, RuleProcessor},
        Page,
    },
    site_context::SiteContext,
    Result, SysPath,
};

pub fn render(engine: &Engine, page: &Page) -> Result<RenderedPage> {
    trace!("rendering page");

    let site_ctx = SiteContext::new("sample");

    match page.frontmatter.template_name.as_ref() {
        Some(template) => {
            let mut tera_ctx = tera::Context::new();

            // site context (from global site.toml file)
            tera_ctx.insert("site", &site_ctx);

            // entire page store
            // TODO: Come up with some better way to manage this / delete it
            tera_ctx.insert("page_store", {
                &engine.page_store().iter().collect::<Vec<_>>()
            });

            // global context provided by user script
            if let Some(global) = engine.rules().global_context() {
                tera_ctx.insert("global", global);
            }

            // the [meta] section where users can define anything they want
            {
                let meta_ctx = tera::Context::from_serialize(&page.frontmatter.meta)
                    .expect("failed converting page metadata into tera context");
                tera_ctx.extend(meta_ctx);
            }

            // page-specific context items provided by user script
            {
                // the context items
                let user_ctx = {
                    let page_ctxs = engine.rules().page_contexts();
                    build_context(engine.rule_processor(), page_ctxs, page).wrap_err_with(|| {
                        format!("Failed building page context for page {}", page.uri())
                    })?
                };

                // abort if a user script overwrites a pre-defined context item
                {
                    let ids = get_overwritten_identifiers(&user_ctx);
                    if !ids.is_empty() {
                        error!(ids = ?ids, "overwritten system identifiers detected");
                        return Err(eyre!(
                            "cannot overwrite reserved system context identifiers"
                        ));
                    }
                }

                // add the context items
                for ctx in user_ctx {
                    let mut user_ctx = tera::Context::new();
                    user_ctx.insert(ctx.identifier, &ctx.data);
                    tera_ctx.extend(user_ctx);
                }
            }

            // the actual markdown content (rendered)
            {
                let rendered_markdown = engine.renderers().markdown.render(
                    page,
                    engine.page_store(),
                    &engine.renderers().highlight,
                )?;
                tera_ctx.insert("content", &rendered_markdown);
            }

            // render the template with the context
            let renderer = &engine.renderers().tera;
            renderer
                .render(template, &tera_ctx)
                .map(|html| RenderedPage::new(page.page_key, html, &page.target()))
                .map_err(|e| eyre!("{}", e))
        }
        None => Err(eyre!("no template declared for page '{}'", page.uri())),
    }
}

pub fn build_context(
    script_fn_runner: &RuleProcessor,
    page_ctxs: &GlobStore<ContextKey, rhai::FnPtr>,
    for_page: &Page,
) -> Result<Vec<ContextItem>> {
    trace!("building page-specific context");
    let contexts: Vec<Vec<ContextItem>> = page_ctxs
        .find_keys(&for_page.search_keys()[0].as_str())
        .iter()
        .filter_map(|key| page_ctxs.get(*key))
        .map(|ptr| script_fn_runner.run(&ptr, (for_page.clone(),)))
        .try_collect()
        .wrap_err("Failed building ContextItem collection when building page context")?;

    let contexts = contexts.into_iter().flatten().collect::<Vec<_>>();

    let mut identifiers = HashSet::new();
    for ctx in &contexts {
        if !identifiers.insert(ctx.identifier.as_str()) {
            return Err(eyre!(
                "duplicate context identifier encountered in page context generation: {}",
                ctx.identifier.as_str()
            ));
        }
    }

    Ok(contexts)
}

#[derive(Debug)]
pub struct RenderedPage {
    page_key: PageKey,
    html: String,
    target: SysPath,
}

impl RenderedPage {
    pub fn new<S: Into<String> + std::fmt::Debug>(
        page_key: PageKey,
        html: S,
        target: &SysPath,
    ) -> Self {
        Self {
            page_key,
            html: html.into(),
            target: target.clone(),
        }
    }

    pub fn target(&self) -> &SysPath {
        &self.target
    }

    pub fn html(&self) -> &str {
        self.html.as_str()
    }
}

#[derive(Debug)]
pub struct RenderedPageCollection {
    pages: Vec<RenderedPage>,
}

impl RenderedPageCollection {
    pub fn new() -> Self {
        Self { pages: vec![] }
    }

    pub fn push(&mut self, page: RenderedPage) {
        self.pages.push(page);
    }

    pub fn from_iterable<T: Iterator<Item = RenderedPage>>(iterable: T) -> Self {
        Self {
            pages: iterable.collect::<Vec<_>>(),
        }
    }

    pub fn from_vec(pages: Vec<RenderedPage>) -> Self {
        Self { pages }
    }

    pub fn write_to_disk(&self) -> Result<()> {
        use std::fs;
        for page in &self.pages {
            let parent_dir = page.target().without_file_name().to_absolute_path();
            crate::util::make_parent_dirs(&parent_dir).wrap_err_with(||format!("Failed making parent directories at '{}' when writing RenderedPageCollection to disk", parent_dir))?;
            let target = page.target().to_absolute_path();
            fs::write(&target, &page.html)
                .wrap_err_with(|| format!("Failed to write rendered page to '{}'", target))?;
        }

        Ok(())
    }

    pub fn iter(&self) -> std::slice::Iter<'_, RenderedPage> {
        self.pages.iter()
    }
}

impl IntoIterator for RenderedPageCollection {
    type Item = RenderedPage;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.pages.into_iter()
    }
}

impl<'a> IntoIterator for &'a RenderedPageCollection {
    type Item = &'a RenderedPage;
    type IntoIter = std::slice::Iter<'a, RenderedPage>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

fn get_overwritten_identifiers(contexts: &[ContextItem]) -> HashSet<String> {
    let reserved = ["site", "content", "page_store", "global"];
    let mut overwritten_ids = HashSet::new();

    for ctx in contexts.iter() {
        if reserved.contains(&ctx.identifier.as_str()) {
            overwritten_ids.insert(ctx.identifier.clone());
        }
    }

    overwritten_ids
}

#[cfg(test)]
mod test {
    #![allow(warnings, unused)]

    use super::*;
    use crate::core::page::page::test::{doc::MINIMAL, new_page, new_page_with_tree};
    use crate::core::PageStore;
    use crate::test::{abs, rel};
    use std::result::Result;
    use temptree::temptree;

    slotmap::new_key_type! {
        pub struct TestKey;
    }

    pub fn new_rendered_page(content: &str) -> RenderedPage {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {
                "default.tera": "",
                "empty.tera": "",
            },
            target: {
                "test.html": "",
            },
            src: {
                "test.md": "",
            },
            syntax_themes: {},
        };

        let page1 = new_page_with_tree(&tree, &tree.path().join("src/test.md"), MINIMAL).unwrap();

        let mut store = PageStore::new();
        let key1 = store.insert(page1);

        let sys_path = SysPath::new(abs!("/root"), rel!(""), rel!("test.html"));

        RenderedPage::new(key1, content, &sys_path)
    }

    #[test]
    fn rendered_page_collection_new() {
        let collection = RenderedPageCollection::new();
    }

    #[test]
    fn rendered_page_collection_push() {
        let mut collection = RenderedPageCollection::new();
        let page = new_rendered_page("");
        collection.push(page);
        assert_eq!(collection.pages.len(), 1);
    }

    #[test]
    fn rendered_page_collection_iter() {
        let mut collection = RenderedPageCollection::new();
        let page = new_rendered_page("");
        collection.push(page);
        assert_eq!(collection.iter().count(), 1);
    }

    #[test]
    fn rendered_page_collection_into_iter() {
        let mut collection = RenderedPageCollection::new();
        let page = new_rendered_page("");
        collection.push(page);
        assert_eq!(collection.into_iter().count(), 1);
    }

    #[test]
    fn rendered_page_collection_into_iter_ref() {
        let mut collection = RenderedPageCollection::new();
        let page = new_rendered_page("");
        collection.push(page);
        let iter = &collection.into_iter();
    }

    #[test]
    fn get_overwritten_ids() {
        let contexts = vec![ContextItem::new("ok", serde_json::from_str("{}").unwrap())];
        let ids = get_overwritten_identifiers(&contexts);
        assert!(ids.is_empty());
    }

    #[test]
    fn get_overwritten_ids_finds_reserved() {
        let contexts = vec![
            ContextItem::new("site", serde_json::from_str("{}").unwrap()),
            ContextItem::new("content", serde_json::from_str("{}").unwrap()),
            ContextItem::new("page_store", serde_json::from_str("{}").unwrap()),
            ContextItem::new("global", serde_json::from_str("{}").unwrap()),
        ];
        let ids = get_overwritten_identifiers(&contexts);
        assert_eq!(ids.len(), 4);
    }
}

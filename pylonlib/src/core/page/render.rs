mod breadcrumbs;

use eyre::{eyre, WrapErr};
use itertools::Itertools;
use std::collections::HashSet;

use tracing::{debug, error, trace};

use crate::{
    core::{
        engine::Engine,
        page::{ContextItem, PageKey, RawMarkdown},
        rules::{ContextKey, GlobStore, RuleProcessor},
        Page,
    },
    site_context::SiteContext,
    Result, SysPath, USER_LOG,
};

const RESERVED_CONTEXT_KEYWORDS: &[&str] = &[
    "site",
    "content",
    "library",
    "global",
    "page",
    "breadcrumbs",
];

#[allow(clippy::too_many_lines)]
pub fn render(engine: &Engine, page: &Page) -> Result<RenderedPage> {
    debug!(
        target: USER_LOG,
        "rendering doc {}",
        page.path().as_sys_path()
    );

    let site_ctx = SiteContext::new("sample");

    match page.frontmatter.template_name.as_ref() {
        Some(template) => {
            let mut tera_ctx = tera::Context::new();

            // site context (from global site.toml file)
            tera_ctx.insert("site", &site_ctx);

            // library
            let library = {
                let mut library = ctx::Library::new();
                for page in engine.library().iter().map(|(_, page)| page) {
                    library.insert(page);
                }

                tera_ctx.insert("library", &library);
                library
            };

            // current page info
            {
                let mut inner = tera::Context::new();
                inner.insert("path", &page.path().to_string());
                inner.insert("uri", &page.uri().to_string());
                inner.insert("template_name", page.template_name().as_str());
                inner.insert("meta", &page.frontmatter.meta);

                let toc = engine.renderers().markdown().render_toc(page);
                inner.insert("toc", &toc);

                tera_ctx.insert("page", &inner.into_json());
            }

            // global context provided by user script
            if let Some(global) = engine.rules().global_context() {
                tera_ctx.insert("global", global);
            }

            // breadcrumbs
            if page.frontmatter().use_breadcrumbs {
                let crumbs = breadcrumbs::generate(engine.library(), page);
                tera_ctx.insert("breadcrumbs", &crumbs);
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

            // shortcodes
            let raw_markdown = {
                let mut raw_markdown = page.raw_markdown().as_ref().to_string();

                while let Some(code) = crate::discover::shortcode::find_next(&raw_markdown)
                    .wrap_err_with(|| {
                        format!(
                            "Failed locating shortcodes when rendering page {}",
                            page.path()
                        )
                    })?
                {
                    let template_name = format!("shortcodes/{}.tera", code.name());
                    let mut context = tera::Context::new();
                    for (k, v) in code.context() {
                        context.insert(*k, v);
                    }
                    context.insert("library", &library);
                    let rendered_shortcode = engine
                        .renderers()
                        .tera()
                        .render(&template_name.into(), &context)?;

                    // required for https://github.com/rust-lang/rust/issues/59159
                    let range = code.range().clone();

                    raw_markdown.replace_range(range, &rendered_shortcode);
                }

                RawMarkdown::from_raw(raw_markdown)
            };

            // the actual markdown content (rendered)
            {
                let rendered_markdown = engine
                    .renderers()
                    .markdown()
                    .render(
                        page,
                        engine.library(),
                        engine.renderers().highlight(),
                        &raw_markdown,
                    )
                    .wrap_err("Failed rendering Markdown")?;
                tera_ctx.insert("content", &rendered_markdown);
            }

            // render the template with the context
            let renderer = &engine.renderers().tera();
            renderer
                .render(template, &tera_ctx)
                .map(|html| RenderedPage::new(page.page_key, html, &page.target()))
                .wrap_err_with(|| {
                    format!(
                        "Failed to render template '{}' for document '{}'",
                        template,
                        page.path()
                    )
                })
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
    #[allow(dead_code)]
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

impl Default for RenderedPageCollection {
    fn default() -> Self {
        Self::new()
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
    let mut overwritten_ids = HashSet::new();

    for ctx in contexts.iter() {
        if RESERVED_CONTEXT_KEYWORDS.contains(&ctx.identifier.as_str()) {
            overwritten_ids.insert(ctx.identifier.clone());
        }
    }

    overwritten_ids
}

mod ctx {
    use std::collections::BTreeMap;

    use serde::Serialize;

    use crate::core::{self, page::FrontMatter};

    #[derive(Debug, Serialize)]
    pub struct Page<'f> {
        frontmatter: &'f FrontMatter,
        path: String,
        search_key: String,
        uri: String,
    }

    impl<'f> From<&'f core::Page> for Page<'f> {
        fn from(page: &'f core::Page) -> Self {
            let content_dir = page.engine_paths();
            let content_dir = content_dir.src_dir();
            let search_key = {
                let search_key = page
                    .path()
                    .as_sys_path()
                    .with_extension("")
                    .to_relative_path();
                search_key
                    .strip_prefix(content_dir)
                    .unwrap_or_else(|e|
                        panic!("Failed to strip prefix '{}' from '{}' while creating ctx::Page: {}. This is a bug.", content_dir, search_key, e)
                    ).to_string()
            };

            let path = {
                let page_relative_path = page.path().as_sys_path().to_relative_path();

                page_relative_path
                .strip_prefix(content_dir)
                .unwrap_or_else(|e|
                    panic!("Failed to strip prefix '{}' from '{}' while creating ctx::Page: {}. This is a bug.", content_dir, page_relative_path, e)
                ).to_string()
            };

            Self {
                frontmatter: page.frontmatter(),
                path,
                search_key,
                uri: page.uri().to_string(),
            }
        }
    }
    #[derive(Debug, Serialize)]
    #[serde(transparent)]
    pub struct Library<'f> {
        pages: BTreeMap<String, Page<'f>>,
    }

    impl<'f> Library<'f> {
        pub fn new() -> Self {
            Self {
                pages: BTreeMap::new(),
            }
        }

        pub fn insert<P: Into<Page<'f>>>(&mut self, page: P) {
            let page = page.into();
            self.pages.insert(page.search_key.clone(), page);
        }
    }

    impl<'f> Default for Library<'f> {
        fn default() -> Self {
            Self::new()
        }
    }

    #[cfg(test)]
    mod test {
        use crate::core::page::render::ctx;
        use crate::core::page::test_page::new_page;

        #[test]
        fn new_library() {
            let library = ctx::Library::new();
            assert!(library.pages.is_empty());

            let library = ctx::Library::default();
            assert!(library.pages.is_empty());
        }

        #[test]
        fn library_insert() {
            let mut library = ctx::Library::new();
            let page = new_page(
                r#"
+++
+++
sample content"#,
                "doc.md",
            )
            .unwrap();

            library.insert(&page);

            assert!(!library.pages.is_empty());
        }

        #[test]
        fn ctx_page_from_core_page_impl() {
            let page = new_page(
                r#"
+++
+++
sample content"#,
                "doc.md",
            )
            .unwrap();

            let ctx_page = ctx::Page::from(&page);

            assert_eq!(ctx_page.path, "doc.md");
            assert_eq!(ctx_page.search_key, "doc");
            assert_eq!(ctx_page.uri, "/doc.html");
        }
        #[test]
        fn ctx_page_from_core_page_impl_nested_src() {
            let page = new_page(
                r#"
+++
+++
sample content"#,
                "inner/two/doc.md",
            )
            .unwrap();

            let ctx_page = ctx::Page::from(&page);

            assert_eq!(ctx_page.path, "inner/two/doc.md");
            assert_eq!(ctx_page.search_key, "inner/two/doc");
            assert_eq!(ctx_page.uri, "/inner/two/doc.html");
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(warnings, unused)]

    use super::*;
    use crate::core::page::test_page::{doc::MINIMAL, new_page, new_page_with_tree};
    use crate::core::Library;
    use crate::render::highlight::SyntectHighlighter;
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

        let mut store = Library::new();
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
            ContextItem::new("library", serde_json::from_str("{}").unwrap()),
            ContextItem::new("global", serde_json::from_str("{}").unwrap()),
            ContextItem::new("page", serde_json::from_str("{}").unwrap()),
            ContextItem::new("breadcrumbs", serde_json::from_str("{}").unwrap()),
        ];
        assert_eq!(
            RESERVED_CONTEXT_KEYWORDS.len(),
            6,
            "A keyword has been added, but is missing in the test code. Please update this test."
        );

        let ids = get_overwritten_identifiers(&contexts);
        assert_eq!(ids.len(), 6);
    }
}

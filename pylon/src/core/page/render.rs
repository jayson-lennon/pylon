use anyhow::anyhow;
use itertools::Itertools;
use std::{collections::HashSet, path::PathBuf};

use tracing::{error, instrument, trace};

use crate::{
    core::{
        engine::Engine,
        linked_asset::LinkedAsset,
        page::{ContextItem, PageKey},
        rules::{ContextKey, GlobStore, RuleProcessor},
        LinkedAssets, Page, PageStore, RelSystemPath, Uri,
    },
    site_context::SiteContext,
    Result,
};

#[instrument(skip(engine), fields(page=%page.uri()))]
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
                    build_context(engine.rule_processor(), page_ctxs, page)?
                };

                // abort if a user script overwrites a pre-defined context item
                {
                    let ids = get_overwritten_identifiers(&user_ctx);
                    if !ids.is_empty() {
                        error!(ids = ?ids, "overwritten system identifiers detected");
                        return Err(anyhow!(
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
                let rendered_markdown = engine
                    .renderers()
                    .markdown
                    .render(page, engine.page_store())?;
                tera_ctx.insert("content", &rendered_markdown);
            }

            // render the template with the context
            let renderer = &engine.renderers().tera;
            renderer
                .render(template, &tera_ctx)
                .map(|html| RenderedPage::new(page.page_key, html, page.target_path()))
                .map_err(|e| anyhow!("{}", e))
        }
        None => Err(anyhow!("no template declared for page '{}'", page.uri())),
    }
}

#[instrument(skip_all, fields(page = %for_page.uri()))]
pub fn build_context(
    script_fn_runner: &RuleProcessor,
    page_ctxs: &GlobStore<ContextKey, rhai::FnPtr>,
    for_page: &Page,
) -> Result<Vec<ContextItem>> {
    trace!("building page-specific context");
    let contexts: Vec<Vec<ContextItem>> = page_ctxs
        .find_keys(&for_page.uri())
        .iter()
        .filter_map(|key| page_ctxs.get(*key))
        .map(|ptr| script_fn_runner.run(&ptr, (for_page.clone(),)))
        .try_collect()?;
    let contexts = contexts.into_iter().flatten().collect::<Vec<_>>();

    let mut identifiers = HashSet::new();
    for ctx in &contexts {
        if !identifiers.insert(ctx.identifier.as_str()) {
            return Err(anyhow!(
                "duplicate context identifier encountered in page context generation: {}",
                ctx.identifier.as_str()
            ));
        }
    }

    Ok(contexts)
}

#[derive(Debug)]
pub struct RenderedPage {
    pub page_key: PageKey,
    pub html: String,
    pub target: RelSystemPath,
}

impl RenderedPage {
    pub fn new<S: Into<String> + std::fmt::Debug>(
        page_key: PageKey,
        html: S,
        target: RelSystemPath,
    ) -> Self {
        Self {
            page_key,
            html: html.into(),
            target,
        }
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

    #[instrument(ret)]
    pub fn from_vec(pages: Vec<RenderedPage>) -> Self {
        Self { pages }
    }

    #[instrument(ret)]
    pub fn write_to_disk(&self) -> Result<()> {
        use std::fs;
        for page in &self.pages {
            let target = PathBuf::from(&page.target);
            crate::util::make_parent_dirs(
                target
                    .as_path()
                    .parent()
                    .expect("should have a parent path"),
            )?;
            fs::write(&target, &page.html)?;
        }

        Ok(())
    }

    pub fn iter(&self) -> std::slice::Iter<'_, RenderedPage> {
        self.pages.iter()
    }

    pub fn as_slice(&self) -> &[RenderedPage] {
        self.pages.as_slice()
    }

    pub fn as_mut_slice(&mut self) -> &mut [RenderedPage] {
        self.pages.as_mut_slice()
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

#[instrument(skip_all)]
/// This function rewrites the asset location if applicable
pub fn rewrite_asset_targets(
    rendered_pages: &mut [RenderedPage],
    store: &PageStore,
) -> Result<LinkedAssets> {
    use lol_html::{rewrite_str, RewriteStrSettings};
    use parking_lot::Mutex;

    use std::sync::Arc;

    macro_rules! use_index_handlers {
        (
            $assets:ident, $target_path:expr;
            $($selector:literal $attr:literal),+
            $(,)*
        ) =>
        {{
            vec![
                $(
                    lol_html::element!($selector, |el| {
                        let asset = rewrite_asset_uri_with_use_index(el, $attr, $target_path)?;
                        // add this asset to our collection of located assets
                        let mut all_assets = $assets.lock();
                        all_assets.insert(asset);
                        Ok(())
                    })
                ),+
            ]
        }};
    }

    macro_rules! no_index_handlers {
        (
            $assets:ident, $target_path:expr;
            $($selector:literal $attr:literal),+
            $(,)*
        ) =>
        {{
            vec![
                $(
                    lol_html::element!($selector, |el| {
                        let asset = rewrite_asset_uri_without_use_index(el, $attr, $target_path)?;
                        // add this asset to our collection of located assets
                        let mut all_assets = $assets.lock();
                        all_assets.insert(asset);
                        Ok(())
                    })
                ),+
            ]
        }};
    }

    trace!("rewriting asset targets and generating linked assets");
    let all_assets = Arc::new(Mutex::new(HashSet::new()));

    for rendered_page in rendered_pages.iter_mut() {
        let all_assets = all_assets.clone();

        let page = store
            .get_with_key(rendered_page.page_key)
            .expect("page missing from store. this is a bug");

        // When use_index is true, a directory having the same name as the document
        // is created and the document is placed within this directory as `index.html`.
        // This is done so the URL for the page is cleaner and has no file extension.
        // A consequence of this is all relative links break because we moved the
        // document from its original location to a new one. All relative resource
        // links need to be rewritten to point one directory up (using `..`) and
        // all relative resource links need to be correctly cataloged for pipeline
        // processing.
        if page.frontmatter.use_index && page.src_path().file_stem() != "index" {
            let element_content_handlers = use_index_handlers!(all_assets, &page.target_path();
                "a[href]"        "href",
                "audio[src]"     "src",
                "embed[src]"     "src",
                "img[src]"       "src",
                "link[href]"     "href",
                "object[data]"   "data",
                "script[src]"    "src",
                "source[src]"    "src",
                "source[srcset]" "srcset",
                "track[src]"     "src",
                "video[src]"     "src",
            );

            // We also need to rewrite linked asset locations to point one directory up.
            let rewritten = rewrite_str(
                &rendered_page.html,
                RewriteStrSettings {
                    element_content_handlers,
                    ..RewriteStrSettings::default()
                },
            )?;

            rendered_page.html = rewritten;
        } else {
            let element_content_handlers = no_index_handlers!(all_assets, &page.target_path();
                "a[href]"        "href",
                "audio[src]"     "src",
                "embed[src]"     "src",
                "img[src]"       "src",
                "link[href]"     "href",
                "object[data]"   "data",
                "script[src]"    "src",
                "source[src]"    "src",
                "source[srcset]" "srcset",
                "track[src]"     "src",
                "video[src]"     "src",
            );
            // No rewrite take place for this branch. We already saved the assets
            // in the closure, but we still need to call this function to start
            // the process.
            rewrite_str(
                &rendered_page.html,
                RewriteStrSettings {
                    element_content_handlers,
                    ..RewriteStrSettings::default()
                },
            )?;
        }
    }

    Ok(LinkedAssets::from_hashset(
        Arc::try_unwrap(all_assets).unwrap().into_inner(),
    ))
}

fn rewrite_asset_uri_with_use_index<A: AsRef<str>>(
    el: &mut lol_html::html_content::Element,
    attr: A,
    target_path: &RelSystemPath,
) -> Result<LinkedAsset> {
    let attr = attr.as_ref();
    let attr_value = el
        .get_attribute(attr)
        .ok_or_else(|| anyhow!("missing '{}' attribute in HTML tag. this is a bug", attr))?;
    // assets using an absolute path don't need to be modified
    if attr_value.starts_with('/') {
        Ok(LinkedAsset::new_unmodified(
            &el.tag_name(),
            &attr_value,
            Uri::from_path(&attr_value),
        ))
    } else {
        // Here we are setting the parent directory of all assets
        // based on the path of the html file:
        //    some_dir/the_page/index.html -> some_dir/whatever_assets

        let parent_path = target_path // base/some_dir/the_page/index.html
            .with_base("") //      some_dir/the_page/index.html
            .pop() //              some_dir/the_page
            .pop() //              some_dir
            .to_path_buf();

        // all Uri must begin with a slash
        let mut target = PathBuf::from("/");
        target.push(parent_path);
        target.push(&attr_value);

        el.set_attribute(attr, &format!("{}", target.display()))?;

        Ok(LinkedAsset::new_modified(
            el.tag_name(),
            attr_value,
            Uri::from_path(&target),
        ))
    }
}

fn rewrite_asset_uri_without_use_index<A: AsRef<str>>(
    el: &mut lol_html::html_content::Element,
    attr: A,
    target_path: &RelSystemPath,
) -> Result<LinkedAsset> {
    let attr = attr.as_ref();
    let attr_value = el
        .get_attribute(attr)
        .ok_or_else(|| anyhow!("missing '{}' attribute in HTML tag. this is a bug", attr))?;
    // assets using an absolute path don't need to be modified
    if attr_value.starts_with('/') {
        Ok(LinkedAsset::new_unmodified(
            &el.tag_name(),
            &attr_value,
            Uri::from_path(&attr_value),
        ))
    } else {
        // Here we are setting the parent directory of all assets
        // based on the path of the html file:
        //    some_dir/another_dir/page.html -> some_dir/another_dir
        let mut target = target_path.with_base("").pop().to_path_buf();

        target.push(&attr_value);

        Ok(LinkedAsset::new_modified(
            &el.tag_name(),
            &attr_value,
            Uri::from_path(&target),
        ))
    }
}

#[cfg(test)]
mod test {
    use std::io;

    use super::{rewrite_asset_targets, RenderedPage};
    use crate::core::{Page, PageStore, Uri};
    use crate::{Renderers, Result};

    pub fn page_from_doc_with_paths(
        doc: &str,
        src: &str,
        target: &str,
        path: &str,
    ) -> Result<Page> {
        let renderers = Renderers::new("test/templates/**/*");
        let mut reader = io::Cursor::new(doc.as_bytes());
        Page::from_reader(src, target, path, &mut reader, &renderers)
    }

    macro_rules! test_rewriter {
        ($fn:ident: $use_index:expr, page=$html_path:literal $test:expr) => {
            #[test]
            fn $fn() {
                let mut store = PageStore::new();
                let doc = {
                    if $use_index {
                        r#"
                        +++
                        title = "test"
                        use_index = true
                        template_name = "template"
                        +++ "#
                    } else {
                        r#"
                        +++
                        title = "test"
                        use_index = false
                        template_name = "template"
                        +++ "#
                    }
                };
                let page = page_from_doc_with_paths(doc, "src", "target", $html_path).unwrap();
                let target = page.target_path();

                let key = store.insert(page);

                for (html, expected_html, asset_path) in $test.into_iter() {
                    let mut rendered = RenderedPage::new(key, html, target.clone());

                    let assets =
                        rewrite_asset_targets(std::slice::from_mut(&mut rendered), &store).unwrap();

                    assert_eq!(rendered.html, expected_html);

                    let actual_asset = assets.iter_uris().next().unwrap();
                    assert_eq!(actual_asset, &Uri::from_path(asset_path));
                }
            }
        };
    }

    test_rewriter!(index_defers_to_no_index_when_page_is_named_index:
    true, page="file_path/is/index.html"
    vec![
        (r#"<a href="test.png">"#, r#"<a href="test.png">"#, "/file_path/is/test.png"),
        (r#"<audio src="test.png">"#, r#"<audio src="test.png">"#, "/file_path/is/test.png"),
        (r#"<embed src="test.png">"#, r#"<embed src="test.png">"#, "/file_path/is/test.png"),
        (r#"<img src="test.png">"#, r#"<img src="test.png">"#, "/file_path/is/test.png"),
        (r#"<link href="test.png">"#, r#"<link href="test.png">"#, "/file_path/is/test.png"),
        (r#"<object data="test.png">"#, r#"<object data="test.png">"#, "/file_path/is/test.png"),
        (r#"<script src="test.png">"#, r#"<script src="test.png">"#, "/file_path/is/test.png"),
        (r#"<source src="test.png">"#, r#"<source src="test.png">"#, "/file_path/is/test.png"),
        (r#"<source srcset="test.png">"#, r#"<source srcset="test.png">"#, "/file_path/is/test.png"),
        (r#"<track src="test.png">"#, r#"<track src="test.png">"#, "/file_path/is/test.png"),
        (r#"<video src="test.png">"#, r#"<video src="test.png">"#, "/file_path/is/test.png"),
    ]);

    test_rewriter!(index_get_assets:
    true, page="file_path/is/page.html"
    vec![
        (r#"<a href="test.png">"#, r#"<a href="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<audio src="test.png">"#, r#"<audio src="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<embed src="test.png">"#, r#"<embed src="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<img src="test.png">"#, r#"<img src="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<link href="test.png">"#, r#"<link href="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<object data="test.png">"#, r#"<object data="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<script src="test.png">"#, r#"<script src="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<source src="test.png">"#, r#"<source src="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<source srcset="test.png">"#, r#"<source srcset="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<track src="test.png">"#, r#"<track src="/file_path/is/test.png">"#, "/file_path/is/test.png"),
        (r#"<video src="test.png">"#, r#"<video src="/file_path/is/test.png">"#, "/file_path/is/test.png"),
    ]);

    test_rewriter!(index_get_assets_when_at_root:
    true, page="page.html"
    vec![
        (r#"<a href="test.png">"#, r#"<a href="/test.png">"#, "/test.png"),
        (r#"<audio src="test.png">"#, r#"<audio src="/test.png">"#, "/test.png"),
        (r#"<embed src="test.png">"#, r#"<embed src="/test.png">"#, "/test.png"),
        (r#"<img src="test.png">"#, r#"<img src="/test.png">"#, "/test.png"),
        (r#"<link href="test.png">"#, r#"<link href="/test.png">"#, "/test.png"),
        (r#"<object data="test.png">"#, r#"<object data="/test.png">"#, "/test.png"),
        (r#"<script src="test.png">"#, r#"<script src="/test.png">"#, "/test.png"),
        (r#"<source src="test.png">"#, r#"<source src="/test.png">"#, "/test.png"),
        (r#"<source srcset="test.png">"#, r#"<source srcset="/test.png">"#, "/test.png"),
        (r#"<track src="test.png">"#, r#"<track src="/test.png">"#, "/test.png"),
        (r#"<video src="test.png">"#, r#"<video src="/test.png">"#, "/test.png"),
    ]);

    test_rewriter!(index_get_assets_when_one_level_deep:
    true, page="dir1/page.html"
    vec![
        (r#"<a href="test.png">"#, r#"<a href="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<audio src="test.png">"#, r#"<audio src="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<embed src="test.png">"#, r#"<embed src="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<img src="test.png">"#, r#"<img src="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<link href="test.png">"#, r#"<link href="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<object data="test.png">"#, r#"<object data="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<script src="test.png">"#, r#"<script src="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<source src="test.png">"#, r#"<source src="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<source srcset="test.png">"#, r#"<source srcset="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<track src="test.png">"#, r#"<track src="/dir1/test.png">"#, "/dir1/test.png"),
        (r#"<video src="test.png">"#, r#"<video src="/dir1/test.png">"#, "/dir1/test.png"),
    ]);

    test_rewriter!(index_get_assets_within_subdirs:
    true, page="file_path/a/b/page.html"
    vec![
        (r#"<a href="test.png">"#, r#"<a href="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<audio src="test.png">"#, r#"<audio src="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<embed src="test.png">"#, r#"<embed src="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<img src="test.png">"#, r#"<img src="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<link href="test.png">"#, r#"<link href="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<object data="test.png">"#, r#"<object data="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<script src="test.png">"#, r#"<script src="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<source src="test.png">"#, r#"<source src="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<source srcset="test.png">"#, r#"<source srcset="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<track src="test.png">"#, r#"<track src="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
        (r#"<video src="test.png">"#, r#"<video src="/file_path/a/b/test.png">"#, "/file_path/a/b/test.png"),
    ]);

    test_rewriter!(index_get_absolute_path_assets:
    true, page="a/b/page.html"
    vec![
        (r#"<a href="/test.png">"#, r#"<a href="/test.png">"#, "/test.png"),
        (r#"<audio src="/test.png">"#, r#"<audio src="/test.png">"#, "/test.png"),
        (r#"<embed src="/test.png">"#, r#"<embed src="/test.png">"#, "/test.png"),
        (r#"<img src="/test.png">"#, r#"<img src="/test.png">"#, "/test.png"),
        (r#"<link href="/test.png">"#, r#"<link href="/test.png">"#, "/test.png"),
        (r#"<object data="/test.png">"#, r#"<object data="/test.png">"#, "/test.png"),
        (r#"<script src="/test.png">"#, r#"<script src="/test.png">"#, "/test.png"),
        (r#"<source src="/test.png">"#, r#"<source src="/test.png">"#, "/test.png"),
        (r#"<source srcset="/test.png">"#, r#"<source srcset="/test.png">"#, "/test.png"),
        (r#"<track src="/test.png">"#, r#"<track src="/test.png">"#, "/test.png"),
        (r#"<video src="/test.png">"#, r#"<video src="/test.png">"#, "/test.png"),
    ]);

    test_rewriter!(noindex_get_assets_within_subdirs:
    false, page="file_path/is/page.html"
    vec![
        (r#"<a href="test.png">"#, r#"<a href="test.png">"#, "/file_path/is/test.png"),
        (r#"<audio src="test.png">"#, r#"<audio src="test.png">"#, "/file_path/is/test.png"),
        (r#"<embed src="test.png">"#, r#"<embed src="test.png">"#, "/file_path/is/test.png"),
        (r#"<img src="test.png">"#, r#"<img src="test.png">"#, "/file_path/is/test.png"),
        (r#"<link href="test.png">"#, r#"<link href="test.png">"#, "/file_path/is/test.png"),
        (r#"<object data="test.png">"#, r#"<object data="test.png">"#, "/file_path/is/test.png"),
        (r#"<script src="test.png">"#, r#"<script src="test.png">"#, "/file_path/is/test.png"),
        (r#"<source src="test.png">"#, r#"<source src="test.png">"#, "/file_path/is/test.png"),
        (r#"<source srcset="test.png">"#, r#"<source srcset="test.png">"#, "/file_path/is/test.png"),
        (r#"<track src="test.png">"#, r#"<track src="test.png">"#, "/file_path/is/test.png"),
        (r#"<video src="test.png">"#, r#"<video src="test.png">"#, "/file_path/is/test.png"),
    ]);

    test_rewriter!(noindex_get_absolute_path_assets:
    false, page="file_path/is/page.html"
    vec![
        (r#"<a href="/test.png">"#, r#"<a href="/test.png">"#, "/test.png"),
        (r#"<audio src="/test.png">"#, r#"<audio src="/test.png">"#, "/test.png"),
        (r#"<embed src="/test.png">"#, r#"<embed src="/test.png">"#, "/test.png"),
        (r#"<img src="/test.png">"#, r#"<img src="/test.png">"#, "/test.png"),
        (r#"<link href="/test.png">"#, r#"<link href="/test.png">"#, "/test.png"),
        (r#"<object data="/test.png">"#, r#"<object data="/test.png">"#, "/test.png"),
        (r#"<script src="/test.png">"#, r#"<script src="/test.png">"#, "/test.png"),
        (r#"<source src="/test.png">"#, r#"<source src="/test.png">"#, "/test.png"),
        (r#"<source srcset="/test.png">"#, r#"<source srcset="/test.png">"#, "/test.png"),
        (r#"<track src="/test.png">"#, r#"<track src="/test.png">"#, "/test.png"),
        (r#"<video src="/test.png">"#, r#"<video src="/test.png">"#, "/test.png"),
    ]);

    test_rewriter!(noindex_get_assets:
    false, page="file_path/is/page.html"
    vec![
        (r#"<a href="test.png">"#, r#"<a href="test.png">"#, "/file_path/is/test.png"),
        (r#"<audio src="test.png">"#, r#"<audio src="test.png">"#, "/file_path/is/test.png"),
        (r#"<embed src="test.png">"#, r#"<embed src="test.png">"#, "/file_path/is/test.png"),
        (r#"<img src="test.png">"#, r#"<img src="test.png">"#, "/file_path/is/test.png"),
        (r#"<link href="test.png">"#, r#"<link href="test.png">"#, "/file_path/is/test.png"),
        (r#"<object data="test.png">"#, r#"<object data="test.png">"#, "/file_path/is/test.png"),
        (r#"<script src="test.png">"#, r#"<script src="test.png">"#, "/file_path/is/test.png"),
        (r#"<source src="test.png">"#, r#"<source src="test.png">"#, "/file_path/is/test.png"),
        (r#"<source srcset="test.png">"#, r#"<source srcset="test.png">"#, "/file_path/is/test.png"),
        (r#"<track src="test.png">"#, r#"<track src="test.png">"#, "/file_path/is/test.png"),
        (r#"<video src="test.png">"#, r#"<video src="test.png">"#, "/file_path/is/test.png"),
    ]);

    test_rewriter!(noindex_get_assets_when_at_root:
    false, page="page.html"
    vec![
        (r#"<a href="test.png">"#, r#"<a href="test.png">"#, "/test.png"),
        (r#"<audio src="test.png">"#, r#"<audio src="test.png">"#, "/test.png"),
        (r#"<embed src="test.png">"#, r#"<embed src="test.png">"#, "/test.png"),
        (r#"<img src="test.png">"#, r#"<img src="test.png">"#, "/test.png"),
        (r#"<link href="test.png">"#, r#"<link href="test.png">"#, "/test.png"),
        (r#"<object data="test.png">"#, r#"<object data="test.png">"#, "/test.png"),
        (r#"<script src="test.png">"#, r#"<script src="test.png">"#, "/test.png"),
        (r#"<source src="test.png">"#, r#"<source src="test.png">"#, "/test.png"),
        (r#"<source srcset="test.png">"#, r#"<source srcset="test.png">"#, "/test.png"),
        (r#"<track src="test.png">"#, r#"<track src="test.png">"#, "/test.png"),
        (r#"<video src="test.png">"#, r#"<video src="test.png">"#, "/test.png"),
    ]);
}

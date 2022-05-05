use anyhow::anyhow;
use itertools::Itertools;
use std::{collections::HashSet, path::PathBuf};

use tracing::{error, instrument, trace};

use crate::{
    core::{
        engine::Engine,
        page::{ContextItem, PageKey},
        rules::{ContextKey, GlobStore, RuleProcessor},
        Page, SysPath,
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
    pub target: SysPath,
}

impl RenderedPage {
    pub fn new<S: Into<String> + std::fmt::Debug>(
        page_key: PageKey,
        html: S,
        target: SysPath,
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

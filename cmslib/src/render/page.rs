use anyhow::anyhow;
use std::{collections::HashSet, path::Path};

use tracing::{error, instrument, trace};

use crate::{
    core::{engine::Engine, rules::gctx::ContextItem},
    page::Page,
    site_context::SiteContext,
    util::RetargetablePathBuf,
};

#[instrument(skip(engine), fields(page=?page.canonical_path.to_string()))]
pub fn render(engine: &Engine, page: &Page) -> Result<RenderedPage, anyhow::Error> {
    trace!("rendering page");

    let site_ctx = SiteContext::new("sample");

    match page.frontmatter.template_path.as_ref() {
        Some(template) => {
            let mut tera_ctx = tera::Context::new();
            tera_ctx.insert("site", &site_ctx);
            tera_ctx.insert("content", &page.markdown);
            tera_ctx.insert("page_store", {
                &engine.page_store().iter().collect::<Vec<_>>()
            });
            if let Some(global) = engine.rules().global_context() {
                tera_ctx.insert("global", global);
            }

            let meta_ctx = tera::Context::from_serialize(&page.frontmatter.meta)
                .expect("failed converting page metadata into tera context");
            tera_ctx.extend(meta_ctx);

            let user_ctx = {
                let user_ctx_generators = engine.rules().page_context();
                crate::core::rules::script::build_context(
                    &engine.rule_processor(),
                    user_ctx_generators,
                    page,
                )?
            };

            {
                let ids = get_overwritten_identifiers(&user_ctx);
                if !ids.is_empty() {
                    error!(ids = ?ids, "overwritten system identifiers detected");
                    return Err(anyhow!(
                        "cannot overwrite reserved system context identifiers"
                    ));
                }
            }

            for ctx in user_ctx {
                let mut user_ctx = tera::Context::new();
                user_ctx.insert(ctx.identifier, &ctx.data);
                tera_ctx.extend(user_ctx);
            }

            let renderer = &engine.renderers().tera;
            renderer
                .render(template, &tera_ctx)
                .map(|html| {
                    // change file extension to 'html'
                    let target_path = {
                        if page.canonical_path.as_str().ends_with("index.md") {
                            page.system_path
                                .with_root::<&Path>(&engine.config().output_root)
                                .with_extension("html")
                        } else {
                            // will require linking directly to file
                            if page.frontmatter.use_file_url {
                                page.system_path
                                    .with_root::<&Path>(&engine.config().output_root)
                                    .with_extension("html")
                            } else {
                                // uses index.html and a subdirectory with the
                                // page name will be created so no direct
                                // file link is needed
                                let mut path = page
                                    .system_path
                                    .with_root::<&Path>(&engine.config().output_root)
                                    .with_extension("");
                                path.push_path("index.html");
                                path
                            }
                        }
                    };
                    RenderedPage::new(html, &target_path)
                })
                .map_err(|e| anyhow!("{}", e))
        }
        None => Err(anyhow!(
            "no template declared for page '{}'",
            page.canonical_path.to_string()
        )),
    }
}

#[derive(Debug)]
pub struct RenderedPage {
    pub html: String,
    pub target: RetargetablePathBuf,
}

impl RenderedPage {
    pub fn new<S: Into<String> + std::fmt::Debug>(html: S, target: &RetargetablePathBuf) -> Self {
        Self {
            html: html.into(),
            target: target.clone(),
        }
    }
}

#[derive(Debug)]
pub struct RenderedPageCollection {
    pages: Vec<RenderedPage>,
}

impl RenderedPageCollection {
    pub fn from_iterable<T: Iterator<Item = RenderedPage>>(iterable: T) -> Self {
        Self {
            pages: iterable.collect::<Vec<_>>(),
        }
    }

    pub fn from_vec(pages: Vec<RenderedPage>) -> Self {
        Self { pages }
    }

    pub fn write_to_disk(&self) -> Result<(), std::io::Error> {
        use std::fs;
        for page in self.pages.iter() {
            let target = page.target.to_path_buf();
            crate::util::make_parent_dirs(target.parent().expect("should have a parent path"))?;
            let _ = fs::write(&target, &page.html)?;
        }

        Ok(())
    }

    pub fn iter<'a>(&'a self) -> std::slice::Iter<'a, RenderedPage> {
        self.pages.iter()
    }

    pub fn as_slice(&self) -> &[RenderedPage] {
        self.pages.as_slice()
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

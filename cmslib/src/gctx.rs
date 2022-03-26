use crate::{
    frontmatter::FrontMatter,
    page::Page,
    pagestore::PageStore,
    util::{Glob, GlobCandidate},
};
use anyhow::anyhow;
use slotmap::SlotMap;
use std::collections::HashSet;
use tracing::{instrument, trace};

slotmap::new_key_type! {
    pub struct GeneratorKey;
}

pub type GenFn = Box<dyn Fn(&PageStore, &Page) -> ContextItem>;
pub struct GeneratorFunc(GenFn);

impl GeneratorFunc {
    pub fn new(func: GenFn) -> Self {
        Self(func)
    }
}

impl GeneratorFunc {
    pub fn call(&self, store: &PageStore, page: &Page) -> ContextItem {
        self.0(store, page)
    }
}

impl std::fmt::Debug for GeneratorFunc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContextGeneratorFn")
    }
}

#[derive(Debug)]
pub struct Generator {
    matcher: Matcher,
    func: GeneratorFunc,
}

impl Generator {
    #[must_use]
    pub fn new(matcher: Matcher, func: GeneratorFunc) -> Self {
        Self { matcher, func }
    }
}

#[derive(Debug)]
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

pub enum Matcher {
    // Runs when the canonical path matches some glob(s). Easy to define specific pages.
    Glob(Vec<Glob>),
    // Runs when the closure returns true. Allows user to define own parameters such
    // as processing metadata (author, title, etc).
    Metadata(Box<dyn Fn(&FrontMatter) -> bool>),
}

impl std::fmt::Debug for Matcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Glob(globs) => {
                f.write_fmt(format_args!(
                    "PageContextMatcher: Glob with {} globs",
                    globs.len()
                ))?;
                f.debug_list().entries(globs.iter()).finish()
            }
            Self::Metadata(_) => f.write_str("PageContextMatcher: Metadata closure"),
        }
    }
}

pub struct Generators {
    generators: SlotMap<GeneratorKey, GeneratorFunc>,
    matchers: Vec<(Matcher, GeneratorKey)>,
}

impl std::fmt::Debug for Generators {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextGenerators")
            .field("generators", &format_args!("{}", self.generators.len()))
            .field("matchers", &format_args!("{:?}", self.matchers))
            .finish()
    }
}

impl Generators {
    #[must_use]
    pub fn new() -> Self {
        Self {
            generators: SlotMap::with_key(),
            matchers: vec![],
        }
    }

    #[instrument(skip_all)]
    pub fn add_generator(&mut self, matcher: Matcher, generator: GeneratorFunc) {
        trace!("add context generator function");
        let key = self.generators.insert(generator);
        self.matchers.push((matcher, key));
    }

    fn find_generators(&self, page: &Page) -> Vec<GeneratorKey> {
        self.matchers
            .iter()
            .filter_map(|(matcher, generator_key)| match matcher {
                Matcher::Glob(globs) => {
                    let candidate = GlobCandidate::new(page.canonical_path.as_str());

                    let mut is_match = false;
                    for g in globs {
                        if g.is_match_candidate(&candidate) {
                            is_match = true;
                            break;
                        }
                    }
                    if is_match {
                        Some(*generator_key)
                    } else {
                        None
                    }
                }
                Matcher::Metadata(func) => {
                    if func(&page.frontmatter) {
                        Some(*generator_key)
                    } else {
                        None
                    }
                }
            })
            .collect()
    }

    #[instrument(skip(self, page_store, for_page), fields(page = ?for_page.canonical_path.to_string()))]
    pub fn build_context(
        &self,
        page_store: &PageStore,
        for_page: &Page,
    ) -> Result<Vec<ContextItem>, anyhow::Error> {
        trace!("building page-specific context");
        let contexts = self
            .find_generators(for_page)
            .iter()
            .filter_map(|key| self.generators.get(*key))
            .map(|gen| gen.call(page_store, for_page))
            .collect::<Vec<_>>();

        let mut identifiers = HashSet::new();
        for ctx in contexts.iter() {
            if !identifiers.insert(ctx.identifier.as_str()) {
                return Err(anyhow!(
                    "duplicate context identifier encountered in page context generation: {}",
                    ctx.identifier.as_str()
                ));
            }
        }

        Ok(contexts)
    }
}

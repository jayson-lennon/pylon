use crate::util::{Glob, GlobCandidate, RetargetablePathBuf};
use crate::{CanonicalPath, Renderers};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

slotmap::new_key_type! {
    pub struct PageKey;
    pub struct GeneratorKey;
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FrontMatter {
    pub template_path: Option<String>,
    pub meta: HashMap<String, serde_json::Value>,
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

    pub fn set_default_template(
        &mut self,
        template_paths: &HashSet<&str>,
    ) -> Result<(), anyhow::Error> {
        if self.frontmatter.template_path.is_none() {
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
}

fn get_default_template_path(
    default_template_dirs: &HashSet<&str>,
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

        if default_template_dirs.contains(&template_path.as_str()) {
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

#[derive(Debug)]
pub struct PageStore {
    src_root: PathBuf,
    pages: SlotMap<PageKey, Page>,
    key_store: HashMap<PathBuf, PageKey>,
}

impl PageStore {
    pub fn new<P: AsRef<Path>>(src_root: P) -> Self {
        Self {
            src_root: src_root.as_ref().to_path_buf(),
            pages: SlotMap::with_key(),
            key_store: HashMap::new(),
        }
    }

    pub fn get_with_key(&self, key: PageKey) -> Option<&Page> {
        self.pages.get(key)
    }

    pub fn get<P: AsRef<Path>>(&self, path: P) -> Option<&Page> {
        let path = path.as_ref();
        let page_key = self.key_store.get(path)?;
        self.pages.get(*page_key)
    }

    pub fn get_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Page> {
        let path = path.as_ref();
        let page_key = self.key_store.get(path)?;
        self.pages.get_mut(*page_key)
    }

    pub fn insert(&mut self, page: Page) -> PageKey {
        let search_key = page.system_path.to_path_buf();

        let page_key = self.pages.insert_with_key(|key| {
            let mut page = page;
            page.set_page_key(key);
            page
        });

        self.key_store.insert(search_key, page_key);

        page_key
    }

    pub fn insert_batch(&mut self, pages: Vec<Page>) {
        for page in pages {
            self.insert(page);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Page> {
        self.pages.iter().map(|(_, page)| page)
    }
}

pub type ContextGeneratorFunc = Box<dyn Fn(&PageStore, &Page) -> ContextItem>;
pub struct ContextGeneratorFn(ContextGeneratorFunc);

impl ContextGeneratorFn {
    pub fn new(func: ContextGeneratorFunc) -> Self {
        Self(func)
    }
}

impl ContextGeneratorFn {
    pub fn call(&self, store: &PageStore, page: &Page) -> ContextItem {
        self.0(store, page)
    }
}

impl std::fmt::Debug for ContextGeneratorFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContextGeneratorFn")
    }
}

#[derive(Debug)]
pub struct ContextGenerator {
    matcher: ContextMatcher,
    func: ContextGeneratorFn,
}

impl ContextGenerator {
    #[must_use]
    pub fn new(matcher: ContextMatcher, func: ContextGeneratorFn) -> Self {
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

pub enum ContextMatcher {
    // Runs when the canonical path matches some glob(s). Easy to define specific pages.
    Glob(Vec<Glob>),
    // Runs when the closure returns true. Allows user to define own parameters such
    // as processing metadata (author, title, etc).
    Metadata(Box<dyn Fn(&FrontMatter) -> bool>),
}

impl std::fmt::Debug for ContextMatcher {
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

pub struct ContextGenerators {
    generators: SlotMap<GeneratorKey, ContextGeneratorFn>,
    matchers: Vec<(ContextMatcher, GeneratorKey)>,
}

impl std::fmt::Debug for ContextGenerators {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextGenerators")
            .field("generators", &format_args!("{}", self.generators.len()))
            .field("matchers", &format_args!("{:?}", self.matchers))
            .finish()
    }
}

impl ContextGenerators {
    #[must_use]
    pub fn new() -> Self {
        Self {
            generators: SlotMap::with_key(),
            matchers: vec![],
        }
    }

    pub fn add_generator(&mut self, matcher: ContextMatcher, generator: ContextGeneratorFn) {
        let key = self.generators.insert(generator);
        self.matchers.push((matcher, key));
    }

    fn find_generators(&self, page: &Page) -> Vec<GeneratorKey> {
        self.matchers
            .iter()
            .filter_map(|(matcher, generator_key)| match matcher {
                ContextMatcher::Glob(globs) => {
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
                ContextMatcher::Metadata(func) => {
                    if func(&page.frontmatter) {
                        Some(*generator_key)
                    } else {
                        None
                    }
                }
            })
            .collect()
    }

    pub fn build_context(
        &self,
        page_store: &PageStore,
        for_page: &Page,
    ) -> Result<Vec<ContextItem>, anyhow::Error> {
        let contexts = self
            .find_generators(for_page)
            .iter()
            .filter_map(|key| self.generators.get(*key))
            .map(|gen| gen.call(page_store, for_page))
            .collect::<Vec<_>>();
        dbg!(&contexts);

        let mut identifiers: HashSet<&str> = HashSet::new();
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

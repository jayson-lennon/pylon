use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

slotmap::new_key_type! {
    pub struct PageKey;
}

use crate::{util::RetargetablePathBuf, CanonicalPath, Renderers};

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

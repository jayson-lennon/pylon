use crate::page::{Page, PageKey};
use slotmap::SlotMap;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tracing::{instrument, trace};

#[derive(Debug, Clone)]
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
        let mut path = path.as_ref().to_path_buf();
        path.set_extension("md");
        dbg!(&path);
        let page_key = self.key_store.get(&path)?;
        dbg!(&page_key);
        self.pages.get(*page_key)
    }

    pub fn get_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Page> {
        let path = path.as_ref();
        let page_key = self.key_store.get(path)?;
        self.pages.get_mut(*page_key)
    }

    #[instrument(skip_all, fields(page=%page.canonical_path.to_string()))]
    pub fn insert(&mut self, page: Page) -> PageKey {
        trace!("inserting page into page store");
        let search_key = page.canonical_path.as_str().to_owned();

        let page_key = self.pages.insert_with_key(|key| {
            let mut page = page;
            page.set_page_key(key);
            page
        });

        self.key_store.insert(search_key.into(), page_key);

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

impl IntoIterator for PageStore {
    type Item = Page;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.pages
            .iter()
            .map(|(_, page)| page)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
    }
}

pub mod script {
    use super::PageStore;
    use rhai::plugin::*;

    impl PageStore {
        /// Returns the page at the given `path`. Returns `()` if the page was not found.
        fn _script_get(&mut self, path: &str) -> rhai::Dynamic {
            self.get(path)
                .cloned()
                .map(|p| Dynamic::from(p))
                .unwrap_or_else(|| ().into())
        }
    }

    pub fn register_type(engine: &mut rhai::Engine) {
        engine
            .register_type::<PageStore>()
            .register_fn("get", PageStore::_script_get)
            .register_iterator::<PageStore>();
    }
}

use crate::page::{Page, PageKey};
use slotmap::SlotMap;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tracing::{instrument, trace};

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

    #[instrument(skip_all, fields(page=%page.canonical_path.to_string()))]
    pub fn insert(&mut self, page: Page) -> PageKey {
        trace!("inserting page into page store");
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

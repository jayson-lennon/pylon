use crate::{
    core::page::{Page, PageKey},
    core::Uri,
};
use slotmap::SlotMap;
use std::collections::HashMap;
use tracing::{instrument, trace};

#[derive(Debug, Clone)]
pub struct PageStore {
    pages: SlotMap<PageKey, Page>,
    key_map: HashMap<Uri, PageKey>,
}

impl PageStore {
    pub fn new() -> Self {
        Self {
            pages: SlotMap::with_key(),
            key_map: HashMap::new(),
        }
    }

    #[instrument(ret)]
    pub fn get_with_key(&self, key: PageKey) -> Option<&Page> {
        self.pages.get(key)
    }

    #[instrument(ret)]
    pub fn get(&self, uri: &Uri) -> Option<&Page> {
        let page_key = self.key_map.get(uri)?;
        self.pages.get(*page_key)
    }

    #[instrument]
    pub fn get_mut(&mut self, uri: &Uri) -> Option<&mut Page> {
        let page_key = self.key_map.get(uri)?;
        self.pages.get_mut(*page_key)
    }

    #[instrument(skip_all, fields(page=%page.uri()))]
    pub fn update(&mut self, page: Page) -> PageKey {
        trace!("updating existing page");

        let (old_search_keys, new_search_keys) = (vec![], vec![]);

        let page_key = match self.get_mut(&page.uri()) {
            Some(old) => {
                let mut page = page;
                page.page_key = old.page_key;

                *old = page;
                old.page_key
            }
            None => self.insert(page),
        };

        for key in old_search_keys {
            self.key_map.remove(&key);
        }
        for key in new_search_keys {
            self.key_map.insert(key, page_key);
        }

        page_key
    }

    #[instrument(skip_all, fields(page=%page.uri()))]
    pub fn insert(&mut self, page: Page) -> PageKey {
        trace!("inserting page into page store");

        let search_keys = build_search_keys(&page);

        let page_key = self.pages.insert_with_key(|key| {
            let mut page = page;
            page.set_page_key(key);
            page
        });

        for key in search_keys {
            self.key_map.insert(key, page_key);
        }

        page_key
    }

    #[instrument]
    pub fn insert_batch(&mut self, pages: Vec<Page>) {
        for page in pages {
            self.insert(page);
        }
    }

    #[instrument]
    pub fn iter<'a>(&'a self) -> slotmap::basic::Iter<'a, PageKey, Page> {
        self.pages.iter()
    }
}

impl Default for PageStore {
    fn default() -> Self {
        Self::new()
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

impl<'a> IntoIterator for &'a PageStore {
    type Item = (PageKey, &'a Page);
    type IntoIter = slotmap::basic::Iter<'a, PageKey, Page>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

fn build_search_keys(page: &Page) -> Vec<Uri> {
    let search_keys = vec![
        page.uri(),
        Uri::from_path(&page.uri().as_str().replace(".html", ".md")),
    ];

    dbg!(&search_keys);

    search_keys
}

pub mod script {

    use crate::core::Uri;

    use super::PageStore;

    #[allow(clippy::wildcard_imports)]
    use rhai::plugin::*;

    impl PageStore {
        /// Returns the page at the given `path`. Returns `()` if the page was not found.
        fn _script_get(&mut self, uri: &str) -> Result<rhai::Dynamic, Box<EvalAltResult>> {
            let uri = Uri::new(uri)
                .map_err(|e| EvalAltResult::ErrorSystem("failed parsing uri".into(), e.into()))?;
            let k = self
                .get(&uri)
                .cloned()
                .map_or_else(|| ().into(), Dynamic::from);
            Ok(k)
        }
    }

    pub fn register_type(engine: &mut rhai::Engine) {
        engine
            .register_type::<PageStore>()
            .register_result_fn("get", PageStore::_script_get)
            .register_iterator::<PageStore>();
    }
}

#[cfg(test)]
mod test {

    use super::PageStore;
    use crate::core::{
        page::page::test::{doc::MINIMAL, new_page},
        Uri,
    };

    #[test]
    fn inserts_and_queries_pages() {
        let page1 = new_page(MINIMAL, "path/1/page.md", "src", "target").unwrap();
        let page2 = new_page(MINIMAL, "path/2/page.md", "src", "target").unwrap();

        let mut store = PageStore::new();
        let key1 = store.insert(page1);
        let key2 = store.insert(page2);

        assert!(store.get_with_key(key1).is_some());

        let page1 = store.get(&Uri::new("/path/1/page.html").unwrap()).unwrap();
        assert_eq!(page1.page_key, key1);

        let page2 = store.get(&Uri::new("/path/2/page.html").unwrap()).unwrap();
        assert_eq!(page2.page_key, key2);
    }

    #[test]
    fn builds_search_key() {
        let page = new_page(MINIMAL, "path/1/page.md", "src", "target").unwrap();

        let mut store = PageStore::new();
        let key = store.insert(page);

        assert!(store.get_with_key(key).is_some());

        let page = store.get(&Uri::from_path("/path/1/page.md")).unwrap();
        assert_eq!(page.page_key, key);

        let page = store.get(&Uri::from_path("/path/1/page.html")).unwrap();
        assert_eq!(page.page_key, key);
    }
}

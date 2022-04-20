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

    #[must_use]
    #[instrument(skip_all, fields(page=%page.uri()))]
    pub fn update(&mut self, page: Page) -> PageKey {
        trace!("updating existing page");

        let (mut old_search_keys, mut new_search_keys) = (vec![], vec![]);

        let page_key = match self.get_mut(&page.uri()) {
            Some(old) => {
                let mut page = page;
                page.page_key = old.page_key;

                // `use_index` causes the path of the documents to change, so
                // we need to update the search keys when it differs.
                if old.frontmatter.use_index != page.frontmatter.use_index {
                    old_search_keys = build_search_keys(old);
                    new_search_keys = build_search_keys(&page);
                }

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
    let mut search_keys = vec![
        page.uri(),
        Uri::from_path(&page.uri().as_str().replace(".html", ".md")),
    ];

    if page.frontmatter.use_index {
        let path = page.src_path();
        let uri_md = Uri::from_path(path.target());
        let uri_html = Uri::from_path(uri_md.as_str().replace(".md", ".html"));
        search_keys.push(uri_md);
        search_keys.push(uri_html);
    }

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
    use crate::core::{page::page::test as page_test, Uri};

    #[test]
    fn inserts_and_queries_pages() {
        let page1 = page_test::page_from_doc_with_paths(
            page_test::doc::MINIMAL,
            "src",
            "tgt",
            "path/1/page.md",
        )
        .unwrap();
        let page2 = page_test::page_from_doc_with_paths(
            page_test::doc::MINIMAL,
            "src",
            "tgt",
            "path/2/page.md",
        )
        .unwrap();

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
    fn builds_search_key_when_using_index_target() {
        let page = page_test::page_from_doc_with_paths(
            page_test::doc::MINIMAL,
            "src",
            "tgt",
            "path/1/page.md",
        )
        .unwrap();

        let mut store = PageStore::new();
        let key = store.insert(page);

        assert!(store.get_with_key(key).is_some());

        let page = store.get(&Uri::from_path("/path/1/page")).unwrap();
        assert_eq!(page.page_key, key);

        let page = store.get(&Uri::from_path("/path/1/page.md")).unwrap();
        assert_eq!(page.page_key, key);

        let page = store.get(&Uri::from_path("/path/1/page.html")).unwrap();
        assert_eq!(page.page_key, key);
    }

    #[test]
    fn builds_search_key_when_using_direct_target() {
        let page = page_test::page_from_doc_with_paths(
            r#"
            +++
            use_index = false
            title = "test"
            template_name = "test"
            +++"#,
            "src",
            "tgt",
            "path/1/page.md",
        )
        .unwrap();

        let mut store = PageStore::new();
        let key = store.insert(page);

        assert!(store.get_with_key(key).is_some());

        let page = store.get(&Uri::from_path("/path/1/page.md")).unwrap();
        assert_eq!(page.page_key, key);

        let page = store.get(&Uri::from_path("/path/1/page.html")).unwrap();
        assert_eq!(page.page_key, key);

        assert!(store
            .get(&Uri::from_path("/path/1/page/index.html"))
            .is_none());
        assert!(store
            .get(&Uri::from_path("/path/1/page/index.md"))
            .is_none());
    }

    #[test]
    fn updates_search_keys_when_useindex_changes() {
        let before = page_test::page_from_doc_with_paths(
            r#"+++
            use_index = true
            template_name = "empty.tera"
            +++"#,
            "src",
            "tgt",
            "path/1/page.md",
        )
        .unwrap();
        let after = page_test::page_from_doc_with_paths(
            r#"+++
            use_index = false
            template_name = "empty.tera"
            +++"#,
            "src",
            "tgt",
            "path/1/page.md",
        )
        .unwrap();

        let mut store = PageStore::new();
        let key_before = store.insert(before);

        assert!(store.get(&Uri::from_path("/path/1/page.md")).is_some());
        assert!(store.get(&Uri::from_path("/path/1/page.html")).is_some());
        assert!(store.get(&Uri::from_path("/path/1/page")).is_some());

        let key_after = store.update(after);

        assert_eq!(key_before, key_after);

        assert!(store.get(&Uri::from_path("/path/1/page.md")).is_some());
        assert!(store.get(&Uri::from_path("/path/1/page.html")).is_some());
        assert!(store.get(&Uri::from_path("/path/1/page")).is_none());
    }
}
use crate::core::page::{Page, PageKey};
use slotmap::SlotMap;
use std::collections::HashMap;
use std::fmt;
use tracing::trace;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SearchKey(String);

impl SearchKey {
    pub fn new<S: Into<String>>(key: S) -> Self {
        let key: String = key.into();
        Self(key)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for SearchKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for SearchKey {
    fn from(key: &str) -> Self {
        Self(key.to_string())
    }
}

impl From<&String> for SearchKey {
    fn from(key: &String) -> Self {
        Self(key.clone())
    }
}

impl From<String> for SearchKey {
    fn from(key: String) -> Self {
        Self(key)
    }
}

impl fmt::Display for SearchKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Library {
    pages: SlotMap<PageKey, Page>,
    key_map: HashMap<SearchKey, PageKey>,
}

impl Library {
    pub fn new() -> Self {
        Self {
            pages: SlotMap::with_key(),
            key_map: HashMap::new(),
        }
    }

    pub fn get_with_key(&self, key: PageKey) -> Option<&Page> {
        self.pages.get(key)
    }

    pub fn get(&self, search_key: &SearchKey) -> Option<&Page> {
        let page_key = self.key_map.get(search_key)?;
        self.pages.get(*page_key)
    }

    pub fn get_mut(&mut self, search_key: &SearchKey) -> Option<&mut Page> {
        let page_key = self.key_map.get(search_key)?;
        self.pages.get_mut(*page_key)
    }

    pub fn update(&mut self, page: Page) -> PageKey {
        trace!("updating existing page");

        let page_key = match self.get_mut(&page.search_keys()[0]) {
            Some(old) => {
                let mut page = page;
                page.page_key = old.page_key;

                *old = page;
                old.page_key
            }
            None => self.insert(page),
        };

        page_key
    }

    pub fn insert(&mut self, page: Page) -> PageKey {
        trace!("inserting page into page store");

        let search_keys = page.search_keys();

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

    pub fn insert_batch(&mut self, pages: Vec<Page>) {
        for page in pages {
            self.insert(page);
        }
    }

    pub fn iter<'a>(&'a self) -> slotmap::basic::Iter<'a, PageKey, Page> {
        self.pages.iter()
    }
}

impl Default for Library {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for Library {
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

impl<'a> IntoIterator for &'a Library {
    type Item = (PageKey, &'a Page);
    type IntoIter = slotmap::basic::Iter<'a, PageKey, Page>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub mod script {

    use super::{Library, SearchKey};

    #[allow(clippy::wildcard_imports)]
    use rhai::plugin::*;

    impl Library {
        /// Returns the page at the given `path`. Returns `()` if the page was not found.
        fn _script_get(&mut self, search_key: &str) -> rhai::Dynamic {
            self.get(&SearchKey::from(search_key))
                .cloned()
                .map_or_else(|| ().into(), Dynamic::from)
        }
    }

    pub fn register_type(engine: &mut rhai::Engine) {
        engine
            .register_type::<Library>()
            .register_fn("get", Library::_script_get)
            .register_iterator::<Library>();
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use super::Library;
    use crate::core::page::page::test::{doc::MINIMAL, new_page, new_page_with_tree};
    use temptree::temptree;

    #[test]
    fn inserts_and_queries_pages() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {
                "default.tera": "",
                "empty.tera": "",
            },
            target: {},
            src: {
                "1": {
                    "page.md": "",
                },
                "2": {
                    "page.md": "",
                },
            },
            syntax_themes: {},
        };

        let page1 = new_page_with_tree(&tree, &tree.path().join("src/1/page.md"), MINIMAL).unwrap();
        let page2 = new_page_with_tree(&tree, &tree.path().join("src/2/page.md"), MINIMAL).unwrap();

        let mut store = Library::new();
        let key1 = store.insert(page1);
        let key2 = store.insert(page2);

        assert!(store.get_with_key(key1).is_some());

        let page1 = store.get(&"/1/page.md".into()).unwrap();
        assert_eq!(page1.page_key, key1);

        let page2 = store.get(&"/2/page.md".into()).unwrap();
        assert_eq!(page2.page_key, key2);
    }

    #[test]
    fn builds_search_key() {
        let tree = temptree! {
            "rules.rhai": "",
            templates: {
                "default.tera": "",
                "empty.tera": "",
            },
            target: {},
            src: {
                "1": {
                    "page.md": "",
                },
                "2": {
                    "page.md": "",
                },
            },
            syntax_themes: {},
        };

        let page = new_page_with_tree(&tree, &tree.path().join("src/1/page.md"), MINIMAL).unwrap();

        let mut store = Library::new();
        let key = store.insert(page);

        assert!(store.get_with_key(key).is_some());

        let page = store.get(&"/1/page.md".into()).unwrap();
        assert_eq!(page.page_key, key);
    }
}

use crate::SearchDoc;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct SearchDocs {
    inner: Vec<SearchDoc>,
}

impl SearchDocs {
    pub fn new() -> Self {
        Self { inner: vec![] }
    }

    pub fn push(&mut self, value: SearchDoc) {
        let id = self.inner.len();

        let mut value = value;
        value.insert("id", serde_json::to_value(id).unwrap());

        self.inner.push(value)
    }
}

impl From<Vec<SearchDoc>> for SearchDocs {
    fn from(entries: Vec<SearchDoc>) -> Self {
        let mut docs = Self::new();
        for entry in entries {
            docs.push(entry);
        }
        docs
    }
}

impl Default for SearchDocs {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for SearchDocs {
    type Item = SearchDoc;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a SearchDocs {
    type Item = &'a SearchDoc;
    type IntoIter = std::slice::Iter<'a, SearchDoc>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

#[cfg(test)]
mod test_search_docs {
    use crate::{collection::SearchDocs, SearchDoc};

    fn new_entry(pairs: &[(&str, serde_json::Value)]) -> SearchDoc {
        let mut entry = SearchDoc::new();
        for pair in pairs {
            entry.insert(pair.0, pair.1.clone());
        }
        entry
    }

    fn str_val(s: &str) -> serde_json::Value {
        serde_json::to_value(s).unwrap()
    }

    #[test]
    fn inserts() {
        let entry_1 = new_entry(&[("1", str_val("one")), ("test1", str_val("entry1"))]);
        let entry_2 = new_entry(&[("2", str_val("two")), ("test2", str_val("entry2"))]);

        let mut docs = SearchDocs::default();
        assert!(docs.inner.is_empty());

        docs.push(entry_1);
        docs.push(entry_2);

        assert_eq!(docs.inner.len(), 2);
    }

    #[test]
    fn serialize_format() {
        let entry_1 = new_entry(&[("1", str_val("one")), ("test1", str_val("entry1"))]);
        let entry_2 = new_entry(&[("2", str_val("two")), ("test2", str_val("entry2"))]);

        let mut docs = SearchDocs::default();
        docs.push(entry_1);
        docs.push(entry_2);

        let serialized = serde_json::to_string(&docs).expect("failed to serialize docs");
        assert_eq!(
            serialized,
            include_str!("test/collection-serialize_format.expected")
        );
    }
}

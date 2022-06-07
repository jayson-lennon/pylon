use crate::IndexEntry;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct Index {
    inner: Vec<IndexEntry>,
}

impl Index {
    pub fn new() -> Self {
        Self { inner: vec![] }
    }

    pub fn push(&mut self, value: IndexEntry) {
        let id = self.inner.len();

        let mut value = value;
        value.insert("id", serde_json::to_value(id).unwrap());

        self.inner.push(value)
    }
}

impl From<Vec<IndexEntry>> for Index {
    fn from(entries: Vec<IndexEntry>) -> Self {
        let mut index = Self::new();
        for entry in entries {
            index.push(entry);
        }
        index
    }
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for Index {
    type Item = IndexEntry;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a Index {
    type Item = &'a IndexEntry;
    type IntoIter = std::slice::Iter<'a, IndexEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

#[cfg(test)]
mod test_index {
    use crate::{index::Index, IndexEntry};

    fn new_entry(pairs: &[(&str, serde_json::Value)]) -> IndexEntry {
        let mut entry = IndexEntry::new();
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

        let mut index = Index::default();
        assert!(index.inner.is_empty());

        index.push(entry_1);
        index.push(entry_2);

        assert_eq!(index.inner.len(), 2);
    }

    #[test]
    fn serialize_format() {
        let entry_1 = new_entry(&[("1", str_val("one")), ("test1", str_val("entry1"))]);
        let entry_2 = new_entry(&[("2", str_val("two")), ("test2", str_val("entry2"))]);

        let mut index = Index::default();
        index.push(entry_1);
        index.push(entry_2);

        let serialized = serde_json::to_string(&index).expect("failed to serialize index");
        assert_eq!(
            serialized,
            include_str!("test/index_serialize_format.expected")
        );
    }
}

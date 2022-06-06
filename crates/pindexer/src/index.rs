use crate::IndexEntry;
use std::fmt;

pub struct Index {
    inner: Vec<IndexEntry>,
}

impl Index {
    pub fn new() -> Self {
        Self { inner: vec![] }
    }
    pub fn push(&mut self, value: IndexEntry) {
        self.inner.push(value)
    }
}

impl Default for Index {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for entry in self {
            let entry = entry.as_value().map_err(|_| std::fmt::Error)?;
            writeln!(f, "{}", entry)?;
        }
        Ok(())
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
    fn display() {
        let entry_1 = new_entry(&[("1", str_val("one")), ("test1", str_val("entry1"))]);
        let entry_2 = new_entry(&[("2", str_val("two")), ("test2", str_val("entry2"))]);

        let mut index = Index::default();
        index.push(entry_1);
        index.push(entry_2);

        let result = index.to_string();
        assert_eq!(result, include_str!("test/index_display.expected"));
    }

    #[test]
    fn display_with_list() {
        let entry_1 = new_entry(&[("1", str_val("one")), ("test1", str_val("entry1"))]);
        let entry_2 = new_entry(&[("list", serde_json::to_value(vec!["a", "b", "c"]).unwrap())]);

        let mut index = Index::default();
        index.push(entry_1);
        index.push(entry_2);

        let result = index.to_string();
        assert_eq!(
            result,
            include_str!("test/index_display_with_list.expected")
        );
    }
}

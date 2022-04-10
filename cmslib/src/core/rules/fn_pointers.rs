use slotmap::SlotMap;
use tracing::{instrument, trace};

use crate::core::Uri;

use super::Matcher;

#[derive(Debug, Clone)]
pub struct GlobStore<K, T>
where
    K: slotmap::Key,
    T: Clone,
{
    inner: SlotMap<K, T>,
    matchers: Vec<(Matcher, K)>,
}

impl<K, T> GlobStore<K, T>
where
    K: slotmap::Key,
    T: Clone,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: SlotMap::with_key(),
            matchers: vec![],
        }
    }

    #[instrument(skip_all)]
    pub fn add(&mut self, matcher: Matcher, data: T) {
        trace!("add matcher to fn pointers");
        let key = self.inner.insert(data);
        self.matchers.push((matcher, key));
    }

    #[instrument(skip_all)]
    pub fn find_keys(&self, uri: &Uri) -> Vec<K> {
        self.matchers
            .iter()
            .filter_map(|(matcher, key)| match matcher.is_match(&uri) {
                true => Some(*key),
                false => None,
            })
            .collect()
    }

    pub fn get(&self, key: K) -> Option<T> {
        self.inner.get(key).cloned()
    }
}

impl<K, T> Default for GlobStore<K, T>
where
    K: slotmap::Key,
    T: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::rules::matcher::test::make_matcher;

    #[test]
    fn finds_pointer_keys() {
        slotmap::new_key_type! {
            pub struct TestKey;
        }

        let mut collection = GlobStore::<TestKey, usize>::new();

        let matcher1 = make_matcher(&["/*.txt"]);
        let matcher2 = make_matcher(&["/*.md"]);

        collection.add(matcher1, 1);
        collection.add(matcher2, 2);

        let key1 = collection.find_keys(&Uri::from_path("/test.txt"));
        assert_eq!(key1.len(), 1);

        let key2 = collection.find_keys(&Uri::from_path("/test.md"));
        assert_eq!(key2.len(), 1);

        assert!(key1[0] != key2[0]);
    }

    #[test]
    fn gets_key() {
        slotmap::new_key_type! {
            pub struct TestKey;
        }

        let mut collection = GlobStore::<TestKey, usize>::new();

        let matcher1 = make_matcher(&["/*.txt"]);
        let matcher2 = make_matcher(&["/*.md"]);

        collection.add(matcher1, 1);
        collection.add(matcher2, 2);

        let key1 = collection.find_keys(&Uri::from_path("/test.txt"))[0];
        let ptr1 = collection.get(key1);
        assert!(ptr1.is_some());

        let key2 = collection.find_keys(&Uri::from_path("/test.md"))[0];
        let ptr2 = collection.get(key1);
        assert!(ptr2.is_some());

        let key3 = collection.find_keys(&Uri::from_path("nope"));
        assert!(key3.is_empty());
    }
}

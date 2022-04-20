use std::collections::HashSet;

use crate::core::uri::Uri;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct LinkedAssetSource {
    tag: String,
    src: String,
}

impl LinkedAssetSource {
    pub fn new<S: Into<String>>(tag: S, src: S) -> Self {
        Self {
            tag: tag.into(),
            src: src.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct LinkedAsset {
    modified: bool,
    uri: Uri,
    src: String,
    tag: String,
}

impl LinkedAsset {
    pub fn new_modified<S: Into<String>>(tag: S, src: S, uri: Uri) -> Self {
        Self {
            modified: true,
            uri,
            tag: tag.into(),
            src: src.into(),
        }
    }

    pub fn new_unmodified<S: Into<String>>(tag: S, src: S, uri: Uri) -> Self {
        Self {
            modified: false,
            uri,
            tag: tag.into(),
            src: src.into(),
        }
    }

    pub fn has_tag_name<S: AsRef<str>>(&self, tag: S) -> bool {
        self.tag == tag.as_ref()
    }

    pub fn uri(&self) -> &Uri {
        &self.uri
    }
}

#[derive(Debug)]
pub struct LinkedAssets {
    inner: HashSet<LinkedAsset>,
}

impl LinkedAssets {
    pub fn new() -> Self {
        Self {
            inner: HashSet::new(),
        }
    }
    pub fn from_hashset(assets: HashSet<LinkedAsset>) -> Self {
        Self { inner: assets }
    }

    pub fn iter_uris(&self) -> impl Iterator<Item = &Uri> {
        self.inner.iter().map(|asset| &asset.uri)
    }

    pub fn iter(&self) -> impl Iterator<Item = &LinkedAsset> {
        self.into_iter()
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }
}

impl Default for LinkedAssets {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for LinkedAssets {
    type Item = LinkedAsset;
    type IntoIter = std::collections::hash_set::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a LinkedAssets {
    type Item = &'a LinkedAsset;
    type IntoIter = std::collections::hash_set::Iter<'a, LinkedAsset>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new_linked_assets() {
        let assets = LinkedAssets::new();
        assert!(assets.inner.is_empty());
    }

    #[test]
    fn new_linked_assets_with_default() {
        let assets = LinkedAssets::default();
        assert!(assets.inner.is_empty());
    }

    #[test]
    fn linked_assets_from_hashset() {
        let mut assets = HashSet::new();
        assets.insert(LinkedAsset::new_unmodified(
            "a",
            "/test",
            Uri::from_path("test"),
        ));
        assets.insert(LinkedAsset::new_unmodified(
            "b",
            "/test",
            Uri::from_path("test"),
        ));

        let assets = LinkedAssets::from_hashset(assets);
        assert_eq!(assets.inner.len(), 2);
    }

    #[test]
    fn linked_assets_iter() {
        let mut assets = HashSet::new();
        assets.insert(LinkedAsset::new_unmodified(
            "a",
            "/test",
            Uri::from_path("test"),
        ));
        assets.insert(LinkedAsset::new_unmodified(
            "b",
            "/test",
            Uri::from_path("test"),
        ));

        let assets = LinkedAssets::from_hashset(assets);

        let assets = assets.iter_uris();
        assert_eq!(assets.count(), 2);
    }
}

use std::collections::HashSet;

use crate::core::uri::Uri;

#[derive(Debug)]
pub struct LinkedAssets {
    assets: HashSet<Uri>,
}

impl LinkedAssets {
    pub fn new() -> Self {
        Self {
            assets: HashSet::new(),
        }
    }
    pub fn from_hashset(assets: HashSet<Uri>) -> Self {
        Self { assets }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Uri> {
        self.assets.iter()
    }

    pub fn count(&self) -> usize {
        self.assets.len()
    }
}

impl Default for LinkedAssets {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new_linked_assets() {
        let assets = LinkedAssets::new();
        assert!(assets.assets.is_empty());
    }

    #[test]
    fn new_linked_assets_with_default() {
        let assets = LinkedAssets::default();
        assert!(assets.assets.is_empty());
    }

    #[test]
    fn linked_assets_from_hashset() {
        let mut assets = HashSet::new();
        assets.insert(Uri::from_path("a"));
        assets.insert(Uri::from_path("b"));

        let assets = LinkedAssets::from_hashset(assets);
        assert_eq!(assets.assets.len(), 2);
    }

    #[test]
    fn linked_assets_iter() {
        let mut assets = HashSet::new();
        assets.insert(Uri::from_path("a"));
        assets.insert(Uri::from_path("b"));

        let assets = LinkedAssets::from_hashset(assets);

        let assets = assets.iter();
        assert_eq!(assets.count(), 2);
    }
}

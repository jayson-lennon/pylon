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
}

impl Default for LinkedAssets {
    fn default() -> Self {
        Self::new()
    }
}
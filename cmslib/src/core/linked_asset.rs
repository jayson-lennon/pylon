use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::Serialize;

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

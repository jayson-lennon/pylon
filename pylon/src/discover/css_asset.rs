use std::{collections::HashSet, path::Path};

use crate::{core::Uri, Result};
use anyhow::anyhow;
use parcel_css::stylesheet::{ParserOptions, StyleSheet};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CssAsset {
    uri: Uri,
}

#[derive(Clone, Debug)]
pub struct CssAssets {
    inner: HashSet<CssAsset>,
}
impl CssAssets {
    pub fn new() -> Self {
        Self {
            inner: HashSet::new(),
        }
    }
    pub fn from_hashset(assets: HashSet<CssAsset>) -> Self {
        Self { inner: assets }
    }

    pub fn iter_uris(&self) -> impl Iterator<Item = &Uri> {
        self.inner.iter().map(|asset| &asset.uri)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CssAsset> {
        self.into_iter()
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }
}

impl Default for CssAssets {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for CssAssets {
    type Item = CssAsset;
    type IntoIter = std::collections::hash_set::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a CssAssets {
    type Item = &'a CssAsset;
    type IntoIter = std::collections::hash_set::Iter<'a, CssAsset>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

pub fn find_css_assets<P: AsRef<Path>>(path: P) -> Result<CssAssets> {
    let mut all_assets: HashSet<CssAsset> = HashSet::new();

    let stylesheets = super::get_all_paths(path.as_ref(), &|path: &Path| -> bool {
        path.extension() == Some(std::ffi::OsStr::new("css"))
    })?;

    let options = ParserOptions::default();

    for file in stylesheets {
        let file_content = std::fs::read_to_string(&file)?;
        let sheet = StyleSheet::parse(
            &file.as_path().to_string_lossy(),
            &file_content,
            options.clone(),
        )
        .map_err(|e| anyhow!("error parsing stylesheet: {}", e))?;
        let assets = find_assets(sheet)?;
        all_assets.extend(assets);
    }

    Ok(CssAssets { inner: all_assets })
}

fn find_assets(_sheet: StyleSheet) -> Result<HashSet<CssAsset>> {
    todo!();
}

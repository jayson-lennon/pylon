use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::Path;

use tracing::instrument;

use crate::core::{uri::Uri, RelSystemPath};
use crate::discover::UrlType;
use crate::{discover, util, Result};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct HtmlAsset {
    target: String,
    tag: String,
}

impl HtmlAsset {
    pub fn new<S: Into<String>>(target: S, tag: S) -> Self {
        Self {
            target: target.into(),
            tag: tag.into(),
        }
    }

    pub fn tag(&self) -> &str {
        self.tag.as_str()
    }

    pub fn has_tag<S: AsRef<str>>(&self, tag: S) -> bool {
        self.tag == tag.as_ref()
    }

    pub fn uri(&self) -> Uri {
        Uri::from_path(&self.target)
    }
}

#[derive(Debug)]
pub struct HtmlAssets {
    inner: HashSet<HtmlAsset>,
}

impl HtmlAssets {
    pub fn new() -> Self {
        Self {
            inner: HashSet::new(),
        }
    }
    pub fn from_hashset(assets: HashSet<HtmlAsset>) -> Self {
        Self { inner: assets }
    }

    pub fn iter(&self) -> impl Iterator<Item = &HtmlAsset> {
        self.into_iter()
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }

    pub fn extend(&mut self, other: Self) {
        self.inner.extend(other.inner);
    }

    pub fn insert(&mut self, asset: HtmlAsset) {
        self.inner.insert(asset);
    }
}

impl Default for HtmlAssets {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for HtmlAssets {
    type Item = HtmlAsset;
    type IntoIter = std::collections::hash_set::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a HtmlAssets {
    type Item = &'a HtmlAsset;
    type IntoIter = std::collections::hash_set::Iter<'a, HtmlAsset>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

#[instrument(skip_all)]
pub fn find_all<P: AsRef<Path>>(root: P) -> Result<HtmlAssets> {
    let root = root.as_ref();
    let html_paths =
        discover::get_all_paths(root, &|path| path.extension() == Some(OsStr::new("html")))?;
    let mut all_assets = HtmlAssets::new();

    for path in html_paths {
        let raw_html = std::fs::read_to_string(&path)?;
        let assets = find(&RelSystemPath::new(root, path.as_path()), &raw_html)?;
        all_assets.extend(assets);
    }

    Ok(all_assets)
}

#[instrument(skip(html))]
/// This function rewrites the asset location if applicable
pub fn find<S: AsRef<str>>(page_path: &RelSystemPath, html: S) -> Result<HtmlAssets> {
    use scraper::{Html, Selector};

    let selectors = [
        ("a", "href"),
        ("audio", "src"),
        ("embed", "src"),
        ("img", "src"),
        ("link", "href"),
        ("object", "data"),
        ("script", "src"),
        ("source", "src"),
        ("source", "srcset"),
        ("track", "src"),
        ("video", "src"),
    ];

    let doc = Html::parse_document(html.as_ref());

    let mut assets = HtmlAssets::new();
    for (tag, attr) in selectors {
        let selector = Selector::parse(tag).expect(&format!(
            "Error parsing CSS selector '{}'. This is a bug.",
            tag
        ));
        for el in doc.select(&selector) {
            if let Some(url) = el.value().attr(attr) {
                dbg!(&url);
                match discover::get_url_type(url) {
                    UrlType::Absolute | UrlType::Offsite => {
                        dbg!("absolute");
                        assets.insert(HtmlAsset::new(url, tag));
                    }
                    // relative links need to get converted to absolute links
                    UrlType::Relative(target) => {
                        dbg!("relative");
                        dbg!(&target);
                        dbg!(&page_path);
                        let target = util::rel_to_abs(&target, page_path);
                        assets.insert(HtmlAsset::new(target, tag.to_string()));
                    }
                    UrlType::InternalDoc(_) => panic!(
                        "encountered internal doc link while discovering assets. this is a bug"
                    ),
                }
            }
        }
    }
    Ok(assets)
}

#[cfg(test)]
mod test {
    use crate::core::RelSystemPath;

    macro_rules! pair {
        ($tagname:literal, $tagsrc:literal, $target:ident) => {
            ($tagname.to_string(), $tagsrc.replace("TARGET", $target))
        };
    }

    type TagName = String;
    #[rustfmt::skip]
    fn tags(target: &str) -> Vec<(TagName, String)> {
        vec![
            pair!("a",      r#"<a href="TARGET">"#, target),
            pair!("audio",  r#"<audio src="TARGET">"#, target),
            pair!("embed",  r#"<embed src="TARGET">"#, target),
            pair!("img",    r#"<img src="TARGET">"#, target),
            pair!("link",   r#"<link href="TARGET">"#, target),
            pair!("object", r#"<object data="TARGET">"#, target),
            pair!("script", r#"<script src="TARGET">"#, target),
            pair!("source", r#"<source src="TARGET">"#, target),
            pair!("source", r#"<source srcset="TARGET">"#, target),
            pair!("track",  r#"<track src="TARGET">"#, target),
            pair!("video",  r#"<video src="TARGET">"#, target),
        ]
    }

    #[test]
    fn finds_assets_with_relative_path() {
        let path = RelSystemPath::new("test", "file_path/is/index.html");
        let html = tags("./test.png");
        for (tagname, entry) in html {
            let assets = super::find(&path, &entry).unwrap();
            dbg!(&assets);
            let asset = assets.iter().next().unwrap();
            assert_eq!(asset.target, "/file_path/is/test.png");
            assert_eq!(asset.tag, tagname);
        }
    }

    #[test]
    fn finds_assets_with_absolute_path() {
        let path = RelSystemPath::new("test", "file_path/is/index.html");
        let html = tags("/test.png");
        for (tagname, entry) in html {
            let assets = super::find(&path, &entry).unwrap();
            let asset = assets.iter().next().unwrap();
            assert_eq!(asset.target, "/test.png");
            assert_eq!(asset.tag, tagname);
        }
    }
}

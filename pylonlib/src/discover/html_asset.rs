use derivative::Derivative;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;
use typed_uri::{CheckedUri, Uri};

use std::sync::Arc;

use serde::Serialize;
use tracing::instrument;

use crate::core::engine::EnginePaths;
use crate::{discover, pathmarker, CheckedFile, CheckedFilePath, RelPath, Result, SysPath};

use crate::discover::AssetPath;

use super::UrlType;

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone)]
pub struct HtmlAsset {
    target: AssetPath,
    tag: String,
    url_type: UrlType,
    html_src_file: CheckedFilePath<pathmarker::Html>,
}

impl HtmlAsset {
    pub fn new<S: Into<String>>(
        target: &AssetPath,
        tag: S,
        url_type: &UrlType,
        html_src_file: &CheckedFilePath<pathmarker::Html>,
    ) -> Self {
        Self {
            target: target.clone(),
            tag: tag.into(),
            url_type: url_type.clone(),
            html_src_file: html_src_file.clone(),
        }
    }

    pub fn tag(&self) -> &str {
        self.tag.as_str()
    }

    pub fn has_tag<S: AsRef<str>>(&self, tag: S) -> bool {
        self.tag == tag.as_ref()
    }

    pub fn uri(&self) -> &CheckedUri {
        self.target.uri()
    }

    pub fn html_src_file(&self) -> &CheckedFilePath<pathmarker::Html> {
        &self.target.html_src_file()
    }

    pub fn url_type(&self) -> &UrlType {
        &self.url_type
    }

    pub fn path(&self) -> &AssetPath {
        &self.target
    }
}

impl std::hash::Hash for HtmlAsset {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.target.uri().as_str().hash(state)
    }
}

impl PartialEq for HtmlAsset {
    fn eq(&self, other: &Self) -> bool {
        self.target.uri().as_str() == other.target.uri().as_str()
    }
}

impl Eq for HtmlAsset {}

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

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn extend(&mut self, other: Self) {
        self.inner.extend(other.inner);
    }

    pub fn insert(&mut self, asset: HtmlAsset) {
        self.inner.insert(asset);
    }

    pub fn drop_offsite(&mut self) {
        let assets = self.inner.drain().collect::<HashSet<_>>();
        for asset in assets {
            if asset.url_type != UrlType::Offsite {
                self.inner.insert(asset);
            }
        }
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

impl FromIterator<HtmlAsset> for HtmlAssets {
    fn from_iter<I: IntoIterator<Item = HtmlAsset>>(iter: I) -> Self {
        let mut assets = HtmlAssets::new();
        for asset in iter {
            assets.insert(asset);
        }
        assets
    }
}

impl<'a> FromIterator<&'a HtmlAsset> for HtmlAssets {
    fn from_iter<I: IntoIterator<Item = &'a HtmlAsset>>(iter: I) -> Self {
        let mut assets = HtmlAssets::new();
        for asset in iter {
            assets.insert(asset.clone());
        }
        assets
    }
}

pub fn find_all(engine_paths: Arc<EnginePaths>, search_dir: &RelPath) -> Result<HtmlAssets> {
    let html_paths =
        discover::get_all_paths(&engine_paths.project_root().join(search_dir), &|path| {
            path.extension() == Some(OsStr::new("html"))
        })?;
    let mut all_assets = HtmlAssets::new();

    for abs_path in html_paths {
        let raw_html = std::fs::read_to_string(&abs_path)?;
        let html_path = SysPath::from_abs_path(
            &abs_path,
            engine_paths.project_root(),
            engine_paths.output_dir(),
        )?
        .to_checked_file()?;
        let assets = find(engine_paths.clone(), &html_path, &raw_html)?;
        all_assets.extend(assets);
    }

    Ok(all_assets)
}

/// This function rewrites the asset location if applicable
pub fn find<S>(
    engine_paths: Arc<EnginePaths>,
    html_path: &CheckedFilePath<pathmarker::Html>,
    html: S,
) -> Result<HtmlAssets>
where
    S: AsRef<str> + std::fmt::Debug,
{
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
                match discover::get_url_type(url) {
                    UrlType::Absolute => {
                        let uri = Uri::new(url)?;
                        let uri = CheckedUri::new(html_path, &uri);
                        let asset_path = AssetPath::new(engine_paths.clone(), &uri)?;
                        let html_asset =
                            HtmlAsset::new(&asset_path, tag, &UrlType::Absolute, html_path);
                        assets.insert(html_asset);
                    }
                    UrlType::Offsite => {
                        // assets.insert(HtmlAsset::new(url, tag, &UrlType::Offsite, page_path));
                    }
                    // relative links need to get converted to absolute links
                    UrlType::Relative(target) => {
                        let uri = relative_uri_to_absolute_uri(html_path, &target);
                        let asset_path = AssetPath::new(engine_paths.clone(), &uri)?;
                        let html_asset =
                            HtmlAsset::new(&asset_path, tag, &UrlType::Relative(target), html_path);
                        assets.insert(html_asset);
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

pub fn relative_uri_to_absolute_uri<S: AsRef<str>>(
    html_path: &CheckedFilePath<pathmarker::Html>,
    relative_uri: S,
) -> CheckedUri {
    let relative_uri = relative_uri.as_ref();

    let mut abs_uri = PathBuf::new();

    if relative_uri.starts_with('/') {
        abs_uri.push(&relative_uri);
    } else {
        abs_uri.push("/");
        abs_uri.push(html_path.as_sys_path().pop().target());
        abs_uri.push(&relative_uri);
    }

    let uri = Uri::new(abs_uri.to_string_lossy().to_string()).unwrap();
    CheckedUri::new(html_path, &uri)
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

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

    // #[test]
    // fn finds_assets_in_single_file() {
    //     let tree = temptree! {
    //       "rules.rhai": "",
    //       templates: {},
    //       target: {
    //           file_path: {
    //               is: {
    //                   "index.html": "",
    //               },
    //           },
    //       },
    //       src: {},
    //       syntax_themes: {}
    //     };
    //     let paths = crate::test::default_test_paths(&tree);

    //     let html_path =
    //         HtmlFilePath::new(paths.clone(), rel!("target/file_path/is/index.html")).unwrap();
    //     let html = tags("test.png");
    //     for (tagname, entry) in html {
    //         let assets = super::find(paths.clone(), &html_path, entry).unwrap();
    //         let asset = assets.iter().next().unwrap();
    //         assert_eq!(asset.uri().as_str(), "/file_path/is/test.png");
    //         assert_eq!(asset.tag(), tagname);
    //         assert_eq!(asset.html_path(), &html_path);
    //     }
    // }

    // #[test]
    // fn finds_assets_in_multiple_files() {
    //     let tree = temptree! {
    //       "rules.rhai": "",
    //       templates: {},
    //       target: {
    //           "1.html": r#"<img src="test1.png">"#,
    //           "2.html": r#"<img src="inner/test2.png">"#,
    //       },
    //       src: {},
    //       syntax_themes: {}
    //     };
    //     let paths = crate::test::default_test_paths(&tree);

    //     let assets = super::find_all(paths, rel!("target")).expect("failed to find assets");

    //     assert_eq!(assets.len(), 2);

    //     for asset in &assets {
    //         if !(asset.uri().as_str() == "/test1.png" || asset.uri().as_str() == "/inner/test2.png")
    //         {
    //             panic!("wrong assets in collection");
    //         }
    //     }
    // }
}

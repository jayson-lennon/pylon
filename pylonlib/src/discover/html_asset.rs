use derivative::Derivative;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;
use typed_uri::{BasedUri, Uri};

use std::sync::Arc;

use serde::Serialize;
use thiserror::Error;


use crate::core::engine::EnginePaths;
use crate::{discover, pathmarker, CheckedFile, CheckedFilePath, RelPath, Result, SysPath};

use crate::discover::AssetPath;

use super::UrlType;

#[derive(Error, Debug)]
#[error("missing assets: {missing:?}")]
pub struct MissingAssetsError {
    missing: Vec<Uri>,
}

impl FromIterator<Uri> for MissingAssetsError {
    fn from_iter<I: IntoIterator<Item = Uri>>(iter: I) -> Self {
        let mut uris = vec![];
        for uri in iter {
            uris.push(uri);
        }
        Self { missing: uris }
    }
}

impl<'a> FromIterator<&'a Uri> for MissingAssetsError {
    fn from_iter<I: IntoIterator<Item = &'a Uri>>(iter: I) -> Self {
        let mut uris = vec![];
        for uri in iter {
            uris.push(uri.clone());
        }
        Self { missing: uris }
    }
}

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

    pub fn uri(&self) -> &BasedUri {
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
                        let uri = BasedUri::new(html_path, &uri);
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
                        let uri = raw_relative_uri_to_based_uri(html_path, &target);
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

fn raw_relative_uri_to_based_uri<S: AsRef<str>>(
    html_path: &CheckedFilePath<pathmarker::Html>,
    relative_uri: S,
) -> BasedUri {
    let relative_uri = relative_uri.as_ref();

    let mut abs_uri = PathBuf::new();

    // If we get an absolute Uri at this point in the program, we will
    // just assume that it is correct since we can create a CheckedUri
    // from an absolute Uri. Leave this `debug_assert` so tests will fail
    // if we ever use an absolute Uri as this may indicate a problem
    // with the caller.
    debug_assert!(!relative_uri.starts_with('/'));
    if relative_uri.starts_with('/') {
        abs_uri.push(&relative_uri);
    } else {
        abs_uri.push("/");
        abs_uri.push(html_path.as_sys_path().pop().target());
        abs_uri.push(&relative_uri);
    }

    let uri = Uri::new(abs_uri.to_string_lossy().to_string()).unwrap();
    BasedUri::new(html_path, &uri)
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use crate::test::{abs, rel};
    use temptree::temptree;
    use typed_path::SysPath;

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
    fn finds_all_tags() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let html = r#"
            <a href="/a.file"></a>
            <audio src="/audio.file"></audio>
            <embed src="/embed.file" />
            <img src="/img.file" />
            <link href="/link.file">
            <object data="/object.file"></object>
            <script src="/script.file"></script>
            <source src="/source.file">
            <source srcset="/sourceset.file">
            <track src="/track.file" />
            <video src="/video.file"></video>
"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 11);
    }

    #[test]
    fn finds_absolute_uri_assets() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
            "asset.png": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let html = r#"<img src="/asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 1);

        let asset = assets.iter().next().unwrap();

        assert_eq!(asset.target.uri().as_str(), "/asset.png");
    }

    #[test]
    fn finds_relative_uri_assets() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
            "asset.png": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let html = r#"<img src="asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 1);

        let asset = assets.iter().next().unwrap();

        assert_eq!(asset.target.uri().as_str(), "/asset.png");
    }

    #[test]
    fn finds_relative_uri_assets_in_subdir() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
            img: {
                "asset.png": "",
            },
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let html = r#"<img src="img/asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 1);

        let asset = assets.iter().next().unwrap();

        assert_eq!(asset.target.uri().as_str(), "/img/asset.png");
    }

    #[test]
    fn finds_absolute_uri_assets_in_subdir() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
            img: {
                "asset.png": "",
            },
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let html = r#"<img src="/img/asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 1);

        let asset = assets.iter().next().unwrap();

        assert_eq!(asset.target.uri().as_str(), "/img/asset.png");
    }

    #[test]
    fn ignores_offsite_assets() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let html = r#"<img src="http://example.com/asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 0);
    }

    #[test]
    #[should_panic]
    fn internal_doc_link_abort() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let html = r#"<img src="@/whoops.md">"#;
        super::find(paths, &html_path, html);
    }

    #[test]
    fn converts_raw_relative_to_based_uri() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
            "test.txt": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let relative_uri = "test.txt";
        let uri = super::raw_relative_uri_to_based_uri(&html_path, relative_uri);
        assert_eq!(uri.as_str(), "/test.txt");
    }

    #[test]
    #[should_panic]
    fn converting_raw_relative_uri_to_based_uri_fails_if_given_absolute_uri() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "test.html": "",
            "test.txt": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("test.html"))
            .try_into()
            .unwrap();
        let relative_uri = "/test.txt";
        super::raw_relative_uri_to_based_uri(&html_path, relative_uri);
    }
}

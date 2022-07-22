use derivative::Derivative;
use eyre::{eyre, WrapErr};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use typed_path::{AbsPath, ConfirmedPath};
use typed_uri::{AssetUri, Uri};

use serde::Serialize;
use thiserror::Error;

use crate::core::engine::GlobalEnginePaths;
use crate::{discover, RelPath, Result, SysPath};

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
    html_src_file: ConfirmedPath<pathmarker::HtmlFile>,
}

impl HtmlAsset {
    pub fn new<S: Into<String>>(
        target: &AssetPath,
        tag: S,
        url_type: &UrlType,
        html_src_file: &ConfirmedPath<pathmarker::HtmlFile>,
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

    pub fn asset_target_uri(&self) -> &AssetUri {
        self.target.uri()
    }

    pub fn html_src_file(&self) -> &ConfirmedPath<pathmarker::HtmlFile> {
        self.target.html_src_file()
    }

    pub fn url_type(&self) -> &UrlType {
        &self.url_type
    }

    pub fn asset_target_path(&self) -> &AssetPath {
        &self.target
    }
}

impl std::hash::Hash for HtmlAsset {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.target.uri().as_str().hash(state);
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
    inner: HashMap<AbsPath, Vec<HtmlAsset>>,
}

impl HtmlAssets {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
    pub fn from_hashmap(assets: HashMap<AbsPath, Vec<HtmlAsset>>) -> Self {
        Self { inner: assets }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&AbsPath, &Vec<HtmlAsset>)> {
        self.into_iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn extend(&mut self, other: Self) {
        for (key, other_assets) in other {
            let this_assets = self.inner.entry(key).or_default();
            for asset in other_assets {
                this_assets.push(asset);
            }
        }
    }

    pub fn insert(&mut self, asset: HtmlAsset) {
        let key = asset.asset_target_path().target().clone();
        let entry = self.inner.entry(key).or_default();
        entry.push(asset);
    }

    pub fn drop_offsite(&mut self) {
        let mut new_assets = HashMap::new();
        for (k, assets) in self.inner.drain() {
            let filtered_assets = assets
                .into_iter()
                .filter(|asset| asset.url_type != UrlType::Offsite)
                .collect::<Vec<_>>();
            new_assets.insert(k, filtered_assets);
        }
        self.inner = new_assets;
    }

    pub fn remove(&mut self, target: &AbsPath) {
        self.inner.remove(target);
    }
}

impl Default for HtmlAssets {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for HtmlAssets {
    type Item = (AbsPath, Vec<HtmlAsset>);
    type IntoIter = std::collections::hash_map::IntoIter<AbsPath, Vec<HtmlAsset>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a HtmlAssets {
    type Item = (&'a AbsPath, &'a Vec<HtmlAsset>);
    type IntoIter = std::collections::hash_map::Iter<'a, AbsPath, Vec<HtmlAsset>>;

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

impl FromIterator<Vec<HtmlAsset>> for HtmlAssets {
    fn from_iter<I: IntoIterator<Item = Vec<HtmlAsset>>>(iter: I) -> Self {
        let mut assets = HtmlAssets::new();
        for collection in iter {
            for asset in collection {
                assets.insert(asset);
            }
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

#[allow(clippy::needless_pass_by_value)]
pub fn find_all(engine_paths: GlobalEnginePaths, search_dir: &RelPath) -> Result<HtmlAssets> {
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
        .confirm(pathmarker::HtmlFile)?;
        let assets = find(engine_paths.clone(), &html_path, &raw_html)?;
        all_assets.extend(assets);
    }

    Ok(all_assets)
}

#[allow(clippy::needless_pass_by_value)]
pub fn find<S>(
    engine_paths: GlobalEnginePaths,
    html_path: &ConfirmedPath<pathmarker::HtmlFile>,
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
        ("iframe", "src"),
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
        let selector = Selector::parse(tag)
            .map_err(|e| eyre!("selector error: {:?}", e))
            .wrap_err_with(|| format!("Error parsing CSS selector '{}' (this is a bug)", tag))?;
        for el in doc.select(&selector) {
            if let Some(url) = el.value().attr(attr) {
                match discover::get_url_type(url) {
                    UrlType::Absolute => {
                        if url.contains('#') {
                            continue;
                        }
                        let uri = Uri::new(url, url)?;
                        let uri = AssetUri::new(html_path, &uri);
                        let asset_path = AssetPath::new(engine_paths.clone(), &uri)?;
                        let html_asset =
                            HtmlAsset::new(&asset_path, tag, &UrlType::Absolute, html_path);
                        assets.insert(html_asset);
                    }
                    // TODO: make sure the anchor exists in the page
                    UrlType::LocalAnchor(_) => (),
                    // TODO: add this to the link checker once it exists
                    UrlType::Offsite => (),
                    // relative links need to get converted to absolute links
                    UrlType::Relative(target) => {
                        if url.contains('#') {
                            continue;
                        }
                        let uri = canonicalized_uri_from_html_path(html_path, &target);
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

fn canonicalized_uri_from_html_path<S: AsRef<str>>(
    html_path: &ConfirmedPath<pathmarker::HtmlFile>,
    relative_uri: S,
) -> AssetUri {
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

    let uri = Uri::new(abs_uri.to_string_lossy().to_string(), relative_uri).unwrap();
    AssetUri::new(html_path, &uri)
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
            pair!("iframe", r#"<iframe src="TARGET">"#, target),
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
            .confirm(pathmarker::HtmlFile)
            .unwrap();
        let html = r#"
            <a href="/a.file"></a>
            <audio src="/audio.file"></audio>
            <embed src="/embed.file" />
            <iframe src="/iframe.file"></iframe>
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

        assert_eq!(assets.len(), 12);
    }

    #[test]
    fn ignore_name_links() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
            "page.html": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = crate::test::default_test_paths(&tree);
        let html_path = SysPath::new(abs!(tree.path()), rel!("target"), rel!("page.html"))
            .confirm(pathmarker::HtmlFile)
            .unwrap();
        let html = r#"<a href="page.html#test">link</a>"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 0);
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
            .confirm(pathmarker::HtmlFile)
            .unwrap();
        let html = r#"<img src="/asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 1);

        let asset = assets.iter().next().unwrap();

        assert_eq!(asset.1[0].target.uri().as_str(), "/asset.png");
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
            .confirm(pathmarker::HtmlFile)
            .unwrap();
        let html = r#"<img src="asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 1);

        let asset = assets.iter().next().unwrap();

        assert_eq!(asset.1[0].target.uri().as_str(), "/asset.png");
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
            .confirm(pathmarker::HtmlFile)
            .unwrap();
        let html = r#"<img src="img/asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 1);

        let asset = assets.iter().next().unwrap();

        assert_eq!(asset.1[0].target.uri().as_str(), "/img/asset.png");
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
            .confirm(pathmarker::HtmlFile)
            .unwrap();
        let html = r#"<img src="/img/asset.png">"#;
        let assets = super::find(paths, &html_path, html).expect("failed to find assets");

        assert_eq!(assets.len(), 1);

        let asset = assets.iter().next().unwrap();

        assert_eq!(asset.1[0].target.uri().as_str(), "/img/asset.png");
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
            .confirm(pathmarker::HtmlFile)
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
            .confirm(pathmarker::HtmlFile)
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
            .confirm(pathmarker::HtmlFile)
            .unwrap();
        let relative_uri = "test.txt";
        let uri = super::canonicalized_uri_from_html_path(&html_path, relative_uri);
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
            .confirm(pathmarker::HtmlFile)
            .unwrap();
        let relative_uri = "/test.txt";
        super::canonicalized_uri_from_html_path(&html_path, relative_uri);
    }
}

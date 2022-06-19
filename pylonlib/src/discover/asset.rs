use derivative::Derivative;
use typed_path::ConfirmedPath;
use typed_uri::AssetUri;

use crate::core::engine::GlobalEnginePaths;
use crate::Result;
use crate::{AbsPath, RelPath};

use serde::Serialize;

#[derive(Derivative, Serialize)]
#[derivative(Debug, Clone, Hash, PartialEq)]
pub struct AssetPath {
    target: AbsPath,
    uri: AssetUri,
}

impl AssetPath {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(engine_paths: GlobalEnginePaths, uri: &AssetUri) -> Result<Self> {
        let target = RelPath::new(&uri.as_str()[1..])?;
        let target = engine_paths.absolute_output_dir().join(&target);

        Ok(Self {
            target,
            uri: uri.clone(),
        })
    }

    pub fn html_src_file(&self) -> &ConfirmedPath<pathmarker::HtmlFile> {
        self.uri.html_src()
    }

    pub fn uri(&self) -> &AssetUri {
        &self.uri
    }

    pub fn target(&self) -> &AbsPath {
        &self.target
    }
}

impl Eq for AssetPath {}

#[cfg(test)]
mod test {
    #![allow(warnings, unused)]

    use crate::test::{abs, rel};
    use temptree::temptree;
    use typed_path::SysPath;

    #[test]
    fn gets_html_src_file() {
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
        let assets = crate::discover::html_asset::find(paths, &html_path, html)
            .expect("failed to find assets");

        let asset = assets.iter().next().unwrap();
        assert_eq!(asset.html_src_file(), &html_path);
    }
}

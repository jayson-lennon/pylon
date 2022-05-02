use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{core::engine::EnginePaths, Result};
use anyhow::{anyhow, Context};

macro_rules! impl_engine_paths {
    ($field:ident) => {
        pub fn engine_paths(&self) -> Arc<EnginePaths> {
            self.$field.engine_paths()
        }
    };
}

macro_rules! impl_to_path_buf {
    ($type:ty, $field:ident) => {
        impl From<$type> for PathBuf {
            fn from(path: $type) -> Self {
                path.to_path_buf()
            }
        }
        impl $type {
            pub fn to_path_buf(&self) -> PathBuf {
                self.$field.to_path_buf()
            }
        }
    };
}

#[derive(Debug, Clone)]
pub struct SysPath {
    base: PathBuf,
    target: PathBuf,
    engine_paths: Arc<EnginePaths>,
}

impl SysPath {
    pub fn new<B, P>(engine_paths: Arc<EnginePaths>, base: B, target: P) -> Result<Self>
    where
        B: AsRef<Path>,
        P: AsRef<Path>,
    {
        let base = base.as_ref();
        let target = target.as_ref();

        dbg!(&engine_paths);
        dbg!(&engine_paths.project_root());
        dbg!(&base);
        dbg!(&target);
        let target = target.strip_prefix(&base).with_context(|| {
            format!(
                "unable to create a sys path when base '{}' is not present in file path '{}'",
                base.display(),
                target.display()
            )
        })?;

        Ok(Self {
            base: base.to_path_buf(),
            target: target.to_path_buf(),
            engine_paths,
        })
    }

    pub fn engine_paths(&self) -> Arc<EnginePaths> {
        Arc::clone(&self.engine_paths)
    }

    pub fn with_base<P: Into<PathBuf>>(&self, base: P) -> Self {
        let base = base.into();
        Self {
            base,
            target: self.target.clone(),
            engine_paths: Arc::clone(&self.engine_paths),
        }
    }

    pub fn with_extension<S: AsRef<str>>(&self, extension: S) -> Self {
        let extension: &str = extension.as_ref();
        let mut target = self.target.clone();
        target.set_extension(extension);
        Self {
            base: self.base.clone(),
            target,
            engine_paths: Arc::clone(&self.engine_paths),
        }
    }

    pub fn file_name(&self) -> &OsStr {
        debug_assert!(self.target.file_name().is_some());
        self.target.file_name().unwrap()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self)
    }

    pub fn exists(&self) -> bool {
        self.to_path_buf().exists()
    }
}

impl From<SysPath> for PathBuf {
    fn from(path: SysPath) -> Self {
        let mut new_path = path.engine_paths().project_root().to_path_buf();
        new_path.push(path.base);
        new_path.push(path.target);
        new_path
    }
}

impl From<&SysPath> for PathBuf {
    fn from(path: &SysPath) -> Self {
        let mut new_path = path.engine_paths().project_root().to_path_buf();
        new_path.push(path.base.clone());
        new_path.push(path.target.clone());
        new_path
    }
}

#[derive(Debug, Clone)]
pub struct MarkdownPath {
    inner: SysPath,
}

impl MarkdownPath {
    pub fn new<P: AsRef<Path>>(engine_paths: Arc<EnginePaths>, file_path: P) -> Result<Self> {
        let file_path = file_path.as_ref();

        if file_path.file_name().is_none() {
            return Err(anyhow!(
                "no file name present in file path: {}",
                file_path.display()
            ));
        }

        Ok(Self {
            inner: SysPath::new(engine_paths.clone(), engine_paths.src_root(), file_path)?,
        })
    }

    impl_engine_paths!(inner);
}

impl_to_path_buf!(MarkdownPath, inner);

impl TryFrom<HtmlPath> for MarkdownPath {
    type Error = anyhow::Error;
    fn try_from(html_path: HtmlPath) -> Result<Self> {
        if let Some(markdown_path) = html_path.src_file {
            Ok(markdown_path)
        } else {
            let engine_paths = html_path.engine_paths();
            let md_file_name = html_path.inner.with_extension("md");
            let md_file_name = md_file_name.file_name();
            let candidate = engine_paths
                .project_root()
                .join(engine_paths.src_root())
                .join(md_file_name);

            if candidate.exists() {
                let src_path = engine_paths.src_root().join(md_file_name);
                MarkdownPath::new(html_path.engine_paths(), src_path)
            } else {
                Err(anyhow!(
                    "no markdown path associated with {} and unable to find markdown file at {}",
                    html_path.to_path_buf().display(),
                    candidate.display()
                ))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct HtmlPath {
    inner: SysPath,
    src_file: Option<MarkdownPath>,
}

impl HtmlPath {
    pub fn new<P: AsRef<Path>>(engine_paths: Arc<EnginePaths>, file_path: P) -> Result<Self> {
        let file_path = file_path.as_ref();
        if file_path.file_name().is_none() {
            return Err(anyhow!(
                "no file name present in file path: {}",
                file_path.display()
            ));
        }

        let sys_path = SysPath::new(engine_paths.clone(), engine_paths.output_root(), file_path)?;
        dbg!(&sys_path);
        if !sys_path.exists() {
            return Err(anyhow!(
                "attempt to make new HtmlPath from nonexistent file: {}",
                sys_path.to_path_buf().display()
            ));
        }

        Ok(Self {
            inner: sys_path,
            src_file: None,
        })
    }

    impl_engine_paths!(inner);
}

impl_to_path_buf!(HtmlPath, inner);

impl From<MarkdownPath> for HtmlPath {
    fn from(md_path: MarkdownPath) -> Self {
        let engine_paths = md_path.engine_paths();
        let new_path = md_path
            .inner
            .with_base(engine_paths.output_root())
            .with_extension("html");
        HtmlPath {
            inner: new_path,
            src_file: Some(md_path),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RelativeUri {
    uri: String,
    initiator: HtmlPath,
}

impl RelativeUri {
    pub fn new<S: Into<String>>(initiator: &HtmlPath, uri: S) -> Result<Self> {
        let uri = uri.into();
        if uri.starts_with('/') {
            return Err(anyhow!(
                "cannot create new relative URI from an absolute URI"
            ));
        }
        if uri.trim().is_empty() {
            return Err(anyhow!("no URI provided"));
        }
        Ok(Self {
            uri,
            initiator: initiator.clone(),
        })
    }
}

pub struct AbsoluteUri {
    uri: String,
    initiator: HtmlPath,
}
impl AbsoluteUri {
    pub fn new<S: Into<String>>(initiator: &HtmlPath, uri: S) -> Result<Self> {
        let uri = uri.into();
        if !uri.starts_with('/') {
            return Err(anyhow!(
                "cannot create new absolute URI from a relative URI"
            ));
        }
        if uri.len() < 2 {
            return Err(anyhow!("no URI provided"));
        }
        Ok(Self {
            uri,
            initiator: initiator.clone(),
        })
    }
}

pub struct WorkingDir;

pub struct AssetTargetPath;

#[cfg(test)]
mod test {
    #![allow(unused_variables)]

    use std::sync::Arc;
    use tempfile::TempDir;
    use temptree::temptree;

    use crate::core::engine::EnginePaths;

    use super::*;

    fn default_test_paths(tree: &TempDir) -> Arc<EnginePaths> {
        Arc::new(EnginePaths {
            rule_script: PathBuf::from("rules.rhai"),
            src_root: PathBuf::from("src"),
            syntax_theme_root: PathBuf::from("syntax_themes"),
            output_root: PathBuf::from("target"),
            template_root: PathBuf::from("templates"),
            project_root: tree.path().to_path_buf(),
        })
    }

    // `TempDir` needs to stay bound in order to maintain temporary directory tree
    fn simple_init() -> (Arc<EnginePaths>, TempDir) {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {},
          src: {},
          syntax_themes: {}
        };
        let paths = default_test_paths(&tree);

        (paths, tree)
    }

    #[test]
    fn makes_markdown_path_with_valid_paths() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md")
            .expect("should be able to make a markdown path");
    }

    #[test]
    fn markdown_path_creation_fails_when_outside_src() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "outside/page.md");
        assert!(md_path.is_err());
    }

    #[test]
    fn htmlpath_from_markdownpath() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);
        assert_eq!(
            html_path.to_path_buf(),
            tree.path().join("target/page.html")
        );
        assert!(html_path.src_file.is_some());
    }

    #[test]
    fn make_new_htmlpath_when_file_exists() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
              "page.html": ""
          },
          src: {},
          syntax_themes: {}
        };
        let paths = default_test_paths(&tree);
        let html_path =
            HtmlPath::new(paths, "target/page.html").expect("failed to make new html path");
        assert_eq!(
            html_path.to_path_buf(),
            tree.path().join("target/page.html")
        );
        assert!(html_path.src_file.is_none());
    }

    #[test]
    fn fails_to_make_new_htmlpath_when_file_does_not_exists() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {},
          src: {},
          syntax_themes: {}
        };
        let paths = default_test_paths(&tree);
        let html_path = HtmlPath::new(paths, "target/page.html");
        assert!(html_path.is_err());
    }

    #[test]
    fn markdownpath_tryfrom_htmlpath_fails_when_markdown_file_doesnt_exist() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
              "page.html": "",
          },
          src: {},
          syntax_themes: {}
        };
        let paths = default_test_paths(&tree);
        let html_path = HtmlPath::new(paths, "target/page.html").unwrap();
        let new_md_path = MarkdownPath::try_from(html_path);
        assert!(new_md_path.is_err());
    }

    #[test]
    fn markdownpath_tryfrom_htmlpath_when_markdown_file_exists() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
              "page.html": "",
          },
          src: {
              "page.md": "",
          },
          syntax_themes: {}
        };
        let paths = default_test_paths(&tree);

        let html_path = HtmlPath::new(paths, "target/page.html").unwrap();
        let new_md_path =
            MarkdownPath::try_from(html_path).expect("failed to create new markdown path");
    }

    #[test]
    fn markdownpath_tryfrom_htmlpath_when_option_is_some() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path.clone());
        let new_md_path = MarkdownPath::try_from(html_path)
            .expect("failed to convert htmlpath back to markdown path");
        assert_eq!(new_md_path.to_path_buf(), md_path.to_path_buf());
    }

    #[test]
    fn makes_new_relative_uri() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);

        let relative_uri =
            RelativeUri::new(&html_path, "img/sample.png").expect("failed to make relative URI");
    }

    #[test]
    fn relative_uri_fails_if_using_absolute_uri() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);

        let relative_uri = RelativeUri::new(&html_path, "/img/sample.png");
        assert!(relative_uri.is_err());
    }

    #[test]
    fn relative_uri_fails_if_empty() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);

        let relative_uri = RelativeUri::new(&html_path, "");
        assert!(relative_uri.is_err());
    }

    #[test]
    fn makes_new_absolute_uri() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);

        let absolute_uri =
            AbsoluteUri::new(&html_path, "/img/sample.png").expect("failed to make absolute URI");
    }

    #[test]
    fn absolute_uri_fails_if_using_relative_uri() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);

        let absolute_uri = AbsoluteUri::new(&html_path, "img/sample.png");
        assert!(absolute_uri.is_err());
    }

    #[test]
    fn absolute_uri_fails_if_blank() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);

        let absolute_uri = AbsoluteUri::new(&html_path, "");
        assert!(absolute_uri.is_err());
    }

    #[test]
    fn absolute_uri_fails_if_nothing_after_slash() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths, "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);

        let absolute_uri = AbsoluteUri::new(&html_path, "/");
        assert!(absolute_uri.is_err());
    }
}

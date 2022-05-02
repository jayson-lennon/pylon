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

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self)
    }
}

impl From<SysPath> for PathBuf {
    fn from(path: SysPath) -> Self {
        let mut new_path = path.base;
        new_path.push(path.target);
        new_path
    }
}

impl From<&SysPath> for PathBuf {
    fn from(path: &SysPath) -> Self {
        let mut new_path = path.base.clone();
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
            Err(anyhow!(
                "no markdown path associated with {}",
                html_path.to_path_buf().display()
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub struct HtmlPath {
    inner: SysPath,
    src_file: Option<MarkdownPath>,
}

impl HtmlPath {
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

pub struct RelativeUri;

pub struct AbsoluteUri;

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
        let md_path = MarkdownPath::new(paths.clone(), "src/page.md")
            .expect("should be able to make a markdown path");
    }

    #[test]
    fn markdown_path_creation_fails_when_outside_src() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths.clone(), "outside/page.md");
        assert!(md_path.is_err());
    }

    #[test]
    fn htmlpath_from_markdownpath() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths.clone(), "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path);
        assert_eq!(html_path.to_path_buf(), PathBuf::from("target/page.html"));
        assert!(html_path.src_file.is_some());
    }

    #[test]
    fn markdownpath_tryfrom_htmlpath() {
        let (paths, tree) = simple_init();
        let md_path = MarkdownPath::new(paths.clone(), "src/page.md").unwrap();
        let html_path = HtmlPath::from(md_path.clone());
        let new_md_path = MarkdownPath::try_from(html_path)
            .expect("failed to convert htmlpath back to markdown path");
        assert_eq!(new_md_path.to_path_buf(), md_path.to_path_buf());
    }
}

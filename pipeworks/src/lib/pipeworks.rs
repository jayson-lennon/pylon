pub mod discover;
pub mod pipeline;
pub mod render;
pub use pipeline::Pipeline;

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FrontMatterError {
    #[error("parse error: {0}")]
    Error(#[from] toml::de::Error),
}

#[derive(Clone, Debug)]
pub struct RenderedMarkdown(pub String);

#[derive(Clone, Debug)]
pub struct RawMarkdown(pub String);

impl AsRef<str> for RawMarkdown {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl RawMarkdown {
    pub fn new<M: AsRef<str>>(markdown: M) -> Self {
        Self(markdown.as_ref().to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawFrontMatter(pub String);

impl AsRef<str> for RawFrontMatter {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl TryInto<FrontMatter> for RawFrontMatter {
    type Error = FrontMatterError;
    fn try_into(self) -> Result<FrontMatter, Self::Error> {
        Ok(toml::from_str(&self.0)?)
    }
}

impl RawFrontMatter {
    pub fn new<F: AsRef<str>>(frontmatter: F) -> Self {
        Self(frontmatter.as_ref().to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FrontMatter {
    title: String,
    template: String,
}

#[derive(Clone, Debug)]
pub struct Page {
    content: RawMarkdown,
    frontmatter: FrontMatter,
}

#[derive(Clone, Debug)]
pub struct Directories {
    src: PathBuf,
    out: PathBuf,
}

impl Directories {
    pub fn new<P: AsRef<Path>>(src: P, out: P) -> Self {
        Self {
            src: src.as_ref().into(),
            out: out.as_ref().into(),
        }
    }
    pub fn abs_src_dir(&self) -> &Path {
        self.src.as_path()
    }

    pub fn abs_output_dir(&self) -> &Path {
        self.out.as_path()
    }

    /// Returns the absolute path to a source asset, given a partial asset path.
    ///
    /// # Example:
    ///
    /// ```
    /// use std::path::Path;
    /// use pipeworks::Directories;
    ///
    /// let dirs = Directories::new(Path::new("content"), Path::new("public"));
    /// let header = Path::new("blog/header.png");
    ///
    /// assert_eq!(dirs.abs_src_asset(header), Path::new("content/blog/header.png"));
    /// ```
    pub fn abs_src_asset(&self, asset_path: &Path) -> PathBuf {
        let mut path = PathBuf::new();
        path.push(self.src.clone());
        path.push(asset_path);
        path
    }

    /// Returns the absolute path to a source asset, given a partial asset path.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use pipeworks::Directories;
    ///
    /// let dirs = Directories::new(Path::new("content"), Path::new("public"));
    /// let header = Path::new("blog/header.png");
    ///
    /// assert_eq!(dirs.abs_target_asset(header), Path::new("public/blog/header.png"));
    /// ```
    pub fn abs_target_asset(&self, asset_path: &Path) -> PathBuf {
        let mut path = PathBuf::new();
        path.push(self.out.clone());
        path.push(asset_path);
        path
    }
}

pub fn gen_temp_file() -> NamedTempFile {
    tempfile::Builder::new()
        .prefix("pipeworks-artifact_")
        .rand_bytes(12)
        .tempfile()
        .expect("failed to create temp file")
}

pub fn glob_to_re<T: AsRef<str>>(glob: T) -> Regex {
    let masks = [
        ("**/", "<__RECURSE__>"),
        ("*", "<__ANY__>"),
        ("?", "<__ONE__>"),
        ("/", "<__SLASH__>"),
        (".", "<__DOT__>"),
    ];
    let mut updated_glob = glob.as_ref().to_owned();
    for m in masks.iter() {
        updated_glob = updated_glob.replace(m.0, m.1);
    }

    let replacements = [
        ("<__RECURSE__>", r".*"),
        ("<__ANY__>", r"[^\/]*"),
        ("<__ONE__>", "."),
        ("<__SLASH__>", r"\/"),
        ("<__DOT__>", r"\."),
    ];
    let mut re_str = format!("^{updated_glob}");
    for r in replacements {
        re_str = re_str.replace(r.0, r.0);
    }
    Regex::new(&re_str).expect("failed to convert glob to regex")
}

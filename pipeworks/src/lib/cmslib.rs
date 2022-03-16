pub mod discover;
pub mod pipeline;
pub mod render;
pub use pipeline::Pipeline;

use anyhow::Context;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("internal error")]
    DocumentSplitError,
}

#[derive(Error, Debug)]
pub enum FrontMatterError {
    #[error("parse error: {0}")]
    Error(#[from] toml::de::Error),
}

#[derive(Error, Debug)]
pub enum DocumentError {
    #[error("missing frontmatter")]
    MissingFrontmatter,
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

impl TryFrom<RawFrontMatter> for FrontMatter {
    type Error = FrontMatterError;
    fn try_from(raw: RawFrontMatter) -> Result<Self, Self::Error> {
        Ok(toml::from_str(&raw.0)?)
    }
}

impl RawFrontMatter {
    pub fn new<F: AsRef<str>>(frontmatter: F) -> Self {
        Self(frontmatter.as_ref().to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FrontMatter {
    pub title: String,
    pub template_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub content: RawMarkdown,
    pub frontmatter: FrontMatter,
    pub path: PathBuf,
}

pub fn generate_pages(dirs: Directories) -> Result<Vec<Page>, anyhow::Error> {
    let markdown_files = discover::get_all_paths(dirs.abs_src_dir(), &|path: &Path| -> bool {
        path.extension()
            .map(|ext| ext == "md")
            .unwrap_or_else(|| false)
    })?;
    let mut pages = vec![];
    for path in markdown_files.iter() {
        let doc = std::fs::read_to_string(path)?;
        let (frontmatter, markdown) = split_document(doc, path).expect("missing frontmatter");
        let frontmatter = FrontMatter::try_from(frontmatter)?;
        pages.push(Page {
            content: markdown,
            frontmatter,
            path: path.to_owned(),
        })
    }
    Ok(pages)
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
    /// use cmslib::Directories;
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
    /// use cmslib::Directories;
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

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

pub fn split_document<D, P>(
    document: D,
    path: P,
) -> Result<(RawFrontMatter, RawMarkdown), anyhow::Error>
where
    D: AsRef<str>,
    P: AsRef<Path>,
{
    let doc = document.as_ref();
    let re = regex!(
        r#"^[[:space:]]*\+\+\+[[:space:]]*\n((?s).*)\n[[:space:]]*\+\+\+[[:space:]]*((?s).*)"#
    );
    match re.captures(doc) {
        Some(captures) => {
            let frontmatter = captures
                .get(1)
                .map(|m| m.as_str())
                .ok_or_else(|| DocumentError::MissingFrontmatter)?;
            let frontmatter = RawFrontMatter::new(frontmatter);

            let markdown = captures
                .get(2)
                .map(|m| m.as_str())
                .ok_or_else(|| EngineError::DocumentSplitError)
                .with_context(|| {
                    format!(
                        "Missing second regex capture when processing document data at '{}'",
                        path.as_ref().to_string_lossy()
                    )
                })?;
            let markdown = RawMarkdown::new(markdown);

            Ok((frontmatter, markdown))
        }
        None => Err(DocumentError::MissingFrontmatter)?,
    }
}

#[cfg(test)]
mod test {
    use super::split_document;

    #[test]
    fn splits_well_formed_document() {
        let data = r"+++
a=1
b=2
c=3
+++
content here";
        let (frontmatter, markdown) = split_document(data, "").unwrap();
        assert_eq!(frontmatter.0, "a=1\nb=2\nc=3");
        assert_eq!(markdown.0, "content here");
    }

    #[test]
    fn splits_well_formed_document_with_newlines() {
        let data = r"+++
a=1
b=2
c=3

+++
content here

some newlines

";
        let (frontmatter, markdown) = split_document(data, "").unwrap();
        assert_eq!(frontmatter.0, "a=1\nb=2\nc=3\n");
        assert_eq!(markdown.0, "content here\n\nsome newlines\n\n");
    }
}

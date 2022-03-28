use anyhow::Context;
use serde::Serialize;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

#[macro_export]
macro_rules! static_regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| {
            regex::Regex::new($re).expect(&format!("Malformed regex '{}'. This is a bug.", $re))
        })
    }};
}
pub(crate) use static_regex;

pub fn gen_temp_file() -> Result<NamedTempFile, anyhow::Error> {
    Ok(tempfile::Builder::new()
        .prefix("pipeworks-artifact_")
        .rand_bytes(12)
        .tempfile()
        .with_context(|| format!("failed creating temporary file for shell processing"))?)
}

pub fn get_all_templates(template_root: PathBuf) -> Result<Vec<PathBuf>, anyhow::Error> {
    Ok(crate::discover::get_all_paths(
        &template_root,
        &|path: &Path| -> bool {
            path.extension()
                .map(|ext| ext == "tera")
                .unwrap_or_else(|| false)
        },
    )?)
}

pub fn strip_root<P: AsRef<Path>>(root: P, path: P) -> PathBuf {
    let root = root.as_ref().iter().collect::<Vec<_>>();
    let path = path.as_ref().iter().collect::<Vec<_>>();

    let mut i = 0;
    while root.get(i) == path.get(i) {
        i += 1;
    }
    PathBuf::from_iter(path[i..].iter())
}

/// A path buffer that allows for easy modification of root paths and
/// changes to file names and extensions.
///
/// This is used to map source file document paths into template file paths
/// and for reverse discovery of assets.
#[derive(Clone, Debug, Serialize, Default)]
pub struct RetargetablePathBuf {
    root: PathBuf,
    target: PathBuf,
}

impl RetargetablePathBuf {
    pub fn script_get(&mut self) -> String {
        self.to_string()
    }
    pub fn new<R: AsRef<Path>, P: AsRef<Path>>(root: R, target: P) -> Self {
        Self {
            root: PathBuf::from(root.as_ref()),
            target: PathBuf::from(target.as_ref()),
        }
    }

    pub fn to_string(&self) -> String {
        self.to_path_buf().to_string_lossy().to_string()
    }

    pub fn as_target(&self) -> &Path {
        self.target.as_path()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        let mut full = PathBuf::from(&self.root);
        full.push(&self.target);
        full
    }

    pub fn change_root<P: AsRef<Path>>(&mut self, path: P) {
        self.root = path.as_ref().to_path_buf();
    }

    pub fn set_file_name<S: AsRef<OsStr>>(&mut self, file_name: S) {
        self.target.set_file_name(file_name);
    }

    pub fn set_extension<S: AsRef<OsStr>>(&mut self, extension: S) {
        self.target.set_extension(extension);
    }

    pub fn push_path<P: AsRef<Path>>(&mut self, path: P) {
        self.target.push(path);
    }

    pub fn to_parent(&self) -> Self {
        let mut new_buf = self.clone();
        new_buf.target.pop();
        new_buf
    }

    pub fn with_root<P: AsRef<Path>>(&self, root: &Path) -> Self {
        let mut new_buf = self.clone();
        new_buf.change_root(root);
        new_buf
    }

    pub fn with_file_name<S: AsRef<OsStr>>(&self, file_name: S) -> Self {
        let mut new_buf = self.clone();
        new_buf.set_file_name(file_name);
        new_buf
    }

    pub fn with_extension<S: AsRef<OsStr>>(&self, extension: S) -> Self {
        let mut new_buf = self.clone();
        new_buf.set_extension(extension);
        new_buf
    }
}

#[derive(Debug)]
pub struct GlobCandidate<'a>(globset::Candidate<'a>);

impl<'a> GlobCandidate<'a> {
    pub fn new<P: AsRef<Path> + ?Sized>(path: &'a P) -> GlobCandidate<'a> {
        Self(globset::Candidate::new(path))
    }
}

#[derive(Debug, Clone)]
pub struct Glob {
    glob: globset::Glob,
    matcher: globset::GlobMatcher,
}

impl Glob {
    pub fn is_match<P: AsRef<Path>>(&self, path: P) -> bool {
        self.matcher.is_match(path)
    }
    pub fn is_match_candidate(&self, path: &GlobCandidate<'_>) -> bool {
        self.matcher.is_match_candidate(&path.0)
    }

    pub fn glob(&self) -> &str {
        self.glob.glob()
    }
}

impl TryFrom<String> for Glob {
    type Error = globset::Error;
    fn try_from(s: String) -> Result<Glob, Self::Error> {
        let glob = globset::GlobBuilder::new(&s)
            .literal_separator(true)
            .build()?;
        let matcher = glob.compile_matcher();
        Ok(Self { glob, matcher })
    }
}

impl TryFrom<&str> for Glob {
    type Error = globset::Error;
    fn try_from(s: &str) -> Result<Glob, Self::Error> {
        let glob = globset::GlobBuilder::new(s)
            .literal_separator(true)
            .build()?;
        let matcher = glob.compile_matcher();
        Ok(Self { glob, matcher })
    }
}

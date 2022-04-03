use anyhow::{anyhow, Context};
use serde::Serialize;
use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use tracing::{instrument, trace};

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

#[instrument]
pub fn make_parent_dirs<P: AsRef<Path> + std::fmt::Debug>(dir: P) -> Result<(), std::io::Error> {
    trace!("create parent directories");
    std::fs::create_dir_all(dir)
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

    #[instrument(ret)]
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

    #[instrument(ret)]
    fn try_from(s: &str) -> Result<Glob, Self::Error> {
        let glob = globset::GlobBuilder::new(s)
            .literal_separator(true)
            .build()?;
        let matcher = glob.compile_matcher();
        Ok(Self { glob, matcher })
    }
}

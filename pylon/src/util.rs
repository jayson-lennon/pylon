use anyhow::Context;

use crate::{core::RelSystemPath, Result};
use std::path::{Path, PathBuf};
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

pub fn gen_temp_file() -> Result<NamedTempFile> {
    tempfile::Builder::new()
        .prefix("pipeworks-artifact_")
        .rand_bytes(12)
        .tempfile()
        .with_context(|| "failed creating temporary file for shell processing".to_string())
}

#[instrument]
pub fn make_parent_dirs<P: AsRef<Path> + std::fmt::Debug>(dir: P) -> Result<()> {
    trace!("create parent directories");
    Ok(std::fs::create_dir_all(dir)?)
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
    fn try_from(s: String) -> std::result::Result<Glob, Self::Error> {
        s.as_str().try_into()
    }
}

impl TryFrom<&str> for Glob {
    type Error = globset::Error;

    #[instrument(ret)]
    fn try_from(s: &str) -> std::result::Result<Glob, Self::Error> {
        let glob = globset::GlobBuilder::new(s)
            .literal_separator(true)
            .build()?;
        let matcher = glob.compile_matcher();
        Ok(Self { glob, matcher })
    }
}

pub fn rel_to_abs<S: AsRef<str>>(relative_target: S, from_page_path: &RelSystemPath) -> String {
    let mut abs_path = PathBuf::from("/");
    abs_path.push(from_page_path.with_base("").to_path_buf().parent().unwrap());
    abs_path.push(&relative_target.as_ref());
    abs_path.to_string_lossy().to_string()
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn rel_to_abs_deep() {
        let rel_target = "some/resource.txt";
        let page_path = RelSystemPath::new("src", "1/2/3.md");
        let abs_target = super::rel_to_abs(rel_target, &page_path);
        assert_eq!(&*abs_target, "/1/2/some/resource.txt");
    }

    #[test]
    fn rel_to_abs_root() {
        let rel_target = "resource.txt";
        let page_path = RelSystemPath::new("src", "page.md");
        let abs_target = super::rel_to_abs(rel_target, &page_path);
        assert_eq!(&*abs_target, "/resource.txt");
    }

    #[test]
    fn rel_to_abs_same_dir() {
        let rel_target = "resource.txt";
        let page_path = RelSystemPath::new("src", "1/2/3/page.md");
        let abs_target = super::rel_to_abs(rel_target, &page_path);
        assert_eq!(&*abs_target, "/1/2/3/resource.txt");
    }

    #[test]
    fn glob_try_into_str() {
        let glob = Glob::try_from("/*.*");
        assert!(glob.is_ok());

        let glob = Glob::try_from("/*.*".to_owned());
        assert!(glob.is_ok());
    }

    #[test]
    fn glob_try_into_string() {
        let glob = Glob::try_from("/*.*".to_owned());
        assert!(glob.is_ok());
    }

    #[test]
    fn glob_is_match() {
        let glob = Glob::try_from("*.txt".to_owned()).unwrap();
        assert_eq!(glob.is_match("test.txt"), true);
        assert_eq!(glob.is_match("test.md"), false);
    }

    #[test]
    fn glob_is_match_candidate() {
        let glob = Glob::try_from("*.txt".to_owned()).unwrap();

        let candidate_ok = GlobCandidate::new("test.txt");
        let candidate_err = GlobCandidate::new("test.md");

        assert_eq!(glob.is_match_candidate(&candidate_ok), true);
        assert_eq!(glob.is_match_candidate(&candidate_err), false);
    }

    #[test]
    fn glob_get_as_str() {
        let glob = Glob::try_from("*.txt".to_owned()).unwrap();

        assert_eq!(glob.glob(), "*.txt");
    }
}

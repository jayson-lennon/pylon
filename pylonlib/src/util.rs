use eyre::WrapErr;
use typed_path::{CheckedFilePath, SysPath};
use typed_uri::{CheckedUri, Uri};

use crate::{AbsPath, Result};
use std::path::Path;
use tempfile::NamedTempFile;
use tracing::{instrument, trace};

pub const TMP_ARTIFACT_PREFIX: &str = "pylon-artifact_";

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
        .prefix(TMP_ARTIFACT_PREFIX)
        .rand_bytes(12)
        .tempfile()
        .with_context(|| "failed creating temporary file for shell processing".to_string())
}

#[instrument]
pub fn make_parent_dirs(dir: &AbsPath) -> Result<()> {
    trace!("create parent directories");
    std::fs::create_dir_all(dir)
        .wrap_err_with(|| format!("Failed to create parent directories from '{}'", dir))
}

pub fn checked_uri_from_sys_path<S: Into<String>>(path: &SysPath, uri: S) -> Result<CheckedUri> {
    let uri = uri.into();
    let checked_html = CheckedFilePath::try_from(path).wrap_err_with(|| {
        format!(
            "Failed to create CheckedFilePath from '{}' when creating SysPath from Uri",
            path
        )
    })?;
    let uri = Uri::new(&uri).wrap_err_with(|| {
        format!(
            "Failed to create URI from '{}' when creating SysPath from Uri",
            uri
        )
    })?;
    Ok(CheckedUri::new(&checked_html, &uri))
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

#[cfg(test)]
mod test {
    #![allow(warnings, unused)]

    use super::*;

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

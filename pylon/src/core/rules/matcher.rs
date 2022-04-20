use tracing::trace;

use crate::{
    core::Uri,
    util::{Glob, GlobCandidate},
};

#[derive(Debug, Clone)]
pub enum Matcher {
    Glob(Vec<Glob>),
}

impl Matcher {
    pub fn is_match(&self, uri: &Uri) -> bool {
        match self {
            Matcher::Glob(globs) => {
                trace!("using glob match");
                let candidate = GlobCandidate::new(uri.as_str());

                let mut is_match = false;
                for g in globs {
                    if g.is_match_candidate(&candidate) {
                        is_match = true;
                        break;
                    }
                }
                is_match
            }
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::util::Glob;

    pub fn make_matcher(globs: &[&str]) -> Matcher {
        let mut matcher_globs = vec![];
        for glob in globs {
            matcher_globs.push(Glob::try_from(*glob).unwrap());
        }
        Matcher::Glob(matcher_globs)
    }

    #[test]
    fn finds_match() {
        let matcher = make_matcher(&["/*_?.md", "/test*.md"]);

        let uri = Uri::from_path("/test_3.md");

        assert!(matcher.is_match(&uri))
    }
}
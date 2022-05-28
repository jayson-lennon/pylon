use tracing::trace;

use crate::util::{GlobCandidate, PylonGlob};

#[derive(Debug, Clone)]
pub enum Matcher {
    Glob(Vec<PylonGlob>),
}

impl Matcher {
    pub fn is_match<S: AsRef<str>>(&self, search: S) -> bool {
        match self {
            Matcher::Glob(globs) => {
                trace!("using glob match");
                let candidate = GlobCandidate::new(search.as_ref());

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

    #![allow(warnings, unused)]
    use super::*;
    use crate::util::PylonGlob;

    pub fn make_matcher(globs: &[&str]) -> Matcher {
        let mut matcher_globs = vec![];
        for glob in globs {
            matcher_globs.push(PylonGlob::try_from(*glob).unwrap());
        }
        Matcher::Glob(matcher_globs)
    }

    #[test]
    fn finds_match() {
        let matcher = make_matcher(&["/*_?.md", "/test*.md"]);

        assert!(matcher.is_match("/test_3.md"))
    }
}

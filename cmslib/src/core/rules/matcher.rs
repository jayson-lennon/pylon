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

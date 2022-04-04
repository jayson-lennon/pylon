use crate::{
    core::Page,
    util::{Glob, GlobCandidate},
};

use serde::Deserialize;
use slotmap::SlotMap;

use tracing::{instrument, trace};

slotmap::new_key_type! {
    pub struct GeneratorKey;
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextItem {
    pub identifier: String,
    pub data: serde_json::Value,
}

impl ContextItem {
    #[must_use]
    pub fn new<S: AsRef<str>>(identifier: S, data: serde_json::Value) -> Self {
        Self {
            identifier: identifier.as_ref().to_string(),
            data,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Matcher {
    Glob(Vec<Glob>),
}

#[derive(Debug, Clone)]
pub struct Generators {
    generators: SlotMap<GeneratorKey, rhai::FnPtr>,
    matchers: Vec<(Matcher, GeneratorKey)>,
}

impl Generators {
    #[must_use]
    pub fn new() -> Self {
        Self {
            generators: SlotMap::with_key(),
            matchers: vec![],
        }
    }

    #[instrument(skip_all)]
    pub fn add_generator(&mut self, matcher: Matcher, generator: rhai::FnPtr) {
        trace!("add context generator function");
        let key = self.generators.insert(generator);
        self.matchers.push((matcher, key));
    }

    #[instrument(skip_all)]
    pub fn find_generators(&self, page: &Page) -> Vec<GeneratorKey> {
        self.matchers
            .iter()
            .filter_map(|(matcher, generator_key)| match matcher {
                Matcher::Glob(globs) => {
                    trace!("using glob match");
                    let candidate = GlobCandidate::new(page.uri.as_str());

                    let mut is_match = false;
                    for g in globs {
                        if g.is_match_candidate(&candidate) {
                            is_match = true;
                            break;
                        }
                    }
                    trace!(is_match);
                    if is_match {
                        Some(*generator_key)
                    } else {
                        None
                    }
                }
            })
            .collect()
    }

    pub fn get(&self, key: GeneratorKey) -> Option<rhai::FnPtr> {
        self.generators.get(key).cloned()
    }
}

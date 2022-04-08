use slotmap::SlotMap;
use tracing::{instrument, trace};

use crate::core::Uri;

use super::Matcher;

#[derive(Debug, Clone)]
pub struct ScriptFnCollection<T: slotmap::Key> {
    lints: SlotMap<T, rhai::FnPtr>,
    matchers: Vec<(Matcher, T)>,
}

impl<T: slotmap::Key> ScriptFnCollection<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            lints: SlotMap::with_key(),
            matchers: vec![],
        }
    }

    #[instrument(skip_all)]
    pub fn add(&mut self, matcher: Matcher, ctx_fn: rhai::FnPtr) {
        trace!("add matcher to fn pointers");
        let key = self.lints.insert(ctx_fn);
        self.matchers.push((matcher, key));
    }

    #[instrument(skip_all)]
    pub fn find_keys(&self, uri: &Uri) -> Vec<T> {
        self.matchers
            .iter()
            .filter_map(|(matcher, key)| match matcher.is_match(&uri) {
                true => Some(*key),
                false => None,
            })
            .collect()
    }

    pub fn get(&self, key: T) -> Option<rhai::FnPtr> {
        self.lints.get(key).cloned()
    }
}

impl<T: slotmap::Key> Default for ScriptFnCollection<T> {
    fn default() -> Self {
        Self::new()
    }
}

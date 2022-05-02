use super::Page;
use crate::core::rules::{Matcher, RuleProcessor};
use crate::core::Uri;
use anyhow::anyhow;

use slotmap::SlotMap;
use std::str::FromStr;
use tracing::{instrument, trace};

pub const LINT_LEVEL_DENY: &str = "DENY";
pub const LINT_LEVEL_WARN: &str = "WARN";

#[derive(Clone, Debug)]
pub struct Lint {
    level: LintLevel,
    msg: String,
    lint_fn: rhai::FnPtr,
}

impl Lint {
    pub fn new<S: Into<String>>(level: LintLevel, msg: S, lint_fn: rhai::FnPtr) -> Self {
        Self {
            level,
            msg: msg.into(),
            lint_fn,
        }
    }
}

#[derive(Clone, Debug, Copy, Eq, PartialEq)]
pub enum LintLevel {
    Deny,
    Warn,
}

impl FromStr for LintLevel {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            LINT_LEVEL_DENY => Ok(Self::Deny),
            LINT_LEVEL_WARN => Ok(Self::Warn),
            other => Err(anyhow!("invalid lint level {}", other)),
        }
    }
}

slotmap::new_key_type! {
    pub struct LintKey;
}

#[derive(Debug, Clone)]
pub struct LintCollection {
    lints: SlotMap<LintKey, Lint>,
    matchers: Vec<(Matcher, LintKey)>,
}

impl LintCollection {
    pub fn new() -> Self {
        Self {
            lints: SlotMap::with_key(),
            matchers: vec![],
        }
    }

    #[instrument(skip_all)]
    pub fn add(&mut self, matcher: Matcher, lint: Lint) {
        trace!("add lint");
        let key = self.lints.insert(lint);
        self.matchers.push((matcher, key));
    }

    #[instrument(skip_all)]
    pub fn find_keys(&self, uri: &Uri) -> Vec<LintKey> {
        self.matchers
            .iter()
            .filter_map(|(matcher, key)| match matcher.is_match(&uri) {
                true => Some(*key),
                false => None,
            })
            .collect()
    }

    pub fn get(&self, key: LintKey) -> Option<Lint> {
        self.lints.get(key).cloned()
    }

    pub fn len(&self) -> usize {
        self.lints.len()
    }
}

impl Default for LintCollection {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct LintResult {
    pub level: LintLevel,
    pub msg: String,
    pub page_uri: Uri,
}

impl LintResult {
    pub fn new<S: Into<String>>(level: LintLevel, msg: S, page_uri: Uri) -> Self {
        Self {
            level,
            msg: msg.into(),
            page_uri,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LintResults {
    inner: Vec<LintResult>,
}

impl LintResults {
    pub fn new() -> Self {
        Self { inner: vec![] }
    }

    pub fn from_slice(lints: &[LintResult]) -> Self {
        Self {
            inner: lints.into(),
        }
    }

    pub fn from_iter<L: Iterator<Item = LintResult>>(lints: L) -> Self {
        Self {
            inner: lints.collect(),
        }
    }

    pub fn has_deny(&self) -> bool {
        for lint in &self.inner {
            if lint.level == LintLevel::Deny {
                return true;
            }
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl IntoIterator for LintResults {
    type Item = LintResult;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a LintResults {
    type Item = &'a LintResult;
    type IntoIter = std::slice::Iter<'a, LintResult>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl std::fmt::Display for LintResults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msgs = self
            .inner
            .iter()
            .map(|lint| lint.msg.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        write!(f, "{}", msgs)
    }
}

pub fn lint(
    rule_processor: &RuleProcessor,
    lints: &LintCollection,
    page: &Page,
) -> crate::Result<Vec<LintResult>> {
    let lints: Vec<Lint> = lints
        .find_keys(&page.uri())
        .iter()
        .filter_map(|key| lints.get(*key))
        .collect();
    let mut lint_results = vec![];
    for lint in lints {
        let check: bool = rule_processor.run(&lint.lint_fn, (page.clone(),))?;
        if check {
            let lint_result = LintResult::new(lint.level, lint.msg, page.uri());
            lint_results.push(lint_result);
        }
    }
    Ok(lint_results)
}

#[cfg(test)]
mod test {

    use super::*;
    use tempfile::TempDir;
    use temptree::temptree;

    use crate::core::{
        engine::{Engine, EnginePaths},
        page::LintLevel,
        Uri,
    };

    fn default_test_config(tree: &TempDir) -> EnginePaths {
        EnginePaths {
            rule_script: tree.path().join("rules.rhai"),
            src_root: tree.path().join("content"),
            syntax_theme_root: tree.path().join("syntax_themes"),
            target_root: tree.path().join("output"),
            template_root: tree.path().join("templates"),
        }
    }
    #[test]
    fn single_lint() {
        let test_page = r#"+++
        template_name = "empty.tera"
        +++
        test content"#;

        let rule_script = r#"
        rules.add_lint(DENY, "Missing author", "**", |page| {
            page.meta("author") == "" || type_of(page.meta("author")) == "()"
        });
        "#;

        let tree = temptree! {
          "rules.rhai": rule_script,
          templates: {
            "empty.tera": "",
          },
          output: {},
          content: {
              "test.md": test_page
          },
          syntax_themes: {},
        };

        let paths = default_test_config(&tree);
        let engine = Engine::new(paths).unwrap();

        let page = engine
            .page_store()
            .get(&Uri::from_path("/test.md"))
            .unwrap();

        let lints = super::lint(engine.rule_processor(), engine.rules().lints(), &page).unwrap();
        assert_eq!(lints[0].level, LintLevel::Deny);
        assert_eq!(lints[0].msg, "Missing author");
        assert_eq!(lints[0].page_uri, Uri::from_path("/test.html"));
    }

    #[test]
    fn multiple_lints() {
        let test_page = r#"+++
        template_name = "empty.tera"
        +++
        test content"#;

        let rule_script = r#"
        rules.add_lint(DENY, "Missing author", "**", |page| {
            page.meta("author") == "" || type_of(page.meta("author")) == "()"
        });
        rules.add_lint(WARN, "Missing publish date", "**", |page| {
            page.meta("published") == "" || type_of(page.meta("published")) == "()"
        });
        "#;

        let tree = temptree! {
          "rules.rhai": rule_script,
          templates: {
            "empty.tera": "",
          },
          output: {},
          content: {
              "test.md": test_page
          },
          syntax_themes: {},
        };

        let paths = default_test_config(&tree);
        let engine = Engine::new(paths).unwrap();

        let page = engine
            .page_store()
            .get(&Uri::from_path("/test.md"))
            .unwrap();

        let lints = super::lint(engine.rule_processor(), engine.rules().lints(), &page).unwrap();
        assert_eq!(lints[0].level, LintLevel::Deny);
        assert_eq!(lints[0].msg, "Missing author");
        assert_eq!(lints[0].page_uri, Uri::from_path("/test.html"));

        assert_eq!(lints[1].level, LintLevel::Warn);
        assert_eq!(lints[1].msg, "Missing publish date");
        assert_eq!(lints[1].page_uri, Uri::from_path("/test.html"));
    }

    #[test]
    fn new_lint_messages() {
        let msgs = LintResults::new();
        assert!(msgs.inner.is_empty());
    }

    #[test]
    fn lint_level_fromstr_deny() {
        let level = LintLevel::from_str(LINT_LEVEL_DENY).unwrap();
        assert_eq!(level, LintLevel::Deny);
    }

    #[test]
    fn lint_level_fromstr_warn() {
        let level = LintLevel::from_str(LINT_LEVEL_WARN).unwrap();
        assert_eq!(level, LintLevel::Warn);
    }

    #[test]
    fn lint_level_fromstr_other_err() {
        let level = LintLevel::from_str("nope");
        assert!(level.is_err());
    }

    #[test]
    fn lintcollection_default() {
        let collection = LintCollection::default();
        assert!(collection.lints.is_empty());
    }

    #[test]
    fn lintmessages_new() {
        let messages = LintResults::new();
        assert!(messages.inner.is_empty());
    }

    #[test]
    fn lintmessages_from_iter() {
        let lints = vec![
            LintResult::new(LintLevel::Warn, "", Uri::from_path("/")),
            LintResult::new(LintLevel::Deny, "", Uri::from_path("/")),
        ];
        let messages = LintResults::from_iter(lints.into_iter());
        assert_eq!(messages.inner.len(), 2);
    }

    #[test]
    fn lintmessages_into_iter() {
        let lints = vec![
            LintResult::new(LintLevel::Warn, "", Uri::from_path("/")),
            LintResult::new(LintLevel::Deny, "", Uri::from_path("/")),
        ];
        let messages = LintResults::from_iter(lints.into_iter());
        let mut messages_iter = messages.into_iter();
        assert_eq!(messages_iter.next().unwrap().level, LintLevel::Warn);
        assert_eq!(messages_iter.next().unwrap().level, LintLevel::Deny);
    }

    #[test]
    fn lintmessages_into_iter_ref() {
        let lints = vec![
            LintResult::new(LintLevel::Warn, "", Uri::from_path("/")),
            LintResult::new(LintLevel::Deny, "", Uri::from_path("/")),
        ];
        let messages = LintResults::from_iter(lints.into_iter());
        let messages_iter = &mut messages.into_iter();
        assert_eq!(messages_iter.next().unwrap().level, LintLevel::Warn);
        assert_eq!(messages_iter.next().unwrap().level, LintLevel::Deny);
    }

    #[test]
    fn lint_messages_denies_properly() {
        let mut lints = vec![LintResult::new(LintLevel::Warn, "", Uri::from_path("/"))];
        let messages = LintResults::from_slice(lints.as_slice());
        assert_eq!(messages.has_deny(), false);

        lints.push(LintResult::new(LintLevel::Deny, "", Uri::from_path("/")));
        let messages = LintResults::from_slice(lints.as_slice());
        assert!(messages.has_deny());
    }

    #[test]
    fn lintmessages_display_impl() {
        let lints = vec![
            LintResult::new(LintLevel::Warn, "abc", Uri::from_path("/")),
            LintResult::new(LintLevel::Deny, "123", Uri::from_path("/")),
        ];
        let messages = LintResults::from_iter(lints.into_iter());
        assert_eq!(messages.to_string(), String::from("abc\n123"));
    }
}

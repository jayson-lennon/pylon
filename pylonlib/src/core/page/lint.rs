use super::Page;
use crate::core::rules::{Matcher, RuleProcessor};
use eyre::{eyre, WrapErr};

use slotmap::SlotMap;
use std::str::FromStr;
use tracing::trace;
use typed_path::{pathmarker, CheckedFilePath};

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
    type Err = eyre::Report;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            LINT_LEVEL_DENY => Ok(Self::Deny),
            LINT_LEVEL_WARN => Ok(Self::Warn),
            other => Err(eyre!("invalid lint level {}", other)),
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

    pub fn add(&mut self, matcher: Matcher, lint: Lint) {
        trace!("add lint");
        let key = self.lints.insert(lint);
        self.matchers.push((matcher, key));
    }

    pub fn find_keys<S: AsRef<str>>(&self, search: S) -> Vec<LintKey> {
        self.matchers
            .iter()
            .filter_map(|(matcher, key)| match matcher.is_match(&search) {
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
    pub md_file: CheckedFilePath<pathmarker::Md>,
}

impl LintResult {
    pub fn new<S: Into<String>>(
        level: LintLevel,
        msg: S,
        md_file: &CheckedFilePath<pathmarker::Md>,
    ) -> Self {
        Self {
            level,
            msg: msg.into(),
            md_file: md_file.clone(),
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
        .find_keys(page.uri().as_str())
        .iter()
        .filter_map(|key| lints.get(*key))
        .collect();
    let mut lint_results = vec![];
    for lint in lints {
        let check: bool = rule_processor
            .run(&lint.lint_fn, (page.clone(),))
            .wrap_err("Failed to run lint")?;
        if check {
            let lint_result = LintResult::new(lint.level, lint.msg, page.path());
            lint_results.push(lint_result);
        }
    }
    Ok(lint_results)
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]

    use std::path::PathBuf;

    use super::*;

    use temptree::temptree;

    use crate::core::{engine::Engine, page::LintLevel};

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
          target: {},
          src: {
              "test.md": test_page
          },
          syntax_themes: {},
        };

        let paths = crate::test::default_test_paths(&tree);
        let engine = Engine::new(paths).unwrap();

        let page = engine.library().get(&"/test.md".into()).unwrap();

        let lints = super::lint(engine.rule_processor(), engine.rules().lints(), &page).unwrap();
        assert_eq!(lints[0].level, LintLevel::Deny);
        assert_eq!(lints[0].msg, "Missing author");
        assert_eq!(
            lints[0]
                .md_file
                .as_sys_path()
                .to_relative_path()
                .to_path_buf(),
            PathBuf::from("src/test.md")
        );
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
          target: {},
          src: {
              "test.md": test_page
          },
          syntax_themes: {},
        };

        let paths = crate::test::default_test_paths(&tree);
        let engine = Engine::new(paths).unwrap();

        let page = engine.library().get(&"/test.md".into()).unwrap();

        let lints = super::lint(engine.rule_processor(), engine.rules().lints(), &page).unwrap();
        assert_eq!(lints[0].level, LintLevel::Deny);
        assert_eq!(lints[0].msg, "Missing author");
        assert_eq!(
            lints[0]
                .md_file
                .as_sys_path()
                .to_relative_path()
                .to_path_buf(),
            PathBuf::from("src/test.md")
        );

        assert_eq!(lints[1].level, LintLevel::Warn);
        assert_eq!(lints[1].msg, "Missing publish date");
        assert_eq!(
            lints[1]
                .md_file
                .as_sys_path()
                .to_relative_path()
                .to_path_buf(),
            PathBuf::from("src/test.md")
        );
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
        let tree = temptree! {
          src: {
              "test.md": "",
          },
        };
        let checked_file = crate::test::checked_md_path(&tree, "src/test.md");
        let lints = vec![
            LintResult::new(LintLevel::Warn, "", &checked_file),
            LintResult::new(LintLevel::Deny, "", &checked_file),
        ];
        let messages = LintResults::from_iter(lints.into_iter());
        assert_eq!(messages.inner.len(), 2);
    }

    #[test]
    fn lintmessages_into_iter() {
        let tree = temptree! {
          src: {
              "test.md": "",
          },
        };
        let checked_file = crate::test::checked_md_path(&tree, "src/test.md");
        let lints = vec![
            LintResult::new(LintLevel::Warn, "", &checked_file),
            LintResult::new(LintLevel::Deny, "", &checked_file),
        ];
        let messages = LintResults::from_iter(lints.into_iter());
        let mut messages_iter = messages.into_iter();
        assert_eq!(messages_iter.next().unwrap().level, LintLevel::Warn);
        assert_eq!(messages_iter.next().unwrap().level, LintLevel::Deny);
    }

    #[test]
    fn lintmessages_into_iter_ref() {
        let tree = temptree! {
          src: {
              "test.md": "",
          },
        };
        let checked_file = crate::test::checked_md_path(&tree, "src/test.md");
        let lints = vec![
            LintResult::new(LintLevel::Warn, "", &checked_file),
            LintResult::new(LintLevel::Deny, "", &checked_file),
        ];
        let messages = LintResults::from_iter(lints.into_iter());
        let messages_iter = &mut messages.into_iter();
        assert_eq!(messages_iter.next().unwrap().level, LintLevel::Warn);
        assert_eq!(messages_iter.next().unwrap().level, LintLevel::Deny);
    }

    #[test]
    fn lint_messages_denies_properly() {
        let tree = temptree! {
          src: {
              "test.md": "",
          },
        };
        let checked_file = crate::test::checked_md_path(&tree, "src/test.md");
        let mut lints = vec![LintResult::new(LintLevel::Warn, "", &checked_file)];
        let messages = LintResults::from_slice(lints.as_slice());
        assert_eq!(messages.has_deny(), false);

        lints.push(LintResult::new(LintLevel::Deny, "", &checked_file));
        let messages = LintResults::from_slice(lints.as_slice());
        assert!(messages.has_deny());
    }

    #[test]
    fn lintmessages_display_impl() {
        let tree = temptree! {
          src: {
              "test.md": "",
          },
        };
        let checked_file = crate::test::checked_md_path(&tree, "src/test.md");
        let lints = vec![
            LintResult::new(LintLevel::Warn, "abc", &checked_file),
            LintResult::new(LintLevel::Deny, "123", &checked_file),
        ];
        let messages = LintResults::from_iter(lints.into_iter());
        assert_eq!(messages.to_string(), String::from("abc\n123"));
    }
}

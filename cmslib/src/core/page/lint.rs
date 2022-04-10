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
    #[must_use]
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
    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
    #[must_use]
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
}

impl Default for LintCollection {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct LintMsg {
    pub level: LintLevel,
    pub msg: String,
    pub page_uri: Uri,
}

impl LintMsg {
    #[must_use]
    pub fn new<S: Into<String>>(level: LintLevel, msg: S, page_uri: Uri) -> Self {
        Self {
            level,
            msg: msg.into(),
            page_uri,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Lints {
    inner: Vec<LintMsg>,
}

impl Lints {
    pub fn new() -> Self {
        Self { inner: vec![] }
    }

    pub fn from_slice(lints: &[LintMsg]) -> Self {
        Self {
            inner: lints.into(),
        }
    }

    pub fn from_iter<L: Iterator<Item = LintMsg>>(lints: L) -> Self {
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

impl IntoIterator for Lints {
    type Item = LintMsg;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a Lints {
    type Item = &'a LintMsg;
    type IntoIter = std::slice::Iter<'a, LintMsg>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl std::fmt::Display for Lints {
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
) -> Result<Vec<LintMsg>, anyhow::Error> {
    let lints: Vec<Lint> = lints
        .find_keys(&page.uri())
        .iter()
        .filter_map(|key| lints.get(*key))
        .collect();
    let mut lint_msgs = vec![];
    for lint in lints {
        let check: bool = rule_processor.run(&lint.lint_fn, (page.clone(),))?;
        if check {
            let lint_msg = LintMsg::new(lint.level, lint.msg, page.uri());
            lint_msgs.push(lint_msg);
        }
    }
    Ok(lint_msgs)
}

#[cfg(test)]
mod test {

    use temptree::temptree;

    use crate::core::{config::EngineConfig, engine::Engine, page::LintLevel, Uri};

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
          }
        };

        let config = EngineConfig::new(
            tree.path().join("content"),
            tree.path().join("output"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );
        let engine = Engine::new(config).unwrap();

        let page = engine.page_store().get(&Uri::from_path("/test")).unwrap();

        let lints = super::lint(engine.rule_processor(), engine.rules().lints(), &page).unwrap();
        assert_eq!(lints[0].level, LintLevel::Deny);
        assert_eq!(lints[0].msg, "Missing author");
        assert_eq!(lints[0].page_uri, Uri::from_path("/test"));
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
          }
        };

        let config = EngineConfig::new(
            tree.path().join("content"),
            tree.path().join("output"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );
        let engine = Engine::new(config).unwrap();

        let page = engine.page_store().get(&Uri::from_path("/test")).unwrap();

        let lints = super::lint(engine.rule_processor(), engine.rules().lints(), &page).unwrap();
        assert_eq!(lints[0].level, LintLevel::Deny);
        assert_eq!(lints[0].msg, "Missing author");
        assert_eq!(lints[0].page_uri, Uri::from_path("/test"));

        assert_eq!(lints[1].level, LintLevel::Warn);
        assert_eq!(lints[1].msg, "Missing publish date");
        assert_eq!(lints[1].page_uri, Uri::from_path("/test"));
    }
}

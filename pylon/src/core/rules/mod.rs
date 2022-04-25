pub mod fn_pointers;
pub mod matcher;

use std::path::{Path, PathBuf};

pub use fn_pointers::GlobStore;
pub use matcher::Matcher;

use crate::pipeline::Pipeline;
use serde::Serialize;

use super::page::lint::{Lint, LintCollection};

slotmap::new_key_type! {
    pub struct ContextKey;
}

#[derive(Debug, Clone)]
pub struct Mount {
    src: PathBuf,
    target: PathBuf,
}

impl Mount {
    pub fn new<P: Into<PathBuf>>(src: P, target: P) -> Self {
        Self {
            src: src.into(),
            target: target.into(),
        }
    }
    pub fn src(&self) -> &Path {
        &self.src
    }

    pub fn target(&self) -> &Path {
        &self.target
    }
}

#[derive(Debug, Clone)]
pub struct Rules {
    pipelines: Vec<Pipeline>,
    global_context: Option<serde_json::Value>,
    page_contexts: GlobStore<ContextKey, rhai::FnPtr>,
    lints: LintCollection,
    mounts: Vec<Mount>,
    watches: Vec<PathBuf>,
}

impl Rules {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_global_context<S: Serialize>(&mut self, ctx: S) -> crate::Result<()> {
        let ctx = serde_json::to_value(ctx)?;
        self.global_context = Some(ctx);
        Ok(())
    }

    pub fn global_context(&self) -> Option<&serde_json::Value> {
        self.global_context.as_ref()
    }

    pub fn add_lint(&mut self, matcher: Matcher, lint: Lint) {
        self.lints.add(matcher, lint);
    }

    pub fn lints(&self) -> &LintCollection {
        &self.lints
    }

    pub fn add_page_context(&mut self, matcher: Matcher, ctx_fn: rhai::FnPtr) {
        self.page_contexts.add(matcher, ctx_fn);
    }

    pub fn page_contexts(&self) -> &GlobStore<ContextKey, rhai::FnPtr> {
        &self.page_contexts
    }

    pub fn add_pipeline(&mut self, pipeline: Pipeline) {
        self.pipelines.push(pipeline);
    }

    pub fn pipelines(&self) -> impl Iterator<Item = &Pipeline> {
        self.pipelines.iter()
    }

    pub fn add_mount<P: Into<PathBuf>>(&mut self, src: P, target: P) {
        self.mounts.push(Mount::new(src, target));
    }

    pub fn mounts(&self) -> impl Iterator<Item = &Mount> {
        self.mounts.iter()
    }

    pub fn add_watch<P: Into<PathBuf>>(&mut self, path: P) {
        self.watches.push(path.into());
    }

    pub fn watches(&self) -> impl Iterator<Item = &Path> {
        self.watches.iter().map(PathBuf::as_path)
    }
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            pipelines: vec![],
            global_context: None,
            page_contexts: GlobStore::new(),
            lints: LintCollection::new(),
            mounts: vec![],
            watches: vec![],
        }
    }
}

#[derive(Debug)]
pub struct RuleProcessor {
    engine: rhai::Engine,
    ast: rhai::AST,
}

impl RuleProcessor {
    pub fn new<S: AsRef<str>>(engine: rhai::Engine, script: S) -> crate::Result<Self> {
        let script = script.as_ref();
        let ast = engine.compile(script)?;
        Ok(Self { engine, ast })
    }

    pub fn run<T: Clone + Send + Sync + 'static, A: rhai::FuncArgs>(
        &self,
        ptr: &rhai::FnPtr,
        args: A,
    ) -> crate::Result<T> {
        Ok(ptr.call(&self.engine, &self.ast, args)?)
    }
}

pub mod script {
    #[allow(clippy::wildcard_imports)]
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::core::rules::{Matcher, Rules};
        use rhai::FnPtr;
        use tracing::{instrument, trace};

        #[rhai_fn()]
        pub fn new_rules() -> Rules {
            Rules::new()
        }

        #[rhai_fn(name = "add_pipeline", return_raw)]
        pub fn add_pipeline(
            rules: &mut Rules,
            base_dir: &str,
            target_glob: &str,
            ops: rhai::Array,
        ) -> Result<(), Box<EvalAltResult>> {
            use crate::pipeline::{Operation, Pipeline};
            use std::str::FromStr;

            let mut parsed_ops = vec![];
            for op in ops {
                let op: String = op.into_string()?;
                let op = Operation::from_str(&op)?;
                parsed_ops.push(op);
            }
            let pipeline = Pipeline::with_ops(base_dir, target_glob, &parsed_ops).map_err(|e| {
                EvalAltResult::ErrorSystem("failed creating pipeline".into(), e.into())
            })?;
            rules.add_pipeline(pipeline);

            dbg!("pipeline added");

            Ok(())
        }

        /// Associates the closure with the given matcher. This closure will be called
        /// and the returned context from the closure will be available in the page template.
        #[instrument(skip(rules, ctx_fn))]
        #[rhai_fn(return_raw)]
        pub fn add_page_context(
            rules: &mut Rules,
            matcher: &str,
            ctx_fn: FnPtr,
        ) -> Result<(), Box<EvalAltResult>> {
            let matcher = crate::util::Glob::try_from(matcher).map_err(|e| {
                EvalAltResult::ErrorSystem("failed processing glob".into(), e.into())
            })?;
            let matcher = Matcher::Glob(vec![matcher]);
            trace!("add page ctx_fn");
            rules.add_page_context(matcher, ctx_fn);
            Ok(())
        }

        /// Associates the closure with the given matcher. This closure will be called
        /// and the returned context from the closure will be available in the page template.
        #[instrument(skip(rules))]
        #[rhai_fn(return_raw)]
        pub fn set_global_context(
            rules: &mut Rules,
            ctx: rhai::Dynamic,
        ) -> Result<(), Box<EvalAltResult>> {
            let ctx: serde_json::Value = rhai::serde::from_dynamic(&ctx)?;
            trace!("add global ctx");
            rules.set_global_context(ctx).map_err(|e| {
                EvalAltResult::ErrorSystem("failed setting global context".into(), e.into())
            })?;
            Ok(())
        }

        #[instrument(skip(rules, lint_fn))]
        #[rhai_fn(return_raw)]
        pub fn add_lint(
            rules: &mut Rules,
            warn_or_deny: &str,
            msg: &str,
            matcher: &str,
            lint_fn: FnPtr,
        ) -> Result<(), Box<EvalAltResult>> {
            use crate::core::page::lint::{Lint, LintLevel};
            use std::str::FromStr;

            let matcher = crate::util::Glob::try_from(matcher).map_err(|e| {
                EvalAltResult::ErrorSystem("failed processing glob".into(), e.into())
            })?;

            trace!("add page lint");

            let lint_level = LintLevel::from_str(warn_or_deny)
                .map_err(|e| EvalAltResult::ErrorSystem("invlaid lint level".into(), e.into()))?;
            let matcher = Matcher::Glob(vec![matcher]);

            let lint = Lint::new(lint_level, msg, lint_fn);
            rules.add_lint(matcher, lint);
            Ok(())
        }

        #[instrument(skip(rules))]
        pub fn mount(rules: &mut Rules, src: &str, target: &str) {
            trace!("add mount");

            rules.add_mount(src, target);
        }

        #[instrument(skip(rules))]
        pub fn watch(rules: &mut Rules, path: &str) {
            trace!("add watch");

            rules.add_watch(path);
        }

        #[cfg(test)]
        mod test {
            use crate::core::config::EngineConfig;

            use super::*;

            #[test]
            fn makes_new_rules() {
                new_rules();
            }

            #[test]
            fn adds_pipeline() {
                let mut rules = Rules::default();
                let values = vec!["[COPY]".into()];
                super::add_pipeline(&mut rules, "base", "*", values)
                    .expect("failed to add pipeline");
                assert_eq!(rules.pipelines().count(), 1);
            }

            #[test]
            fn adds_mount() {
                let mut rules = Rules::default();
                super::mount(&mut rules, "src", "target");
                assert_eq!(rules.mounts().count(), 1);
            }

            #[test]
            fn adds_watch() {
                let mut rules = Rules::default();
                super::watch(&mut rules, "test");
                assert_eq!(rules.watches().count(), 1);
            }

            #[test]
            fn rejects_bad_pipeline_op() {
                let mut rules = Rules::default();
                let values = vec![1.into()];
                assert!(super::add_pipeline(&mut rules, "base", "*", values).is_err());
            }

            #[test]
            fn adds_page_context() {
                use temptree::temptree;

                let ptr = {
                    let rules = r#"
            rules.add_page_context("**", |page| { () });
        "#;

                    let doc1 = r#"+++
            template_name = "empty.tera"
            +++
        "#;

                    let tree = temptree! {
                      "rules.rhai": rules,
                      templates: {
                          "empty.tera": ""
                      },
                      target: {},
                      src: {
                          "doc1.md": doc1,
                      },
                    };

                    let config = EngineConfig::new(
                        tree.path().join("src"),
                        tree.path().join("target"),
                        tree.path().join("templates"),
                        tree.path().join("rules.rhai"),
                    );

                    let engine = crate::core::engine::Engine::new(config).unwrap();

                    let all = engine
                        .rules()
                        .page_contexts()
                        .iter()
                        .map(|(_, p)| p)
                        .collect::<Vec<_>>();
                    all[0].clone()
                };
                let mut rules = Rules::default();
                assert!(super::add_page_context(&mut rules, "*", ptr).is_ok());
                assert_eq!(rules.page_contexts().iter().count(), 1);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::core::{config::EngineConfig, engine::Engine};
    use temptree::temptree;

    use super::*;

    #[test]
    fn rules_default() {
        Rules::default();
    }

    #[test]
    fn sets_global_context() {
        let mut rules = Rules::new();
        assert!(rules
            .set_global_context(serde_json::to_value(1).unwrap())
            .is_ok());
        assert!(rules.global_context().is_some());
    }

    #[test]
    fn adds_page_context() {
        let rules = r#"
            rules.add_page_context("**", |page| { () });
        "#;

        let doc1 = r#"+++
            template_name = "empty.tera"
            +++
        "#;

        let tree = temptree! {
          "rules.rhai": rules,
          templates: {
              "empty.tera": ""
          },
          target: {},
          src: {
              "doc1.md": doc1,
          },
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();

        assert_eq!(engine.rules().page_contexts().iter().count(), 1);
    }

    #[test]
    fn adds_mount() {
        let mut rules = Rules::new();
        rules.add_mount("src", "target");

        assert_eq!(rules.mounts().count(), 1);
    }
}

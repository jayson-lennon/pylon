pub mod fn_pointers;
pub mod matcher;

pub use fn_pointers::ScriptFnCollection;
pub use matcher::Matcher;

use crate::pipeline::Pipeline;
use serde::Serialize;

use super::page::lint::{Lint, LintCollection};

slotmap::new_key_type! {
    pub struct ContextKey;
}

#[derive(Debug, Clone)]
pub struct Rules {
    pipelines: Vec<Pipeline>,
    global_context: Option<serde_json::Value>,
    page_contexts: ScriptFnCollection<ContextKey, rhai::FnPtr>,
    lints: LintCollection,
}

impl Rules {
    pub fn add_pipeline(&mut self, pipeline: Pipeline) {
        self.pipelines.push(pipeline);
    }

    pub fn set_global_context<S: Serialize>(&mut self, ctx: S) -> Result<(), anyhow::Error> {
        let ctx = serde_json::to_value(ctx)?;
        self.global_context = Some(ctx);
        Ok(())
    }

    pub fn add_page_context(&mut self, matcher: Matcher, ctx_fn: rhai::FnPtr) {
        self.page_contexts.add(matcher, ctx_fn);
    }

    pub fn add_lint(&mut self, matcher: Matcher, lint: Lint) {
        self.lints.add(matcher, lint);
    }

    pub fn pipelines(&self) -> impl Iterator<Item = &Pipeline> {
        self.pipelines.iter()
    }

    pub fn global_context(&self) -> Option<&serde_json::Value> {
        self.global_context.as_ref()
    }

    pub fn page_contexts(&self) -> &ScriptFnCollection<ContextKey, rhai::FnPtr> {
        &self.page_contexts
    }

    pub fn lints(&self) -> &LintCollection {
        &self.lints
    }

    pub fn test(&mut self) {
        self.global_context = None;
    }
}

impl Rules {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pipelines: vec![],
            global_context: None,
            page_contexts: ScriptFnCollection::new(),
            lints: LintCollection::new(),
        }
    }
}

impl Default for Rules {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct RuleProcessor {
    engine: rhai::Engine,
    ast: rhai::AST,
}

impl RuleProcessor {
    pub fn new<S: AsRef<str>>(engine: rhai::Engine, script: S) -> Result<Self, anyhow::Error> {
        let script = script.as_ref();
        let ast = engine.compile(script)?;
        Ok(Self { engine, ast })
    }

    pub fn run<T: Clone + Send + Sync + 'static, A: rhai::FuncArgs>(
        &self,
        ptr: &rhai::FnPtr,
        args: A,
    ) -> Result<T, anyhow::Error> {
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
            let pipeline = Pipeline::with_ops(target_glob, &parsed_ops).map_err(|e| {
                EvalAltResult::ErrorSystem("failed creating pipeline".into(), e.into())
            })?;
            dbg!(&pipeline);
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
    }
}

use super::gctx::{Generators, Matcher};

use crate::pipeline::Pipeline;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct Rules {
    pipelines: Vec<Pipeline>,
    global_context: Option<serde_json::Value>,
    page_context: Generators,
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

    pub fn add_context_generator(&mut self, matcher: Matcher, generator: rhai::FnPtr) {
        self.page_context.add_generator(matcher, generator)
    }

    pub fn pipelines(&self) -> impl Iterator<Item = &Pipeline> {
        self.pipelines.iter()
    }

    pub fn global_context(&self) -> Option<&serde_json::Value> {
        self.global_context.as_ref()
    }

    pub fn page_context(&self) -> &Generators {
        &self.page_context
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
            page_context: Generators::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FrontmatterHookResponse {
    Ok,
    Warn(String),
    Error(String),
}

pub mod script {
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::core::rules::gctx::Matcher;
        use crate::core::rules::Rules;
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
            for op in ops.into_iter() {
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
        #[instrument(skip(rules, generator))]
        #[rhai_fn(return_raw)]
        pub fn add_page_context(
            rules: &mut Rules,
            matcher: &str,
            generator: FnPtr,
        ) -> Result<(), Box<EvalAltResult>> {
            let matcher = crate::util::Glob::try_from(matcher).map_err(|e| {
                EvalAltResult::ErrorSystem("failed processing glob".into(), e.into())
            })?;
            let matcher = Matcher::Glob(vec![matcher]);
            trace!("add context generator");
            rules.add_context_generator(matcher, generator);
            Ok(())
        }
    }
}

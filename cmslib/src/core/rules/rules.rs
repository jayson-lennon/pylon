use super::gctx::{Generators, Matcher};
use crate::core::rules::script::ScriptEngine;
use crate::{page::Page, pipeline::Pipeline};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct Rules {
    pipelines: Vec<Pipeline>,
    frontmatter_hooks: FrontmatterHooks,
    global_context: Option<serde_json::Value>,
    page_context: Generators,
}

impl Rules {
    pub fn add_pipeline(&mut self, pipeline: Pipeline) {
        self.pipelines.push(pipeline);
    }

    pub fn add_frontmatter_hook(&mut self, hook: rhai::FnPtr) {
        self.frontmatter_hooks.add(hook);
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
            frontmatter_hooks: FrontmatterHooks::new(),
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

type FrontmatterHook = Box<dyn Fn(&Page) -> FrontmatterHookResponse>;

#[derive(Debug, Clone)]
pub struct FrontmatterHooks {
    inner: Vec<rhai::FnPtr>,
}

impl FrontmatterHooks {
    pub fn new() -> Self {
        Self { inner: vec![] }
    }
    pub fn add(&mut self, hook: rhai::FnPtr) {
        self.inner.push(hook);
    }
}

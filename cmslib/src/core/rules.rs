use crate::{
    gctx::{GeneratorFunc, Generators, Matcher},
    page::Page,
    pipeline::Pipeline,
};
use serde::Serialize;

#[derive(Debug)]
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

    pub fn add_frontmatter_hook(&mut self, hook: FrontmatterHook) {
        self.frontmatter_hooks.add(hook);
    }

    pub fn set_global_context<S: Serialize>(&mut self, ctx: S) -> Result<(), anyhow::Error> {
        let ctx = serde_json::to_value(ctx)?;
        self.global_context = Some(ctx);
        Ok(())
    }

    pub fn add_context_generator(&mut self, matcher: Matcher, generator: GeneratorFunc) {
        self.page_context.add_generator(matcher, generator)
    }

    pub fn pipelines(&self) -> impl Iterator<Item = &Pipeline> {
        self.pipelines.iter()
    }

    pub fn frontmatter_hooks(&self) -> impl Iterator<Item = &FrontmatterHook> {
        self.frontmatter_hooks.iter()
    }

    pub fn global_context(&self) -> Option<&serde_json::Value> {
        self.global_context.as_ref()
    }

    pub fn page_context(&self) -> &Generators {
        &self.page_context
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

pub struct FrontmatterHooks {
    inner: Vec<Box<dyn Fn(&Page) -> FrontmatterHookResponse>>,
}

impl FrontmatterHooks {
    pub fn new() -> Self {
        Self { inner: vec![] }
    }
    pub fn add(&mut self, hook: FrontmatterHook) {
        self.inner.push(hook);
    }
    pub fn iter(&self) -> impl Iterator<Item = &FrontmatterHook> {
        self.inner.iter()
    }
}

impl std::fmt::Debug for FrontmatterHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("FrontmatterHook count: {}", self.inner.len()))
    }
}

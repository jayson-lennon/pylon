use anyhow::anyhow;
use itertools::Itertools;
use rhai::packages::{Package, StandardPackage};
use rhai::plugin::*;
use rhai::{def_package, Scope};
use std::collections::HashSet;
use tracing::{instrument, trace};

use crate::core::rules::{
    gctx::{ContextItem, Generators},
    RuleProcessor, Rules,
};
use crate::core::{Page, PageStore};

// Define the custom package 'MyCustomPackage'.
def_package! {
    /// My own personal super-duper custom package
    pub CmsPackage(module) {
      // Aggregate other packages simply by calling 'init' on each.
      StandardPackage::init(module);

     combine_with_exported_module!(module, "rules", crate::core::rules::script::rhai_module);
     combine_with_exported_module!(module, "frontmatter", crate::frontmatter::script::rhai_module);
     combine_with_exported_module!(module, "page", crate::core::page::script::rhai_module);
    //  combine_with_exported_module!(module, "pagestore", crate::pagestore::rhai_module);

      // custom functions go here
  }
}

pub struct ScriptEngineConfig {
    package: CmsPackage,
}

impl ScriptEngineConfig {
    #[must_use]
    pub fn new() -> Self {
        Self {
            package: CmsPackage::new(),
        }
    }

    pub fn modules(&self) -> Vec<rhai::Shared<Module>> {
        vec![self.package.as_shared_module()]
    }
}

impl Default for ScriptEngineConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct ScriptEngine {
    engine: rhai::Engine,
    packages: Vec<rhai::Shared<Module>>,
}

impl ScriptEngine {
    #[must_use]
    pub fn new(packages: &[rhai::Shared<Module>]) -> Self {
        let engine = Self::new_engine(packages);

        Self {
            engine,
            packages: packages.into(),
        }
    }

    fn register_types(engine: &mut rhai::Engine) {
        crate::core::pagestore::script::register_type(engine);
    }

    fn new_engine(packages: &[rhai::Shared<Module>]) -> rhai::Engine {
        let mut engine = rhai::Engine::new_raw();
        for pkg in packages {
            engine.register_global_module(pkg.clone());
        }

        engine.set_max_expr_depths(64, 64);
        engine.set_max_call_levels(64);
        engine.set_max_operations(5000);
        engine.set_max_modules(100);
        engine.on_print(|x| println!("script engine: {}", x));
        engine.on_debug(move |s, src, pos| {
            println!("{} @ {:?} > {}", src.unwrap_or("unknown"), pos, s);
        });

        ScriptEngine::register_types(&mut engine);

        engine
    }

    pub fn clone_engine(&self) -> rhai::Engine {
        Self::new_engine(&self.packages)
    }

    pub fn new_fn_runner<S: AsRef<str>>(&self, script: S) -> Result<RuleProcessor, anyhow::Error> {
        let engine = Self::new_engine(&self.packages);
        RuleProcessor::new(engine, script.as_ref())
    }

    pub fn build_rules<S: AsRef<str>>(
        &self,
        page_store: &PageStore,
        script: S,
    ) -> Result<(RuleProcessor, Rules), anyhow::Error> {
        let script = script.as_ref();
        let ast = self.engine.compile(script)?;

        let mut scope = Scope::new();
        scope.push("rules", Rules::new());
        scope.push("PAGES", page_store.clone());
        dbg!(&page_store);

        let rules = self.engine.eval_ast_with_scope::<Rules>(&mut scope, &ast)?;

        let runner = {
            let new_engine = Self::new_engine(&self.packages);
            RuleProcessor::new(new_engine, script)?
        };
        Ok((runner, rules))
    }
}

#[instrument(skip_all, fields(page = %for_page.uri()))]
pub fn build_context(
    script_fn_runner: &RuleProcessor,
    generators: &Generators,
    for_page: &Page,
) -> Result<Vec<ContextItem>, anyhow::Error> {
    trace!("building page-specific context");
    let contexts: Vec<Vec<ContextItem>> = generators
        .find_generators(for_page)
        .iter()
        .filter_map(|key| generators.get(*key))
        .map(|ptr| script_fn_runner.run(ptr, (for_page.clone(),)))
        .try_collect()?;
    let contexts = contexts.into_iter().flatten().collect::<Vec<_>>();

    let mut identifiers = HashSet::new();
    for ctx in contexts.iter() {
        if !identifiers.insert(ctx.identifier.as_str()) {
            return Err(anyhow!(
                "duplicate context identifier encountered in page context generation: {}",
                ctx.identifier.as_str()
            ));
        }
    }

    Ok(contexts)
}

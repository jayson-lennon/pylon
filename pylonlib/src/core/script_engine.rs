use rhai::packages::{Package, StandardPackage};
#[allow(clippy::wildcard_imports)]
use rhai::plugin::*;
use rhai::{def_package, Scope};

use crate::core::rules::{RuleProcessor, Rules};
use crate::core::PageStore;
use crate::{Result};

use super::engine::GlobalEnginePaths;

// Define the custom package 'MyCustomPackage'.
def_package! {
    /// My own personal super-duper custom package
    pub CmsPackage(module) {
      // Aggregate other packages simply by calling 'init' on each.
      StandardPackage::init(module);

     combine_with_exported_module!(module, "rules", crate::core::rules::script::rhai_module);
     combine_with_exported_module!(module, "frontmatter", crate::core::page::frontmatter::script::rhai_module);
     combine_with_exported_module!(module, "page", crate::core::page::script::rhai_module);
    //  combine_with_exported_module!(module, "pagestore", crate::pagestore::rhai_module);

      // custom functions go here
  }
}

pub struct ScriptEngineConfig {
    package: CmsPackage,
}

impl ScriptEngineConfig {
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

    pub fn new_rule_processor<S: AsRef<str>>(&self, script: S) -> Result<RuleProcessor> {
        let engine = self.clone_engine();
        RuleProcessor::new(engine, script.as_ref())
    }

    fn new_scope(engine_paths: GlobalEnginePaths, page_store: &PageStore) -> Scope<'static> {
        use crate::core::page::lint::{LINT_LEVEL_DENY, LINT_LEVEL_WARN};
        let mut scope = Scope::new();
        scope.push("rules", Rules::new(engine_paths));
        scope.push("PAGES", page_store.clone());
        scope.push("DENY", LINT_LEVEL_DENY);
        scope.push("WARN", LINT_LEVEL_WARN);
        //dbg!(&page_store);
        scope
    }

    pub fn build_rules<S: AsRef<str>>(
        &self,
        engine_paths: GlobalEnginePaths,
        page_store: &PageStore,
        script: S,
    ) -> Result<(RuleProcessor, Rules)> {
        let script = script.as_ref();
        let ast = self.engine.compile(script)?;

        let mut scope = Self::new_scope(engine_paths, page_store);

        self.engine.run_ast_with_scope(&mut scope, &ast)?;

        let rules = scope.get_value("rules").unwrap();

        let runner = {
            let new_engine = Self::new_engine(&self.packages);
            RuleProcessor::new(new_engine, script)?
        };
        Ok((runner, rules))
    }
}

#[cfg(test)]
mod test {

    #![allow(warnings, unused)]
    use super::*;
    use crate::test::abs;

    #[test]
    fn default_script_engine_config() {
        ScriptEngineConfig::default();
    }

    #[test]
    fn scope_contains_proper_items() {
        let store = PageStore::default();
        let (paths, tree) = crate::test::simple_init();
        let scope = ScriptEngine::new_scope(paths, &store);
        let required_items = &["rules", "PAGES", "DENY", "WARN"];
        for item in required_items {
            assert!(scope.contains(item));
        }
    }

    #[test]
    fn makes_new_rule_processor() {
        let engine = ScriptEngine::new(&[]);
        engine
            .new_rule_processor("")
            .expect("failed to generate new rule processor");
    }

    #[test]
    fn clones_engine() {
        let engine = ScriptEngine::new(&[]);
        engine.clone_engine();
    }
}

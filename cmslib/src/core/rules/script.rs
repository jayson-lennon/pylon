use anyhow::anyhow;
use itertools::Itertools;
use parking_lot::RwLock;
use rhai::packages::{Package, StandardPackage};
use rhai::{def_package, Scope};
use rhai::{plugin::*};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{instrument, trace};

use crate::core::{Page, PageStore};

use super::gctx::{ContextItem, Generators};
use super::Rules;

// Define the custom package 'MyCustomPackage'.
def_package! {
    /// My own personal super-duper custom package
    pub CmsPackage(module) {
      // Aggregate other packages simply by calling 'init' on each.
      StandardPackage::init(module);

     combine_with_exported_module!(module, "rules", crate::core::rules::rules::script::rhai_module);
     combine_with_exported_module!(module, "frontmatter", crate::frontmatter::script::rhai_module);
     combine_with_exported_module!(module, "page", crate::core::page::script::rhai_module);
    //  combine_with_exported_module!(module, "pagestore", crate::pagestore::rhai_module);

      // custom functions go here
  }
}

#[derive(Debug)]
struct NotClonable(i64);

#[derive(Debug, Clone)]
pub struct Database {
    inner: Arc<RwLock<NotClonable>>,
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

#[derive(Debug)]
pub struct RuleProcessor {
    engine: rhai::Engine,
    script: String,
    ast: rhai::AST,
}

impl RuleProcessor {
    #[must_use]
    pub fn new<S: AsRef<str>>(engine: rhai::Engine, script: S) -> Result<Self, anyhow::Error> {
        let script = script.as_ref();
        let ast = engine.compile(script)?;
        Ok(Self {
            engine,
            script: script.to_string(),
            ast,
        })
    }

    pub fn run<T: Clone + Send + Sync + 'static, A: rhai::FuncArgs>(
        &self,
        ptr: rhai::FnPtr,
        args: A,
    ) -> Result<T, anyhow::Error> {
        Ok(ptr.call(&self.engine, &self.ast, args)?)
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

        // for rule in rules.callbacks {
        //     let ans: () = rule.call(&engine, &ast, ((db.clone()),))?;
        //     dbg!(ans);
        // }
    }
}

impl Database {
    pub fn increment(&self) {
        let mut inner = self.inner.write();
        inner.0 += 1;
    }
}

// fn main() -> Result<(), anyhow::Error> {
//     let cms_package = CmsPackage::new();

//     let mut engine = Engine::new_raw();
//     engine.register_global_module(cms_package.as_shared_module());

//     let rules_script = r#"
//         let rules = default_rules();
//         rules.add_callback(|db| {
//             db.increment();
//         });
//         rules
//     "#;

//     let ast = engine.compile(rules_script)?;

//     // let mut fn_ptr = FnPtr::new("foo")?;

//     // Curry values into the function pointer
//     // fn_ptr.set_curry(vec!["abc".into()]);

//     // Values are only needed for non-curried parameters
//     // let result: i64 = fn_ptr.call(&engine, &ast, (39_i64,))?;
//     let db = Database {
//         inner: Arc::new(RwLock::new(NotClonable(5))),
//     };

//     let rules = engine.eval_ast::<Rules>(&ast)?;
//     for rule in rules.callbacks {
//         let ans: () = rule.call(&engine, &ast, ((db.clone()),))?;
//         dbg!(ans);
//     }
//     dbg!(db);

//     Ok(())
// }

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

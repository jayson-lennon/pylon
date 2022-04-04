mod sample {
    use rhai::{Engine, EvalAltResult, FnPtr, Scope};

    #[derive(Debug, Clone)]
    struct ClosureContainer {
        inner: Option<FnPtr>,
    }

    impl ClosureContainer {
        #[must_use]
        pub fn new() -> Self {
            Self { inner: None }
        }

        pub fn set(&mut self, inner: FnPtr) {
            self.inner = Some(inner);
        }
    }

    pub fn test_script() {
        let mut my_scope = Scope::new();
        my_scope.push("sample", ClosureContainer::new());

        let mut engine = Engine::new();
        engine
            .register_type_with_name::<ClosureContainer>("ScriptRules")
            .register_fn("new_closure", ClosureContainer::new)
            .register_fn("set", ClosureContainer::set);

        println!("Functions registered:");

        engine
            .gen_fn_signatures(false)
            .into_iter()
            .for_each(|func| println!("{}", func));

        println!();

        let script = r#"
    sample.set(
        |a| {
            a + 1
        }
    );
    "#;

        let ast = engine.compile_with_scope(&mut my_scope, script).unwrap();
        engine.run_ast_with_scope(&mut my_scope, &ast).unwrap();

        let container = my_scope.get_value::<ClosureContainer>("sample").unwrap();
        dbg!(&container);

        let func = move |x: i64| -> i64 {
            container
                .inner
                .as_ref()
                .unwrap()
                .call(&engine, &ast, (x,))
                .unwrap()
        };
        dbg!(func(1));
        let f = Box::new(func);
        dbg!(f(2));

        // let result = engine.run_ast_with_scope(&mut my_scope, &ast);
        // dbg!(result);
        // dbg!(&my_scope.get_value::<ScriptRules>("sample"));
        // dbg!(&my_scope);

        // let rules = my_scope.get_value::<ScriptRules>("sample").unwrap();
        // let result: Result<i32, _> = rules.cb.unwrap().call(&engine, &ast, ((1),));
        // dbg!(result);
    }
}

use std::collections::HashMap;

use rhai::{serde::from_dynamic, Dynamic, Engine, EvalAltResult, FnPtr, Scope, Shared};

use crate::{page::Page, Renderers};

macro_rules! script_get {
    ($fn:ident, $field:ident, $type:ty) => {
        pub fn $fn(&mut self) -> $type {
            self.$field.clone()
        }
    };
}

macro_rules! set {
    ($fn:ident, $field:ident, $type:ty) => {
        pub fn $fn(&mut self, value: $type) {
            self.$field = Some(value);
        }
    };
}

#[derive(Debug, Clone)]
struct ClosureContainer {
    inner: Option<FnPtr>,
}

impl ClosureContainer {
    #[must_use]
    pub fn new() -> Self {
        Self { inner: None }
    }

    pub fn set_inner(&mut self, inner: FnPtr) {
        self.inner = Some(inner);
    }
}

#[derive(Debug)]
pub struct Sample {
    inner: i64,
}

impl Sample {
    #[must_use]
    pub fn new(inner: i64) -> Self {
        Self { inner }
    }

    pub fn set(&mut self, inner: i64) {
        self.inner = inner;
    }
}

fn register_types(engine: &mut Engine) {
    use crate::frontmatter::FrontMatter;
    use crate::util::RetargetablePathBuf;

    engine
        .register_type_with_name::<ClosureContainer>("ScriptRules")
        .register_fn("new_closure", ClosureContainer::new)
        .register_fn("set", ClosureContainer::set_inner);

    engine
        .register_type_with_name::<Page>("Page")
        .register_get("system_path", Page::system_path)
        .register_get("raw_document", Page::raw_document)
        .register_get("frontmatter", Page::frontmatter)
        .register_get("canonical_path", Page::canonical_path);

    engine
        .register_type_with_name::<RetargetablePathBuf>("RetargetablePathBuf")
        .register_get("path", RetargetablePathBuf::script_get);

    engine
        .register_type_with_name::<FrontMatter>("FrontMatter")
        .register_get("template_name", FrontMatter::script_get_template_name)
        .register_get("use_file_url", FrontMatter::use_file_url)
        .register_get("meta", FrontMatter::meta);

    pub fn contains_meta(meta: HashMap<String, serde_json::Value>, item: String) -> bool {
        meta.contains_key(&item)
    }

    engine.register_fn("contains_meta", contains_meta);
}

pub fn test_script() {
    use std::sync::{Arc, Mutex};

    let mut my_scope = Scope::new();
    my_scope.push("sample", ClosureContainer::new());

    let mut engine = Engine::new();
    register_types(&mut engine);

    println!("Functions registered:");

    engine
        .gen_fn_signatures(false)
        .into_iter()
        .for_each(|func| println!("{}", func));

    let initial = Arc::new(Mutex::new(Sample::new(10)));

    let initial_clone = Arc::clone(&initial);

    engine.register_fn("global_update", move |x: i64| {
        let mut initial = initial_clone.lock().unwrap();
        initial.set(x);
    });

    let script = r#"
    page_context(|page| {
        if page.has_meta("test") {
            "yo"
        } else {
            "no"
        }
    });

    sample.set(
        |page| {
            if contains_meta(page.frontmatter.meta, "su") {
                print("yup")
            } else {
                print("nope")
            }
            1
        }
    );
    global_update(99);
    "#;

    dbg!(&initial);

    let ast = engine.compile_with_scope(&mut my_scope, script).unwrap();
    engine.run_ast_with_scope(&mut my_scope, &ast).unwrap();
    dbg!(&initial);

    let container = my_scope.get_value::<ClosureContainer>("sample").unwrap();
    dbg!(&container);

    let func = |x: Page| -> Dynamic {
        container
            .inner
            .as_ref()
            .unwrap()
            .call(&engine, &ast, (x,))
            .unwrap()
    };
    let renderers = Renderers::new("test/templates");
    let mut page = Page::new("test/src/index.md", "test/src", &renderers).unwrap();
    page.frontmatter.meta.insert("sup".into(), "hi".into());
    dbg!(func(page.clone()));
    let f = Box::new(func);
    let v: serde_json::Value = from_dynamic(&f(page.clone())).unwrap();
    dbg!(v);

    // let result = engine.run_ast_with_scope(&mut my_scope, &ast);
    // dbg!(result);
    // dbg!(&my_scope.get_value::<ScriptRules>("sample"));
    // dbg!(&my_scope);

    // let rules = my_scope.get_value::<ScriptRules>("sample").unwrap();
    // let result: Result<i32, _> = rules.cb.unwrap().call(&engine, &ast, ((1),));
    // dbg!(result);
}

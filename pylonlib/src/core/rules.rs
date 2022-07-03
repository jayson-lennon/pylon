pub mod fn_pointers;
pub mod matcher;

use std::path::Path;
use std::sync::Arc;

use eyre::WrapErr;
pub use fn_pointers::GlobStore;
pub use matcher::Matcher;
use typed_uri::AssetUri;

use crate::{AbsPath, RelPath};
use serde::Serialize;

use super::{
    engine::GlobalEnginePaths,
    page::lint::{Lint, LintCollection},
};

slotmap::new_key_type! {
    pub struct ContextKey;
}

#[derive(Debug, Clone)]
pub struct Mount {
    src: AbsPath,
    target: AbsPath,
}

impl Mount {
    pub fn new(
        project_root: &AbsPath,
        output_dir: &RelPath,
        src: &RelPath,
        target: &RelPath,
    ) -> Self {
        Self {
            src: project_root.join(src),
            target: project_root.join(output_dir).join(target),
        }
    }
    pub fn src(&self) -> &AbsPath {
        &self.src
    }

    pub fn target(&self) -> &AbsPath {
        &self.target
    }
}

#[derive(Debug, Clone)]
pub struct ExternalWatch<T>
where
    T: std::fmt::Debug + Clone,
{
    command: String,
    working_dir: AbsPath,

    // TODO: for capturing child output / proper cleanup. see:
    // https://github.com/jayson-lennon/pylon/issues/146
    // https://github.com/jayson-lennon/pylon/issues/137
    // https://github.com/jayson-lennon/pylon/issues/122
    _child_process: T,
}

impl ExternalWatch<()> {
    pub fn new<S: Into<String>>(working_dir: &AbsPath, command: S) -> Self {
        let command = command.into();

        Self {
            command,
            working_dir: working_dir.clone(),
            _child_process: (),
        }
    }

    fn full_command(&self) -> String {
        format!(
            "cd {} && {}",
            self.working_dir.as_path().display(),
            self.command
        )
    }

    pub fn run(&self) -> crate::Result<ExternalWatch<Arc<std::process::Child>>> {
        use std::process::Command;
        let child = Arc::new(
            Command::new("sh")
                .arg("-c")
                .arg(self.full_command())
                .spawn()?,
        );
        Ok(ExternalWatch {
            command: self.command.clone(),
            working_dir: self.working_dir.clone(),
            _child_process: child,
        })
    }

    pub fn command(&self) -> &str {
        self.command.as_str()
    }
}

#[derive(Debug, Clone)]
pub struct PylonPipeline {
    pipeline: pipeworks::Pipeline,
    target_glob: crate::util::PylonGlob,
}

impl PylonPipeline {
    pub fn new(pipeline: pipeworks::Pipeline, target_glob: crate::util::PylonGlob) -> Self {
        Self {
            pipeline,
            target_glob,
        }
    }

    pub fn is_match<P: AsRef<Path>>(&self, asset: P) -> bool {
        self.target_glob.is_match(asset)
    }

    pub fn run(&self, asset_uri: &AssetUri) -> crate::Result<()> {
        self.pipeline.run(asset_uri)
    }

    pub fn glob(&self) -> &str {
        self.target_glob.glob()
    }
}

#[derive(Debug, Clone)]
pub struct Rules {
    pipelines: Vec<PylonPipeline>,
    global_context: Option<serde_json::Value>,
    page_contexts: GlobStore<ContextKey, rhai::FnPtr>,
    lints: LintCollection,
    mounts: Vec<Mount>,
    watches: Vec<AbsPath>,
    external_watches: Vec<ExternalWatch<()>>,
    engine_paths: GlobalEnginePaths,
}

impl Rules {
    pub fn new(engine_paths: GlobalEnginePaths) -> Self {
        Self {
            pipelines: vec![],
            global_context: None,
            page_contexts: GlobStore::new(),
            lints: LintCollection::new(),
            mounts: vec![],
            watches: vec![],
            external_watches: vec![],
            engine_paths,
        }
    }
    pub fn set_global_context<S: Serialize>(&mut self, ctx: S) -> crate::Result<()> {
        let ctx = serde_json::to_value(ctx)
            .wrap_err("Failed converting global context to serde value")?;
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

    pub fn add_doc_context(&mut self, matcher: Matcher, ctx_fn: rhai::FnPtr) {
        self.page_contexts.add(matcher, ctx_fn);
    }

    pub fn page_contexts(&self) -> &GlobStore<ContextKey, rhai::FnPtr> {
        &self.page_contexts
    }

    pub fn add_pipeline(&mut self, pipeline: PylonPipeline) {
        self.pipelines.push(pipeline);
    }

    pub fn pipelines(&self) -> impl Iterator<Item = &PylonPipeline> {
        self.pipelines.iter()
    }

    pub fn add_mount(&mut self, src: &RelPath, target: &RelPath) {
        self.mounts.push(Mount::new(
            self.engine_paths.project_root(),
            self.engine_paths.output_dir(),
            src,
            target,
        ));
    }

    pub fn mounts(&self) -> impl Iterator<Item = &Mount> {
        self.mounts.iter()
    }

    pub fn add_watch(&mut self, path: &AbsPath) {
        self.watches.push(path.clone());
    }

    pub fn watches(&self) -> impl Iterator<Item = &AbsPath> {
        self.watches.iter()
    }

    pub fn add_external_watch<S: Into<String>>(&mut self, command: S) {
        let paths = self.engine_paths();
        let working_dir = paths.project_root();
        let watch = ExternalWatch::new(working_dir, command);
        self.external_watches.push(watch);
    }

    pub fn external_watches(&self) -> impl Iterator<Item = &ExternalWatch<()>> {
        self.external_watches.iter()
    }

    pub fn engine_paths(&self) -> GlobalEnginePaths {
        self.engine_paths.clone()
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
        let ast = engine
            .compile(script)
            .wrap_err("Failed to compile an AST from rule script")?;
        Ok(Self { engine, ast })
    }

    pub fn run<T: Clone + Send + Sync + 'static, A: rhai::FuncArgs>(
        &self,
        ptr: &rhai::FnPtr,
        args: A,
    ) -> crate::Result<T> {
        ptr.call(&self.engine, &self.ast, args)
            .wrap_err("Failed to call function pointer in rule script")
    }
}

pub mod script {
    #[allow(clippy::wildcard_imports)]
    use rhai::plugin::*;

    #[rhai::export_module]
    pub mod rhai_module {
        use crate::core::rules::{Matcher, Rules};
        use pipeworks::BaseDir;
        use rhai::FnPtr;
        use tracing::trace;
        use typed_path::{AbsPath, RelPath};

        #[rhai_fn(name = "add_pipeline", return_raw)]
        pub fn add_pipeline(
            rules: &mut Rules,
            base_dir: &str,
            target_glob: &str,
            ops: rhai::Array,
        ) -> Result<(), Box<EvalAltResult>> {
            use crate::core::rules::PylonPipeline;
            use crate::util::PylonGlob;
            use std::str::FromStr;

            let mut parsed_ops = vec![];
            for op in ops {
                let op: String = op.into_string()?;
                let op = pipeworks::Operation::from_str(&op)?;
                parsed_ops.push(op);
            }

            let base_dir = if base_dir.starts_with('/') {
                BaseDir::RelativeToRoot(AbsPath::from_absolute(base_dir))
            } else {
                BaseDir::RelativeToDoc(RelPath::from_relative(base_dir))
            };

            let pylon_pipeline = {
                let glob: PylonGlob = {
                    let glob: Result<PylonGlob, _> = target_glob.try_into();
                    glob.map_err(|e| {
                        EvalAltResult::ErrorSystem("failed to parse pipeline glob".into(), e.into())
                    })?
                };

                let pipeline = {
                    let paths = rules.engine_paths();
                    let paths = pipeworks::Paths::new(
                        paths.project_root(),
                        paths.output_dir(),
                        paths.src_dir(),
                    );
                    pipeworks::Pipeline::with_ops(paths, &base_dir, &parsed_ops).map_err(|e| {
                        EvalAltResult::ErrorSystem("failed creating pipeline".into(), e.into())
                    })?
                };
                PylonPipeline::new(pipeline, glob)
            };

            rules.add_pipeline(pylon_pipeline);

            Ok(())
        }

        /// Associates the closure with the given matcher. This closure will be called
        /// and the returned context from the closure will be available in the page template.
        #[rhai_fn(return_raw)]
        pub fn add_doc_context(
            rules: &mut Rules,
            matcher: &str,
            ctx_fn: FnPtr,
        ) -> Result<(), Box<EvalAltResult>> {
            let matcher = crate::util::PylonGlob::try_from(matcher).map_err(|e| {
                EvalAltResult::ErrorSystem("failed processing glob".into(), e.into())
            })?;
            let matcher = Matcher::Glob(vec![matcher]);
            trace!("add page ctx_fn");
            rules.add_doc_context(matcher, ctx_fn);
            Ok(())
        }

        /// Associates the closure with the given matcher. This closure will be called
        /// and the returned context from the closure will be available in the page template.
        #[rhai_fn(return_raw)]
        #[allow(clippy::needless_pass_by_value)]
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

            let matcher = crate::util::PylonGlob::try_from(matcher).map_err(|e| {
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

        #[rhai_fn(return_raw, name = "mount")]
        pub fn mount_at(
            rules: &mut Rules,
            src: &str,
            target: &str,
        ) -> Result<(), Box<EvalAltResult>> {
            trace!("add mount");

            let src = &crate::RelPath::new(src).map_err(|e| {
                EvalAltResult::ErrorSystem("src dir must be relative: {}".into(), e.into())
            })?;

            let target = {
                if target.starts_with('/') {
                    target.strip_prefix('/').unwrap()
                } else {
                    target
                }
            };

            let target = &crate::RelPath::new(target).map_err(|e| {
                EvalAltResult::ErrorSystem("error parsing target directory: {}".into(), e.into())
            })?;

            rules.add_mount(src, target);
            Ok(())
        }

        #[rhai_fn(return_raw, name = "mount")]
        pub fn mount(rules: &mut Rules, src: &str) -> Result<(), Box<EvalAltResult>> {
            trace!("add mount");

            let src = &crate::RelPath::new(src).map_err(|e| {
                EvalAltResult::ErrorSystem(
                    "error parsing source directory for mount: {}".into(),
                    e.into(),
                )
            })?;

            rules.add_mount(src, &RelPath::from_relative(""));
            Ok(())
        }

        #[rhai_fn(return_raw)]
        pub fn watch(rules: &mut Rules, path: &str) -> Result<(), Box<EvalAltResult>> {
            trace!("add watch");

            let path =
                rules
                    .engine_paths()
                    .project_root()
                    .join(&crate::RelPath::new(path).map_err(|e| {
                        EvalAltResult::ErrorSystem(
                            "watch dir must be relative to project root: {}".into(),
                            e.into(),
                        )
                    })?);

            rules.add_watch(&path);

            Ok(())
        }

        #[rhai_fn(return_raw)]
        pub fn external_watch(rules: &mut Rules, command: &str) -> Result<(), Box<EvalAltResult>> {
            trace!("add external watch");

            rules.add_external_watch(command);

            Ok(())
        }
    }
    #[cfg(test)]
    mod test_script {

        #![allow(warnings, unused)]

        use super::rhai_module::*;
        use crate::core::rules::Rules;
        use crate::test::abs;

        #[test]
        fn adds_pipeline() {
            let (paths, tree) = crate::test::simple_init();
            let mut rules = Rules::new(paths);
            let values = vec!["_COPY_".into()];
            add_pipeline(&mut rules, "base", "*", values).expect("failed to add pipeline");
            assert_eq!(rules.pipelines().count(), 1);
        }

        #[test]
        fn adds_mount_at_target() {
            let (paths, tree) = crate::test::simple_init();
            let mut rules = Rules::new(paths);
            mount_at(&mut rules, "src", "target");
            assert_eq!(rules.mounts().count(), 1);
        }

        #[test]
        fn adds_mount_at_root() {
            let (paths, tree) = crate::test::simple_init();
            let mut rules = Rules::new(paths);
            mount(&mut rules, "src");
            assert_eq!(rules.mounts().count(), 1);
        }

        #[test]
        fn adds_watch() {
            let (paths, tree) = crate::test::simple_init();
            let mut rules = Rules::new(paths);
            watch(&mut rules, "test");
            assert_eq!(rules.watches().count(), 1);
        }

        #[test]
        fn adds_external_watch() {
            let (paths, tree) = crate::test::simple_init();
            let mut rules = Rules::new(paths);
            external_watch(&mut rules, "echo test");
            assert_eq!(rules.external_watches().count(), 1);
        }

        #[test]
        fn rejects_bad_pipeline_op() {
            let (paths, tree) = crate::test::simple_init();
            let mut rules = Rules::new(paths);
            let values = vec![1.into()];
            assert!(add_pipeline(&mut rules, "base", "*", values).is_err());
        }

        #[test]
        fn adds_page_context() {
            use temptree::temptree;

            let (ptr, paths) = {
                let rules = r#"
            rules.add_doc_context("**", |doc| { () });
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
                  syntax_themes: {},
                };

                let paths = crate::test::default_test_paths(&tree);

                let engine = crate::core::engine::Engine::new(paths.clone()).unwrap();

                let all = engine
                    .rules()
                    .page_contexts()
                    .iter()
                    .map(|(_, p)| p)
                    .collect::<Vec<_>>();
                (all[0].clone(), paths)
            };
            let mut rules = Rules::new(paths);
            assert!(add_doc_context(&mut rules, "*", ptr).is_ok());
            assert_eq!(rules.page_contexts().iter().count(), 1);
        }
    }
}

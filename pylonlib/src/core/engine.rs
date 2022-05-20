use eyre::{eyre, WrapErr};
use itertools::Itertools;
use serde::Serialize;
use std::{
    collections::HashSet,
    ffi::OsStr,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    thread::JoinHandle,
};
use tracing::{error, instrument, trace, warn};

use crate::{
    core::rules::{RuleProcessor, Rules},
    core::script_engine::ScriptEngine,
    core::{script_engine::ScriptEngineConfig, Page, PageStore},
    devserver::{broker::RenderBehavior, DevServer, EngineBroker},
    discover::html_asset::{HtmlAsset, HtmlAssets},
    render::Renderers,
    AbsPath, CheckedFile, RelPath, Result, SysPath,
};

use super::{
    page::{lint::LintResults, LintResult, RenderedPage, RenderedPageCollection},
    rules::Mount,
};

pub mod step;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PipelineBehavior {
    Overwrite,
    NoOverwrite,
}

pub type GlobalEnginePaths = Arc<EnginePaths>;

#[derive(Debug, Clone, Serialize)]
pub struct EnginePaths {
    pub rule_script: RelPath,
    pub src_dir: RelPath,
    pub syntax_theme_dir: RelPath,
    pub output_dir: RelPath,
    pub template_dir: RelPath,
    pub project_root: AbsPath,
}

impl EnginePaths {
    pub fn rule_script(&self) -> &RelPath {
        &self.rule_script
    }
    pub fn absolute_rule_script(&self) -> AbsPath {
        self.project_root.join(self.rule_script())
    }

    pub fn src_dir(&self) -> &RelPath {
        &self.src_dir
    }
    pub fn absolute_src_dir(&self) -> AbsPath {
        self.project_root.join(self.src_dir())
    }

    pub fn syntax_theme_dir(&self) -> &RelPath {
        &self.syntax_theme_dir
    }
    pub fn absolute_syntax_theme_dir(&self) -> AbsPath {
        self.project_root.join(self.syntax_theme_dir())
    }

    pub fn output_dir(&self) -> &RelPath {
        &self.output_dir
    }
    pub fn absolute_output_dir(&self) -> AbsPath {
        self.project_root.join(self.output_dir())
    }

    pub fn template_dir(&self) -> &RelPath {
        &self.template_dir
    }
    pub fn absolute_template_dir(&self) -> AbsPath {
        self.project_root.join(self.template_dir())
    }

    pub fn project_root(&self) -> &AbsPath {
        &self.project_root
    }
}

#[derive(Debug)]
pub struct Engine {
    paths: Arc<EnginePaths>,
    renderers: Renderers,

    // these are reset when the user script is updated
    script_engine: ScriptEngine,
    rules: Rules,
    rule_processor: RuleProcessor,

    // Contains all the site pages. Will be updated when needed
    // if running in devserver mode.
    page_store: PageStore,
}

impl Engine {
    pub fn renderers(&self) -> &Renderers {
        &self.renderers
    }
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    pub fn page_store(&self) -> &PageStore {
        &self.page_store
    }

    pub fn page_store_mut(&mut self) -> &mut PageStore {
        &mut self.page_store
    }

    pub fn rule_processor(&self) -> &RuleProcessor {
        &self.rule_processor
    }

    pub fn paths(&self) -> Arc<EnginePaths> {
        Arc::clone(&self.paths)
    }

    pub fn with_broker<S: Into<SocketAddr> + std::fmt::Debug>(
        paths: Arc<EnginePaths>,
        bind: S,
        debounce_ms: u64,
        render_behavior: RenderBehavior,
    ) -> Result<(JoinHandle<Result<()>>, EngineBroker)> {
        let bind = bind.into();

        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_io()
                .enable_time()
                .build()
                .wrap_err("failed starting up tokio runtime when initializing broker")?,
        );

        let broker = EngineBroker::new(rt, render_behavior, paths.clone());
        let broker_clone = broker.clone();

        let engine_handle = broker
            .spawn_engine_thread(paths, bind, debounce_ms)
            .wrap_err("failed spawning engine thread when starting up broker")?;

        Ok((engine_handle, broker_clone))
    }

    pub fn new(paths: Arc<EnginePaths>) -> Result<Engine> {
        let renderers = Renderers::new(&paths.absolute_template_dir()).wrap_err_with(|| {
            format!(
                "failed initializing renderers using template root '{}'",
                paths.absolute_template_dir().display()
            )
        })?;

        let page_store = step::build_page_store(paths.clone(), &renderers).wrap_err_with(|| {
            format!(
                "failed building page store when initializing engine with engine paths '{:?}'",
                paths
            )
        })?;

        let (script_engine, rule_processor, rules) = step::load_rules(paths.clone(), &page_store)
            .wrap_err_with(|| {
            format!(
                "failed loading rule script when initializing engine with paths '{:?}'",
                paths
            )
        })?;

        Ok(Self {
            paths,
            renderers,

            script_engine,
            rules,
            rule_processor,

            page_store,
        })
    }

    pub fn reload_rules(&mut self) -> Result<()> {
        let (script_engine, rule_processor, rules) =
            step::load_rules(self.paths(), &self.page_store).wrap_err("failed to reload rules")?;
        self.script_engine = script_engine;
        self.rule_processor = rule_processor;
        self.rules = rules;
        Ok(())
    }

    pub fn reload_template_engines(&mut self) -> Result<()> {
        self.renderers.tera_mut().reload()?;
        Ok(())
    }

    pub fn rebuild_page_store(&mut self) -> Result<()> {
        trace!("rebuilding the page store");
        self.page_store = step::build_page_store(self.paths(), &self.renderers)
            .wrap_err("Failed to rebuild the page store")?;
        Ok(())
    }

    pub fn re_init(&mut self) -> Result<()> {
        trace!("rebuilding everything");
        self.reload_template_engines()
            .wrap_err("Failed to reload template engines during re-init")?;
        self.rebuild_page_store()
            .wrap_err("Failed to rebuild the page store during re-init")?;
        self.reload_rules()
            .wrap_err("Failed to reload site rules during re-init")?;
        Ok(())
    }

    pub fn build_site(&self) -> Result<()> {
        use tap::prelude::*;
        trace!("running build");

        let pages = self
            .page_store()
            .iter()
            .map(|(_, page)| page)
            .collect::<Vec<_>>();

        // lints
        step::run_lints(self, pages.iter().copied())
            .wrap_err("Failed getting lints while building site")
            .and_then(|lints| step::report::lints(&lints))?;

        // rendering
        step::render(self, pages.iter().copied())
            .wrap_err("Failed to render pages during site build")?
            .write_to_disk()
            .wrap_err("Failed to write rendered pages to disk during site build")?;

        // mounts
        step::mount_directories(self.rules().mounts())
            .wrap_err("Failed to process mounts during site build")?;

        // build list of assets needed for the site (stuff linked in HTML pages)
        let missing_assets = step::get_all_html_output_files(self)
            .wrap_err("Failed to discover HTML files during site build")
            .and_then(|files| step::build_required_asset_list(self, files.iter()))
            .wrap_err("Failed to discover HTML assets during site build")
            .map(|mut assets| {
                // We don't care about links that exist offsite (for now)
                assets.drop_offsite();
                assets
            })?
            .into_iter()
            .filter(|asset| !asset.path().target().exists())
            .collect::<HtmlAssets>();

        // run pipelines
        step::run_pipelines(self, &missing_assets)
            .wrap_err("Failed to run pipelines during site build")?
            .pipe(step::find_unpipelined_assets)
            .pipe(step::report::missing_assets)?;

        Ok(())
    }

    pub fn start_devserver(
        &self,
        bind: SocketAddr,
        debounce_ms: u64,
        engine_broker: EngineBroker,
    ) -> Result<DevServer> {
        use crate::devserver;
        use std::time::Duration;

        let engine = self;

        trace!("starting devserver");

        // spawn filesystem monitoring thread
        {
            let paths = engine.paths();

            let watch_dirs = {
                let mut dirs = vec![
                    paths.absolute_template_dir(),
                    paths.absolute_src_dir(),
                    paths.absolute_rule_script(),
                ];

                #[allow(clippy::redundant_closure_for_method_calls)]
                dirs.extend(engine.rules().mounts().map(|mount| mount.src().clone()));
                dirs.extend(engine.rules().watches().map(|path| path.clone()));
                dirs
            };

            devserver::fswatcher::start_watching(
                &watch_dirs,
                engine_broker.clone(),
                Duration::from_millis(debounce_ms),
            )
            .wrap_err("Failed to start watching directories when starting devserver")?;
        }

        let devserver = DevServer::run(engine_broker, engine.paths().output_dir(), bind);
        Ok(devserver)
    }
}

#[cfg(test)]
pub mod test {

    #![allow(warnings, unused)]

    use crate::devserver::broker::EngineMsg;
    use tracing_test::traced_test;

    use temptree::temptree;

    use super::*;

    // this makes traced_test happy
    use std::result::Result;

    #[test]
    fn makes_new_engine() {
        let (paths, tree) = crate::test::simple_init();
        Engine::new(paths).expect("should be able to make new engine");
    }

    #[test]
    fn makes_new_engine_with_broker() {
        use std::str::FromStr;

        let (paths, tree) = crate::test::simple_init();

        let (engine_handle, broker) = Engine::with_broker(
            paths,
            SocketAddr::from_str("127.0.0.1:9999").unwrap(),
            200,
            RenderBehavior::Memory,
        )
        .expect("failed to create engine with broker");

        broker
            .send_engine_msg_sync(EngineMsg::Quit)
            .expect("failed to send Quit message to engine");

        engine_handle
            .join()
            .expect("failed to join on engine thread")
            .expect("engine returned an error during initialization when it should return Ok");
    }

    #[test]
    fn gets_renderers() {
        let (paths, tree) = crate::test::simple_init();
        let engine = Engine::new(paths).unwrap();
        assert!(std::ptr::eq(engine.renderers(), &engine.renderers));
    }

    #[test]
    fn gets_mutable_page_store() {
        let (paths, tree) = crate::test::simple_init();
        let mut engine = Engine::new(paths).unwrap();
        assert!(std::ptr::eq(engine.page_store_mut(), &engine.page_store));
    }

    #[test]
    fn reloads_rules() {
        let old_rules = r#"
            rules.add_lint(DENY, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
            rules.add_lint(WARN, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
        "#;

        let new_rules = r#"
            rules.add_lint(WARN, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
        "#;

        let tree = temptree! {
          "rules.rhai": old_rules,
          templates: {
              "default.tera": "",
          },
          target: {},
          src: {},
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let mut engine = Engine::new(paths).unwrap();
        assert_eq!(engine.rules().lints().len(), 2);

        std::fs::write(tree.path().join("rules.rhai"), new_rules)
            .expect("failed to write new rules");

        engine.reload_rules().expect("failed to reload rules");
        assert_eq!(engine.rules().lints().len(), 1);
    }

    #[test]
    fn reloads_template_engines() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {
              "a.tera": "",
          },
          target: {},
          src: {},
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let mut engine = Engine::new(paths).unwrap();
        assert_eq!(engine.renderers().tera().get_template_names().len(), 1);

        let mut new_template = tree.path().join("templates");
        new_template.push("b.tera");

        std::fs::write(new_template, "").unwrap();

        engine
            .reload_template_engines()
            .expect("failed to reload template engines");

        assert_eq!(engine.renderers().tera().get_template_names().len(), 2);
    }

    #[test]
    fn does_lint() {
        let rules = r#"
            rules.add_lint(WARN, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
            rules.add_lint(WARN, "Missing author 2", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
        "#;

        let doc = r#"+++
            template_name = "empty.tera"
            +++
        "#;

        let tree = temptree! {
          "rules.rhai": rules,
          templates: {},
          target: {},
          src: {
              "sample.md": doc,
          },
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let engine = Engine::new(paths).unwrap();

        let lints = step::run_lints(&engine, engine.page_store().iter().map(|(_, page)| page))
            .expect("linting failed");
        assert_eq!(lints.into_iter().count(), 2);
    }

    #[test]
    fn does_render() {
        let doc1 = r#"+++
            template_name = "test.tera"
            +++
doc1"#;

        let doc2 = r#"+++
            template_name = "test.tera"
            +++
doc2"#;

        let tree = temptree! {
          "rules.rhai": "",
          templates: {
              "test.tera": "content: {{content}}"
          },
          target: {},
          src: {
              "doc1.md": doc1,
              "doc2.md": doc2,
          },
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let engine = Engine::new(paths).unwrap();

        let rendered = step::render(&engine, engine.page_store().iter().map(|(_, page)| page))
            .expect("failed to render pages");

        assert_eq!(rendered.iter().count(), 2);
    }

    #[test]
    fn aborts_render_when_assets_are_missing() {
        let doc = r#"+++
            template_name = "test.tera"
            +++"#;

        let tree = temptree! {
          "rules.rhai": "",
          templates: {
              "test.tera": r#"<img src="missing.png">"#,
          },
          target: {},
          src: {
              "doc.md": doc,
          },
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let engine = Engine::new(paths).unwrap();

        assert!(engine.build_site().is_err());
    }

    #[test]
    fn renders_properly_when_assets_are_available() {
        let doc = r#"+++
            template_name = "test.tera"
            +++"#;

        let tree = temptree! {
          "rules.rhai": "",
          templates: {
              "test.tera": r#"<img src="found_it.png">"#,
          },
          target: {},
          src: {
              "doc.md": doc,
              "found_it.png": "",
          },
          syntax_themes: {}
        };

        let rules = r#"rules.add_pipeline(".", "**/*.png", ["[COPY]"]);"#;

        let rule_script = tree.path().join("rules.rhai");
        std::fs::write(&rule_script, rules).unwrap();

        let paths = crate::test::default_test_paths(&tree);

        let engine = Engine::new(paths).unwrap();

        engine.build_site().expect("failed to build site");
    }

    #[test]
    fn doesnt_reprocess_existing_assets() {
        let doc = r#"+++
            template_name = "test.tera"
            +++"#;

        let tree = temptree! {
          "rules.rhai": "",
          templates: {
              "test.tera": r#"<img src="/found_it.png">"#,
          },
          target: {},
          src: {
              "doc.md": doc,
          },
          wwwroot: {
              "found_it.png": "",
          },
          syntax_themes: {}
        };

        let rules = r#"
                rules.mount("wwwroot", "target");
                rules.add_pipeline(".", "**/*.png", ["[COPY]"]);"#;

        let rule_script = tree.path().join("rules.rhai");
        std::fs::write(&rule_script, rules).unwrap();

        let paths = crate::test::default_test_paths(&tree);

        let engine = Engine::new(paths).unwrap();

        // Here we go through the site building process manually in order to arrive
        // at the point where the pipelines are being processed. If the pipeline
        // processing returns an empty `LinkedAssets` structure, then this test
        // was successful. Since the file under test was copied via `mount`, the
        // pipeline should skip processing. If the pipeline returns the asset,
        // then this indicates a test failure because the asset should have been
        // located before running the pipeline.
        {
            let pages = engine.page_store().iter().map(|(_, page)| page);
            let rendered = step::render(&engine, pages).expect("failed to render");

            step::mount_directories(engine.rules().mounts()).expect("failed to process mounts");

            let html_assets =
                crate::discover::html_asset::find_all(engine.paths(), engine.paths().output_dir())
                    .expect("failed to discover html assets");

            let unhandled_assets =
                step::run_pipelines(&engine, &html_assets).expect("failed to run pipelines");

            assert!(unhandled_assets.is_empty());
        }
    }

    #[test]
    fn rebuilds_page_store() {
        let doc1 = r#"+++
            template_name = "empty.tera"
            +++
        "#;

        let doc2 = r#"+++
            template_name = "empty.tera"
            +++
        "#;

        let tree = temptree! {
          "rules.rhai": "",
          templates: {
              "empty.tera": ""
          },
          target: {},
          src: {
              "doc1.md": doc1,
          },
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let mut engine = Engine::new(paths).unwrap();

        let page_store = engine.page_store();
        assert_eq!(page_store.iter().count(), 1);

        std::fs::write(tree.path().join("src/doc2.md"), doc2).expect("failed to write new doc");

        engine
            .rebuild_page_store()
            .expect("failed to rebuild page store");
        let page_store = engine.page_store();
        assert_eq!(page_store.iter().count(), 2);
    }

    #[test]
    fn re_inits_everything() {
        let (paths, tree) = crate::test::simple_init();
        let mut engine = Engine::new(paths).unwrap();
        assert!(engine.re_init().is_ok());
    }

    #[traced_test]
    #[test]
    fn builds_site_no_lint_errors() {
        let doc1 = r#"+++
            template_name = "empty.tera"
            +++
        "#;

        let doc2 = r#"+++
            template_name = "test.tera"
            [meta]
            author = "test"
            +++
        "#;

        let tree = temptree! {
          "rules.rhai": "",
          templates: {
              "test.tera": r#"<img src="blank.png">"#,
              "empty.tera": "",
              "default.tera": "",
          },
          target: {},
          src: {
              "doc1.md": doc1,
              "doc2.md": doc2,
              "blank.png": "test",
          },
          syntax_themes: {}
        };

        let rules = r#"
            rules.add_lint(WARN, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
            rules.add_pipeline(".", "**/*.png", ["[COPY]"]);
            "#;

        let rule_script = tree.path().join("rules.rhai");
        std::fs::write(&rule_script, &rules).unwrap();

        let paths = crate::test::default_test_paths(&tree);

        let engine = Engine::new(paths).unwrap();
        engine.build_site().expect("failed to build site");

        let target_doc1 = tree.path().join("target").join("doc1.html");

        let target_doc2 = tree.path().join("target").join("doc2.html");

        let target_img = tree.path().join("target").join("blank.png");

        assert!(target_doc1.exists());
        assert!(target_doc2.exists());
        assert!(target_img.exists());
    }

    #[test]
    fn aborts_site_build_with_deny_lint_error() {
        let rules = r#"
            rules.add_lint(DENY, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
            rules.add_pipeline("base", "**/*.png", ["[COPY]"]);
        "#;

        let doc1 = r#"+++
            template_name = "empty.tera"
            +++
        "#;

        let doc2 = r#"+++
            template_name = "test.tera"
            [meta]
            author = "test"
            +++
        "#;

        let tree = temptree! {
          "rules.rhai": rules,
          templates: {
              "test.tera": r#"<img src="blank.png">"#,
              "empty.tera": ""
          },
          target: {},
          src: {
              "doc1.md": doc1,
              "doc2.md": doc2,
              "blank.png": "",
          },
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let engine = Engine::new(paths).unwrap();
        assert!(engine.build_site().is_err());
    }

    #[test]
    fn copies_mounts() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {},
          src: {},
          wwwroot: {
              "file_1": "data",
              inner: {
                  "file_2": "data"
              }
          },
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let rules = r#"
                rules.mount("wwwroot");
            "#;

        let rule_script = tree.path().join("rules.rhai");
        std::fs::write(&rule_script, rules).unwrap();

        let engine = Engine::new(paths).unwrap();
        engine.build_site().expect("failed to build site");

        {
            let mut wwwroot = tree.path().join("target");
            wwwroot.push("wwwroot");
            assert!(!wwwroot.exists());

            let mut file_1 = tree.path().join("target");
            file_1.push("file_1");
            assert!(file_1.exists());

            let mut file_2 = tree.path().join("target");
            file_2.push("inner");
            file_2.push("file_2");
            assert!(file_2.exists());
        }
    }

    #[test]
    fn copies_mounts_inner() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {},
          src: {},
          wwwroot: {
              "file_1": "data",
              inner: {
                  "file_2": "data"
              }
          },
          syntax_themes: {}
        };

        let paths = crate::test::default_test_paths(&tree);

        let rules = r#"
                rules.mount("wwwroot", "inner");
            "#;

        let rule_script = tree.path().join("rules.rhai");
        std::fs::write(&rule_script, rules).unwrap();

        let engine = Engine::new(paths).unwrap();
        engine.build_site().expect("failed to build site");

        {
            let mut wwwroot = tree.path().join("target/inner");
            wwwroot.push("wwwroot");
            assert!(!wwwroot.exists());

            let mut file_1 = tree.path().join("target/inner");
            file_1.push("file_1");
            assert!(file_1.exists());

            let mut file_2 = tree.path().join("target/inner");
            file_2.push("inner");
            file_2.push("file_2");
            assert!(file_2.exists());
        }
    }
}

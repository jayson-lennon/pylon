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

    #[instrument]
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
                .with_context(|| "failed starting up tokio runtime when initializing broker")?,
        );

        let broker = EngineBroker::new(rt, render_behavior, paths.clone());
        let broker_clone = broker.clone();

        let engine_handle = broker
            .spawn_engine_thread(paths, bind, debounce_ms)
            .with_context(|| "failed spawning engine thread when starting up broker")?;

        Ok((engine_handle, broker_clone))
    }

    #[instrument]
    pub fn new(paths: Arc<EnginePaths>) -> Result<Engine> {
        let renderers = Renderers::new(&paths.absolute_template_dir()).with_context(|| {
            format!(
                "failed initializing renderers using template root '{}'",
                paths.absolute_template_dir().display()
            )
        })?;

        let page_store = do_build_page_store(paths.clone(), &renderers).with_context(|| {
            format!(
                "failed building page store when initializing engine with engine paths '{:?}'",
                paths
            )
        })?;

        let (script_engine, rule_processor, rules) = Self::load_rules(paths.clone(), &page_store)
            .with_context(|| {
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

    #[instrument(ret)]
    pub fn load_rules(
        engine_paths: Arc<EnginePaths>,
        page_store: &PageStore,
    ) -> Result<(ScriptEngine, RuleProcessor, Rules)> {
        let script_engine_config = ScriptEngineConfig::new();
        let script_engine = ScriptEngine::new(&script_engine_config.modules());

        let _project_root = engine_paths.project_root();

        let rule_script = std::fs::read_to_string(engine_paths.absolute_rule_script())
            .with_context(|| {
                format!(
                    "failed reading rule script at '{}'",
                    engine_paths.absolute_rule_script().display()
                )
            })?;

        let (rule_processor, rules) = script_engine
            .build_rules(engine_paths, page_store, rule_script)
            .with_context(|| "failed to build Rules structure")?;

        Ok((script_engine, rule_processor, rules))
    }

    #[instrument(skip(self), ret)]
    pub fn reload_rules(&mut self) -> Result<()> {
        let (script_engine, rule_processor, rules) =
            Self::load_rules(self.paths(), &self.page_store)
                .with_context(|| "failed to reload rules")?;
        self.script_engine = script_engine;
        self.rule_processor = rule_processor;
        self.rules = rules;
        Ok(())
    }

    #[instrument(skip(self), ret)]
    pub fn reload_template_engines(&mut self) -> Result<()> {
        self.renderers.tera.reload()?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn run_pipelines<'a>(
        &self,
        html_assets: &'a HtmlAssets,
        behavior: PipelineBehavior,
    ) -> Result<HashSet<&'a HtmlAsset>> {
        trace!("running pipelines");

        let mut unhandled_assets = HashSet::new();

        for asset in html_assets {
            // Ignore anchor links for now. Issue https://github.com/jayson-lennon/pylon/issues/75
            // to eventually make this work.
            if asset.tag() == "a" {
                continue;
            }

            // Ignore any assets that already exist in the target directory.
            {
                if behavior == PipelineBehavior::NoOverwrite && asset.path().target().exists() {
                    continue;
                }
            }

            // tracks which assets have no processing logic
            let mut asset_has_pipeline = false;

            for pipeline in self.rules().pipelines() {
                if pipeline.is_match(asset.uri().as_str()) {
                    // asset has an associate pipeline, so we won't report an error
                    asset_has_pipeline = true;

                    dbg!(&asset);
                    let asset_uri = asset.uri();
                    let relative_asset = &asset_uri.as_str()[1..];
                    // Make a new target in order to create directories for the asset.
                    let mut target_dir = PathBuf::from(self.paths().output_dir());
                    target_dir.push(relative_asset);

                    let target_dir = target_dir.parent().expect("should have parent directory");
                    let target_dir = AbsPath::new(
                        self.paths()
                            .absolute_output_dir()
                            .join(&RelPath::new(target_dir)?),
                    )?;
                    crate::util::make_parent_dirs(&target_dir)?;

                    pipeline.run(asset.uri())?;
                }
            }
            if !asset_has_pipeline {
                unhandled_assets.insert(asset);
            }
        }
        Ok(unhandled_assets)
    }

    #[instrument(skip_all)]
    pub fn lint<'a, P: Iterator<Item = &'a Page>>(&self, pages: P) -> Result<LintResults> {
        trace!("linting");
        let engine: &Engine = self;

        let lint_results: Vec<Vec<LintResult>> = pages
            .map(|page| {
                crate::core::page::lint(engine.rule_processor(), engine.rules().lints(), page)
            })
            .try_collect()?;

        let lint_results = lint_results.into_iter().flatten();

        Ok(LintResults::from_iter(lint_results))
    }

    #[instrument(skip_all)]
    pub fn render<'a, P: Iterator<Item = &'a Page>>(
        &self,
        pages: P,
    ) -> Result<RenderedPageCollection> {
        trace!("rendering");

        let engine: &Engine = self;

        let rendered: Vec<RenderedPage> = pages
            .map(|page| crate::core::page::render(engine, page))
            .try_collect()?;

        Ok(RenderedPageCollection::from_vec(rendered))
    }

    #[instrument(skip_all)]
    pub fn process_mounts<'a, M: Iterator<Item = &'a Mount>>(&self, mounts: M) -> Result<()> {
        use fs_extra::dir::CopyOptions;
        for mount in mounts {
            dbg!(&mount);
            trace!(mount=?mount, "processing mount");
            std::fs::create_dir_all(mount.target())?;
            let options = CopyOptions {
                copy_inside: true,
                skip_exist: true,
                content_only: true,
                ..CopyOptions::default()
            };
            fs_extra::dir::copy(mount.src(), mount.target(), &options)?;
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn rebuild_page_store(&mut self) -> Result<()> {
        trace!("rebuilding the page store");
        self.page_store = do_build_page_store(self.paths(), &self.renderers)?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn re_init(&mut self) -> Result<()> {
        trace!("rebuilding everything");
        self.reload_template_engines()?;
        self.rebuild_page_store()?;
        self.reload_rules()?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn build_site(&self) -> Result<()> {
        use crate::core::page::lint::LintLevel;

        trace!("running build");

        let pages = self.page_store().iter().map(|(_, page)| page);

        {
            trace!("running lints");
            let lints = self.lint(pages.clone())?;
            let mut abort = false;
            for lint in lints {
                match lint.level {
                    LintLevel::Warn => warn!(%lint.msg),
                    LintLevel::Deny => {
                        error!(%lint.msg);
                        abort = true;
                    }
                }
            }
            if abort {
                return Err(eyre!("lint errors encountered while building site"));
            }
        }

        trace!("rendering pages");
        let rendered = self.render(pages)?;
        dbg!(&rendered);

        trace!("writing rendered pages to disk");
        rendered.write_to_disk()?;

        trace!("processing mounts");
        self.process_mounts(self.rules().mounts())?;

        {
            trace!("locating HTML assets");
            let mut html_assets =
                crate::discover::html_asset::find_all(self.paths(), self.paths().output_dir())?;
            html_assets.drop_offsite();
            dbg!(&html_assets);

            trace!("running pipelines");
            let unhandled_assets =
                self.run_pipelines(&html_assets, PipelineBehavior::NoOverwrite)?;
            // check for missing assets in pages
            {
                for asset in &unhandled_assets {
                    error!(asset = ?asset, "missing asset or no pipeline defined");
                }
                if !unhandled_assets.is_empty() {
                    return Err(eyre!("one or more assets are missing"));
                }
            }
        }

        {
            // TODO: check for missing assets in CSS
        }
        Ok(())
    }

    #[instrument(skip(self, engine_broker))]
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
            )?;
        }

        let devserver = DevServer::run(engine_broker, engine.paths().output_dir(), bind);
        Ok(devserver)
    }
}

#[instrument(skip(renderers), ret)]
fn do_build_page_store(engine_paths: Arc<EnginePaths>, renderers: &Renderers) -> Result<PageStore> {
    dbg!(&engine_paths);
    let pages: Vec<_> =
        crate::discover::get_all_paths(&engine_paths.absolute_src_dir(), &|path: &Path| -> bool {
            path.extension() == Some(OsStr::new("md"))
        })?
        .iter()
        .map(|abs_path| {
            let root = engine_paths.project_root();
            let base = engine_paths.src_dir();
            let target = abs_path.strip_prefix(root.join(base))?;
            let checked_file_path = SysPath::new(root, base, &target).to_checked_file()?;
            Page::from_file(engine_paths.clone(), checked_file_path, renderers)
        })
        .try_collect()?;

    dbg!(&pages);
    let mut page_store = PageStore::new();
    page_store.insert_batch(pages);
    dbg!(&page_store);

    Ok(page_store)
}

#[cfg(test)]
pub mod test {

    #![allow(warnings, unused)]

    use crate::devserver::broker::EngineMsg;

    use temptree::temptree;

    use super::*;

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
        assert_eq!(engine.renderers().tera.get_template_names().count(), 1);

        let mut new_template = tree.path().join("templates");
        new_template.push("b.tera");

        std::fs::write(new_template, "").unwrap();

        engine
            .reload_template_engines()
            .expect("failed to reload template engines");

        assert_eq!(engine.renderers().tera.get_template_names().count(), 2);
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

        let lints = engine
            .lint(engine.page_store().iter().map(|(_, page)| page))
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

        let rendered = engine
            .render(engine.page_store().iter().map(|(_, page)| page))
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
            let rendered = engine.render(pages).expect("failed to render");

            engine
                .process_mounts(engine.rules().mounts())
                .expect("failed to process mounts");

            let html_assets =
                crate::discover::html_asset::find_all(engine.paths(), engine.paths().output_dir())
                    .expect("failed to discover html assets");

            let unhandled_assets = engine
                .run_pipelines(&html_assets, PipelineBehavior::Overwrite)
                .expect("failed to run pipelines");

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
        dbg!(&paths);

        let rules = r#"
                rules.mount("wwwroot", "target");
            "#;

        dbg!(&rules);

        let rule_script = tree.path().join("rules.rhai");
        dbg!(&rule_script);
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
}

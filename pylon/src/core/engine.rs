use itertools::Itertools;
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
    core::config::EngineConfig,
    core::rules::{RuleProcessor, Rules},
    core::script_engine::ScriptEngine,
    core::{script_engine::ScriptEngineConfig, Page, PageStore},
    devserver::{DevServer, EngineBroker},
    discover::{
        html_asset::{HtmlAsset, HtmlAssets},
        UrlType,
    },
    render::Renderers,
    util, Result,
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

#[derive(Debug)]
pub struct Engine {
    config: EngineConfig,
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

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    #[instrument]
    pub fn with_broker<S: Into<SocketAddr> + std::fmt::Debug>(
        config: EngineConfig,
        bind: S,
        debounce_ms: u64,
    ) -> Result<(JoinHandle<Result<()>>, EngineBroker)> {
        let bind = bind.into();

        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_io()
                .enable_time()
                .build()?,
        );

        let broker = EngineBroker::new(rt);
        let broker_clone = broker.clone();

        let engine_handle = broker.spawn_engine_thread(config, bind, debounce_ms)?;

        Ok((engine_handle, broker_clone))
    }

    #[instrument]
    pub fn new(config: EngineConfig) -> Result<Engine> {
        let renderers = Renderers::new(&config.template_root);

        let page_store = do_build_page_store(&config.src_root, &config.target_root, &renderers)?;

        let (script_engine, rule_processor, rules) =
            Self::load_rules(&config.rule_script, &page_store)?;

        Ok(Self {
            config,
            renderers,

            script_engine,
            rules,
            rule_processor,

            page_store,
        })
    }

    #[instrument(ret)]
    pub fn load_rules<P: AsRef<Path> + std::fmt::Debug>(
        rule_script: P,
        page_store: &PageStore,
    ) -> Result<(ScriptEngine, RuleProcessor, Rules)> {
        let script_engine_config = ScriptEngineConfig::new();
        let script_engine = ScriptEngine::new(&script_engine_config.modules());

        let rule_script = std::fs::read_to_string(&rule_script)?;

        let (rule_processor, rules) = script_engine.build_rules(page_store, rule_script)?;

        Ok((script_engine, rule_processor, rules))
    }

    #[instrument(skip(self), ret)]
    pub fn reload_rules(&mut self) -> Result<()> {
        let (script_engine, rule_processor, rules) =
            Self::load_rules(&self.config.rule_script, &self.page_store)?;
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

        let engine: &Engine = self;

        let mut unhandled_assets = HashSet::new();

        for asset in html_assets {
            // Ignore anchor links for now. Issue https://github.com/jayson-lennon/pylon/issues/75
            // to eventually make this work.
            if asset.tag() == "a" {
                continue;
            }

            // Ignore any assets that already exist in the target directory.
            {
                if behavior == PipelineBehavior::NoOverwrite {
                    match asset.url_type() {
                        UrlType::Relative(abs) => {
                            let target = PathBuf::from(&abs[1..]);
                            if target.exists() {
                                continue;
                            }
                        }
                        _ => {
                            let mut target_sys_path = PathBuf::from(&self.config().target_root);
                            let relative_uri = PathBuf::from(&asset.uri().as_str()[1..]);
                            target_sys_path.push(relative_uri);
                            if target_sys_path.exists() {
                                continue;
                            }
                        }
                    }
                }
            }

            // tracks which assets have no processing logic
            let mut asset_has_pipeline = false;

            for pipeline in engine.rules.pipelines() {
                if pipeline.is_match(asset.uri().as_str()) {
                    // asset has an associate pipeline, so we won't report an error
                    asset_has_pipeline = true;

                    let asset_uri = &asset.uri();
                    let relative_asset = &asset_uri.as_str()[1..];
                    // Make a new target in order to create directories for the asset.
                    let mut target_dir = PathBuf::from(&engine.config.target_root);
                    target_dir.push(relative_asset);
                    let target_dir = target_dir.parent().expect("should have parent directory");
                    util::make_parent_dirs(target_dir)?;
                    dbg!(&target_dir);
                    pipeline.run(
                        &engine.config.src_root,
                        &engine.config.target_root,
                        relative_asset,
                    )?;
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
        self.page_store = do_build_page_store(
            &self.config.src_root,
            &self.config.target_root,
            &self.renderers,
        )?;
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
                return Err(anyhow::anyhow!(
                    "lint errors encountered while building site"
                ));
            }
        }

        trace!("rendering pages");
        let rendered = self.render(pages)?;

        trace!("writing rendered pages to disk");
        rendered.write_to_disk()?;

        trace!("processing mounts");
        self.process_mounts(self.rules().mounts())?;

        {
            trace!("locating HTML assets");
            let mut html_assets = crate::discover::html_asset::find_all(&self.config.target_root)?;
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
                    return Err(anyhow::anyhow!("one or more assets are missing"));
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
            let watch_dirs = {
                let mut dirs = vec![
                    engine.config().template_root(),
                    engine.config().src_root(),
                    engine.config().rule_script(),
                ];

                #[allow(clippy::redundant_closure_for_method_calls)]
                dirs.extend(engine.rules().mounts().map(|mount| mount.src()));
                dirs.extend(engine.rules().watches());
                dirs
            };

            devserver::fswatcher::start_watching(
                &watch_dirs,
                engine_broker.clone(),
                Duration::from_millis(debounce_ms),
            )?;
        }

        let devserver = DevServer::run(engine_broker, engine.config().target_root(), bind);
        Ok(devserver)
    }
}

#[instrument(skip(renderers), ret)]
fn do_build_page_store<P: AsRef<Path> + std::fmt::Debug>(
    src_root: P,
    target_root: P,
    renderers: &Renderers,
) -> Result<PageStore> {
    let src_root = src_root.as_ref();
    let target_root = target_root.as_ref();

    let pages: Vec<_> = crate::discover::get_all_paths(src_root, &|path: &Path| -> bool {
        path.extension() == Some(OsStr::new("md"))
    })?
    .iter()
    .map(|path| {
        let path = path.strip_prefix(src_root).unwrap();
        Page::from_file(src_root, target_root, path, renderers)
    })
    .try_collect()?;

    let mut page_store = PageStore::new();
    page_store.insert_batch(pages);

    Ok(page_store)
}

#[cfg(test)]
pub mod test {
    #![allow(unused_variables)]

    use crate::devserver::broker::EngineMsg;
    use tempfile::TempDir;
    use temptree::temptree;

    use super::*;

    // `TempDir` needs to stay bound in order to maintain temporary directory tree
    fn simple_config() -> (EngineConfig, TempDir) {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {},
          src: {}
        };
        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );
        (config, tree)
    }

    #[test]
    fn makes_new_engine() {
        let (config, tree) = simple_config();
        Engine::new(config).expect("should be able to make new engine");
    }

    #[test]
    fn makes_new_engine_with_broker() {
        use std::str::FromStr;

        let (config, tree) = simple_config();

        let (engine_handle, broker) =
            Engine::with_broker(config, SocketAddr::from_str("127.0.0.1:9999").unwrap(), 200)
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
        let (config, tree) = simple_config();
        let engine = Engine::new(config).unwrap();
        assert!(std::ptr::eq(engine.renderers(), &engine.renderers));
    }

    #[test]
    fn gets_mutable_page_store() {
        let (config, tree) = simple_config();
        let mut engine = Engine::new(config).unwrap();
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
          "old_rules.rhai": old_rules,
          "new_rules.rhai": new_rules,
          templates: {},
          target: {},
          src: {}
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("old_rules.rhai"),
        );

        let mut engine = Engine::new(config).unwrap();
        assert_eq!(engine.rules().lints().len(), 2);

        engine.config.rule_script = tree.path().join("new_rules.rhai");
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
          src: {}
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let mut engine = Engine::new(config).unwrap();
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
          }
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();

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
          }
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();

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
          }
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();

        assert!(engine.build_site().is_err());
    }

    #[test]
    fn renders_properly_when_assets_are_available() {
        let doc = r#"+++
            template_name = "test.tera"
            +++"#;
        let rules = r#"rules.add_pipeline("**/*.png", ["[COPY]"]);"#;

        let tree = temptree! {
          "rules.rhai": rules,
          templates: {
              "test.tera": r#"<img src="found_it.png">"#,
          },
          target: {},
          src: {
              "doc.md": doc,
              "found_it.png": "",
          }
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();

        engine.build_site().expect("failed to build site");
    }

    #[test]
    fn locates_css_urls() {
        let tree = temptree! {
          "rules.rhai": "",
          templates: {},
          target: {
              "main.css": r#" @font-face {
                                font-family: "Test";
                                src:
                                    local("Test"),
                                    url("fonts/vendor/test/test.woff2") format("woff2"),
                                    url("fonts/vendor/jost/test.woff") format("woff");
                            }"#,
            },
          src: {}
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();

        assert!(engine.build_site().is_ok());
        panic!();
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
          }
        };

        let rules = {
            let wwwroot = tree.path().join("wwwroot");
            let wwwroot = wwwroot.to_string_lossy();
            let target = tree.path().join("target");
            let target = target.to_string_lossy();
            r#"
                rules.mount("wwwroot", "target");
                rules.add_pipeline("**/*.png", ["[COPY]"]);
            "#
            .replace("wwwroot", &wwwroot)
            .replace("target", &target)
        };

        let rule_script = tree.path().join("rules.rhai");
        std::fs::write(&rule_script, rules).unwrap();

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();

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

            let html_assets = crate::discover::html_asset::find_all(engine.config.target_root())
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
          src_new: {
              "doc1.md": doc1,
              "doc2.md": doc2,
          }
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let mut engine = Engine::new(config).unwrap();

        let page_store = engine.page_store();
        assert_eq!(page_store.iter().count(), 1);

        engine.config.src_root = tree.path().join("src_new");

        engine
            .rebuild_page_store()
            .expect("failed to rebuild page store");
        let page_store = engine.page_store();
        assert_eq!(page_store.iter().count(), 2);
    }

    #[test]
    fn re_inits_everything() {
        let (config, tree) = simple_config();
        let mut engine = Engine::new(config).unwrap();
        assert!(engine.re_init().is_ok());
    }

    #[test]
    fn builds_site_no_lint_errors() {
        let rules = r#"
            rules.add_lint(WARN, "Missing author", "**", |page| {
                page.meta("author") == "" || type_of(page.meta("author")) == "()"
            });
            rules.add_pipeline("**/*.png", ["[COPY]"]);
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
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();
        engine.build_site().expect("failed to build site");

        let mut target_doc1 = tree.path().join("target");
        target_doc1.push("doc1.html");

        let mut target_doc2 = tree.path().join("target");
        target_doc2.push("doc2.html");

        let mut target_img = tree.path().join("target");
        target_img.push("blank.png");

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
            rules.add_pipeline("**/*.png", ["[COPY]"]);
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
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let engine = Engine::new(config).unwrap();
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
          }
        };

        let config = EngineConfig::new(
            tree.path().join("src"),
            tree.path().join("target"),
            tree.path().join("templates"),
            tree.path().join("rules.rhai"),
        );

        let rules = {
            let wwwroot = tree.path().join("wwwroot");
            let wwwroot = wwwroot.to_string_lossy();
            let target = tree.path().join("target");
            let target = target.to_string_lossy();
            r#"
                rules.mount("wwwroot", "target");
            "#
            .replace("wwwroot", &wwwroot)
            .replace("target", &target)
        };

        let rule_script = tree.path().join("rules.rhai");
        std::fs::write(&rule_script, rules).unwrap();

        let engine = Engine::new(config).unwrap();
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

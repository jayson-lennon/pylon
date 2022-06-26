use eyre::WrapErr;
use serde::Serialize;
use std::{net::SocketAddr, sync::Arc, thread::JoinHandle};
use tracing::{debug, info, trace};

use crate::{
    core::rules::{RuleProcessor, Rules},
    core::script_engine::ScriptEngine,
    core::Library,
    devserver::{broker::RenderBehavior, DevServer, EngineBroker},
    discover::html_asset::HtmlAssets,
    render::Renderers,
    AbsPath, RelPath, Result, USER_LOG,
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
    paths: GlobalEnginePaths,
    renderers: Renderers,

    // these are reset when the user script is updated
    script_engine: ScriptEngine,
    rules: Rules,
    rule_processor: RuleProcessor,

    // Contains all the site pages. Will be updated when needed
    // if running in devserver mode.
    library: Library,
}

impl Engine {
    pub fn renderers(&self) -> &Renderers {
        &self.renderers
    }
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn library_mut(&mut self) -> &mut Library {
        &mut self.library
    }

    pub fn rule_processor(&self) -> &RuleProcessor {
        &self.rule_processor
    }

    pub fn paths(&self) -> GlobalEnginePaths {
        Arc::clone(&self.paths)
    }

    pub fn with_broker<S: Into<SocketAddr> + std::fmt::Debug>(
        paths: GlobalEnginePaths,
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

    pub fn new(paths: GlobalEnginePaths) -> Result<Engine> {
        let renderers = Renderers::new(paths.clone()).wrap_err_with(|| {
            format!(
                "failed initializing renderers using template root '{}'",
                paths.absolute_template_dir().display()
            )
        })?;

        let library = step::build_library(paths.clone(), &renderers).wrap_err_with(|| {
            format!(
                "failed building page store when initializing engine with engine paths '{:?}'",
                paths
            )
        })?;

        let (script_engine, rule_processor, rules) = step::load_rules(paths.clone(), &library)
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

            library,
        })
    }

    pub fn reload_rules(&mut self) -> Result<()> {
        info!(target: USER_LOG, "reloading site rules script");

        let (script_engine, rule_processor, rules) =
            step::load_rules(self.paths(), &self.library).wrap_err("failed to reload rules")?;
        self.script_engine = script_engine;
        self.rule_processor = rule_processor;
        self.rules = rules;
        Ok(())
    }

    pub fn reload_template_engines(&mut self) -> Result<()> {
        info!(target: USER_LOG, "reloading template engines");

        self.renderers.tera_mut().reload()?;
        Ok(())
    }

    pub fn rebuild_library(&mut self) -> Result<()> {
        info!(target: USER_LOG, "rebuilding library");

        self.library = step::build_library(self.paths(), &self.renderers)
            .wrap_err("Failed to rebuild the page store")?;
        Ok(())
    }

    pub fn re_init(&mut self) -> Result<()> {
        trace!("rebuilding everything");

        self.reload_template_engines()
            .wrap_err("Failed to reload template engines during re-init")?;
        self.rebuild_library()
            .wrap_err("Failed to rebuild the page store during re-init")?;
        self.reload_rules()
            .wrap_err("Failed to reload site rules during re-init")?;
        Ok(())
    }

    pub fn build_site(&self) -> Result<()> {
        use tap::prelude::*;
        info!(target: USER_LOG, "building site");

        let pages = self
            .library()
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
            .pipe_borrow(step::find_unpipelined_assets)
            .pipe_borrow(step::report::missing_assets)?;

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

        info!(target: USER_LOG, "starting dev server");

        // spawn filesystem monitoring thread
        {
            let paths = engine.paths();

            let watch_dirs = {
                // watch rule script
                let mut dirs = vec![paths.absolute_rule_script()];

                // watch mounts
                dirs.extend(engine.rules().mounts().map(|mount| mount.src().clone()));
                // watches within rule-script
                dirs.extend(engine.rules().watches().cloned());
                dirs
            };

            info!(target: USER_LOG, "starting filesystem event thread");
            devserver::fswatcher::start_watching(
                &watch_dirs,
                engine_broker.clone(),
                Duration::from_millis(debounce_ms),
            )
            .wrap_err("Failed to start watching directories when starting devserver")?;
        }

        let devserver = DevServer::run(engine_broker, engine.paths().output_dir(), bind);
        info!(target: USER_LOG, "devserver now running on {}", bind);

        Ok(devserver)
    }
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
    fn gets_mutable_library() {
        let (paths, tree) = crate::test::simple_init();
        let mut engine = Engine::new(paths).unwrap();
        assert!(std::ptr::eq(engine.library_mut(), &engine.library));
    }

    #[test]
    fn reloads_rules() {
        let old_rules = r#"
            rules.add_lint(DENY, "Missing author", "**", |doc| {
                doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
            });
            rules.add_lint(WARN, "Missing author", "**", |doc| {
                doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
            });
        "#;

        let new_rules = r#"
            rules.add_lint(WARN, "Missing author", "**", |doc| {
                doc.meta("author") == "" || type_of(doc.meta("author")) == "()"
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
    fn rebuilds_library() {
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

        let library = engine.library();
        assert_eq!(library.iter().count(), 1);

        std::fs::write(tree.path().join("src/doc2.md"), doc2).expect("failed to write new doc");

        engine
            .rebuild_library()
            .expect("failed to rebuild page store");
        let library = engine.library();
        assert_eq!(library.iter().count(), 2);
    }

    #[test]
    fn re_inits_everything() {
        let (paths, tree) = crate::test::simple_init();
        let mut engine = Engine::new(paths).unwrap();
        assert!(engine.re_init().is_ok());
    }
}

use itertools::Itertools;
use std::{
    ffi::OsStr,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    thread::{self, JoinHandle},
};
use tracing::{instrument, trace};

use crate::{
    core::broker::EngineBroker,
    core::config::EngineConfig,
    core::rules::{RuleProcessor, Rules},
    core::script_engine::ScriptEngine,
    core::{broker::EngineMsg, script_engine::ScriptEngineConfig, LinkedAssets, Page, PageStore},
    devserver::{DevServer, DevServerMsg},
    render::{
        rendered_page::{RenderedPage, RenderedPageCollection},
        Renderers,
    },
    util,
};

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
    ) -> Result<(JoinHandle<Result<(), anyhow::Error>>, EngineBroker), anyhow::Error> {
        let bind = bind.into();

        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_io()
                .enable_time()
                .build()?,
        );

        let broker = EngineBroker::new(rt.clone());
        let broker_clone = broker.clone();

        trace!("spawning engine thread");
        let engine_handle = thread::spawn(move || {
            let mut engine = Self::new(config)?;

            let _devserver =
                engine.start_devserver(bind, debounce_ms, &engine.config, broker.clone())?;

            loop {
                match broker.recv_engine_msg_sync() {
                    Ok(msg) => match msg {
                        EngineMsg::RenderPage(request) => {
                            use crate::render::rendered_page::rewrite_asset_targets;
                            trace!(request = ?request, "receive render page message");
                            dbg!(&engine.page_store);
                            let page: Option<RenderedPage> = {
                                if let Some(page) = engine.page_store.get(request.uri()) {
                                    let mut rendered = engine.render(page)?;
                                    let linked_assets = rewrite_asset_targets(
                                        std::slice::from_mut(&mut rendered),
                                        engine.page_store(),
                                    )?;
                                    engine.run_pipelines(&linked_assets)?;
                                    Some(rendered)
                                } else {
                                    None
                                }
                            };
                            request.send_sync(rt.handle().clone(), page)?
                        }

                        EngineMsg::FilesystemUpdate(events) => {
                            trace!(events = ?events, "receive file system update message");
                            let mut reload_templates = false;
                            let mut reload_rules = false;
                            for changed in events.changed() {
                                // These paths come in as absolute paths. Use strip_prefix
                                // to transform them into relative paths which can then
                                // be used with the engine config.
                                let path = {
                                    let cwd = std::env::current_dir()?;
                                    changed.strip_prefix(cwd)?
                                };
                                if path.starts_with(&engine.config.src_root)
                                    && path.extension().unwrap_or_default().to_string_lossy()
                                        == "md"
                                {
                                    let page = Page::from_file(
                                        &engine.config.src_root.as_path(),
                                        &engine.config.target_root.as_path(),
                                        &path,
                                        engine.renderers(),
                                    )?;
                                    engine.page_store.upsert(page);
                                }

                                if path.starts_with(&engine.config.template_root) {
                                    reload_templates = true;
                                }

                                if path == engine.config.rule_script {
                                    reload_rules = true;
                                }
                            }

                            if reload_templates {
                                engine.reload_template_engines()?;
                            }

                            if reload_rules {
                                engine.reload_rules()?;
                            }
                            // notify websocket server to reload all connected clients
                            broker.send_devserver_msg_sync(DevServerMsg::ReloadPage)?;
                        }
                        EngineMsg::Quit => {
                            break;
                        }
                    },
                    Err(_e) => panic!("problem receiving from engine channel"),
                }
            }
            Ok(())
        });

        Ok((engine_handle, broker_clone))
    }

    #[instrument]
    pub fn new(config: EngineConfig) -> Result<Engine, anyhow::Error> {
        let renderers = Renderers::new(&config.template_root);

        let page_store = do_build_page_store(&config.src_root, &config.target_root, &renderers)?;

        let (script_engine, rule_processor, rules) =
            Self::load_rules(&config.rule_script, &page_store)?;

        Ok(Self {
            config,
            renderers,

            rules,
            rule_processor,
            page_store,
            script_engine,
        })
    }

    #[instrument(ret)]
    pub fn load_rules<P: AsRef<Path> + std::fmt::Debug>(
        rule_script: P,
        page_store: &PageStore,
    ) -> Result<(ScriptEngine, RuleProcessor, Rules), anyhow::Error> {
        let script_engine_config = ScriptEngineConfig::new();
        let script_engine = ScriptEngine::new(&script_engine_config.modules());

        let rule_script = std::fs::read_to_string(&rule_script)?;

        let (rule_processor, rules) = script_engine.build_rules(page_store, rule_script)?;

        Ok((script_engine, rule_processor, rules))
    }

    #[instrument(skip(self), ret)]
    pub fn reload_rules(&mut self) -> Result<(), anyhow::Error> {
        let (script_engine, rule_processor, rules) =
            Self::load_rules(&self.config.rule_script, &self.page_store)?;
        self.script_engine = script_engine;
        self.rule_processor = rule_processor;
        self.rules = rules;
        Ok(())
    }

    #[instrument(skip(self), ret)]
    pub fn reload_template_engines(&mut self) -> Result<(), anyhow::Error> {
        self.renderers.tera.reload()?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn run_pipelines(&self, linked_assets: &LinkedAssets) -> Result<(), anyhow::Error> {
        trace!("running pipelines");

        let engine: &Engine = self;
        for pipeline in engine.rules.pipelines() {
            for asset in linked_assets.iter() {
                if pipeline.is_match(asset.as_str()) {
                    let relative_asset = &asset.as_str()[1..];
                    // Make a new target in order to create directories for the asset.
                    let mut target_dir = PathBuf::from(&engine.config.target_root);
                    target_dir.push(relative_asset);
                    let target_dir = target_dir.parent().expect("should have parent directory");
                    util::make_parent_dirs(target_dir)?;

                    pipeline.run(
                        &engine.config.src_root,
                        &engine.config.target_root,
                        relative_asset,
                    )?;
                }
            }
        }
        Ok(())
    }

    #[instrument(skip(self), fields(page=%page.uri()))]
    pub fn render(&self, page: &Page) -> Result<RenderedPage, anyhow::Error> {
        crate::render::rendered_page::render(self, page)
    }

    #[instrument(skip_all)]
    pub fn render_all(&self) -> Result<RenderedPageCollection, anyhow::Error> {
        trace!("rendering pages");

        let engine: &Engine = self;

        let rendered: Vec<RenderedPage> = engine
            .page_store
            .iter()
            .map(|(_, page)| self.render(page))
            .try_collect()?;

        Ok(RenderedPageCollection::from_vec(rendered))
    }

    #[instrument(skip_all)]
    pub fn rebuild_page_store(&mut self) -> Result<(), anyhow::Error> {
        trace!("rebuilding the page store");
        self.page_store = do_build_page_store(
            &self.config.src_root,
            &self.config.target_root,
            &self.renderers,
        )?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn re_init(&mut self) -> Result<(), anyhow::Error> {
        trace!("rebuilding everything");
        self.reload_template_engines()?;
        self.rebuild_page_store()?;
        self.reload_rules()?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn build_site(&self) -> Result<(), anyhow::Error> {
        use crate::render::rendered_page::rewrite_asset_targets;

        trace!("running build");

        let mut rendered = self.render_all()?;

        trace!("rewriting asset links");
        let assets = rewrite_asset_targets(rendered.as_mut_slice(), self.page_store())?;

        trace!("writing rendered pages to disk");
        rendered.write_to_disk()?;

        self.run_pipelines(&assets)?;
        Ok(())
    }

    #[instrument(skip(self, engine_broker))]
    pub fn start_devserver(
        &self,
        bind: SocketAddr,
        debounce_ms: u64,
        engine_config: &EngineConfig,
        engine_broker: EngineBroker,
    ) -> Result<DevServer, anyhow::Error> {
        trace!("starting devserver");
        use crate::devserver;
        use std::time::Duration;

        // spawn filesystem monitoring thread
        {
            let watch_dirs = vec![
                &engine_config.template_root,
                &engine_config.src_root,
                &engine_config.rule_script,
            ];
            devserver::fswatcher::start_watching(
                &watch_dirs,
                engine_broker.clone(),
                Duration::from_millis(debounce_ms),
            )?;
        }

        let devserver = DevServer::run(engine_broker, &engine_config.target_root, bind);
        Ok(devserver)
    }
}

#[instrument(skip(renderers), ret)]
fn do_build_page_store<P: AsRef<Path> + std::fmt::Debug>(
    src_root: P,
    target_root: P,
    renderers: &Renderers,
) -> Result<PageStore, anyhow::Error> {
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

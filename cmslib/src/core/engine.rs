use anyhow::anyhow;
use itertools::Itertools;
use std::{
    collections::HashSet,
    ffi::OsStr,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    thread::{self, JoinHandle},
};
use tracing::{instrument, trace};

use crate::{
    core::{broker::EngineMsg, rules::script::ScriptEngineConfig},
    devserver::{DevServer, DevServerMsg},
    page::{LinkedAssets, Page},
    pagestore::PageStore,
    render::{
        page::{RenderedPage, RenderedPageCollection},
        Renderers,
    },
    site_context::SiteContext,
    util,
};

use super::{
    broker::EngineBroker,
    config::EngineConfig,
    rules::{
        script::{RuleProcessor, ScriptEngine},
        Rules,
    },
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

            let devserver =
                engine.start_devserver(bind, debounce_ms, &engine.config, broker.clone())?;

            loop {
                match broker.recv_engine_msg_sync() {
                    Ok(msg) => match msg {
                        EngineMsg::RenderPage(request) => {
                            trace!(request = ?request, "receive render page message");
                            dbg!(&engine.page_store);
                            let page: Option<RenderedPage> = if let Some(page) =
                                engine.page_store.get(request.canonical_path.as_str())
                            {
                                let mut rendered = engine.render(page)?;
                                let linked_assets = crate::discover::linked_assets(
                                    std::slice::from_mut(&mut rendered),
                                )?;
                                engine.run_pipelines(&linked_assets)?;
                                Some(rendered)
                            } else {
                                None
                            };
                            request.send_sync(rt.handle().clone(), page)?
                        }

                        EngineMsg::FilesystemUpdate(events) => {
                            trace!(events = ?events, "receive file system update message");
                            let mut reload_templates = false;
                            let mut reload_rules = false;
                            for changed in events.changed() {
                                // These paths come in as absolute paths. We need to convert
                                // them to paths relative to our site content and then into
                                // CanonicalPaths.
                                let path = {
                                    let cwd = std::env::current_dir()?;
                                    changed.strip_prefix(cwd)?
                                };
                                if path.starts_with(&engine.config.src_root) {
                                    if path.extension().unwrap_or_default().to_string_lossy()
                                        == "md"
                                    {
                                        let page = Page::new(
                                            path,
                                            &engine.config.src_root,
                                            &engine.renderers,
                                        )?;
                                        engine.page_store.update(page);
                                    }
                                }

                                if path.starts_with(&engine.config.template_root) {
                                    reload_templates = true;
                                }

                                if path == &engine.config.rule_script {
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
                        EngineMsg::Build => {
                            // engine.build_site()?;
                        }
                        EngineMsg::Rebuild => {
                            // engine.re_init()?;
                        }
                        EngineMsg::ReloadUserConfig => {
                            // engine.reload_rules()?;
                        }
                        EngineMsg::Quit => {
                            break;
                        }
                    },
                    Err(e) => panic!("problem receiving from engine channel"),
                }
            }
            Ok(())
        });

        Ok((engine_handle, broker_clone))
    }

    #[instrument]
    pub fn new(config: EngineConfig) -> Result<Engine, anyhow::Error> {
        let renderers = Renderers::new(&config.template_root);

        let page_store = do_build_page_store(&config.src_root, &renderers)?;

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

    pub fn load_rules<P: AsRef<Path>>(
        rule_script: P,
        page_store: &PageStore,
    ) -> Result<(ScriptEngine, RuleProcessor, Rules), anyhow::Error> {
        let script_engine_config = ScriptEngineConfig::new();
        let script_engine = ScriptEngine::new(&script_engine_config.modules());

        let rule_script = std::fs::read_to_string(&rule_script)?;

        let (rule_processor, rules) = script_engine.build_rules(&page_store, rule_script)?;

        Ok((script_engine, rule_processor, rules))
    }

    pub fn reload_rules(&mut self) -> Result<(), anyhow::Error> {
        let (script_engine, rule_processor, rules) =
            Self::load_rules(&self.config.rule_script, &self.page_store)?;
        self.script_engine = script_engine;
        self.rule_processor = rule_processor;
        self.rules = rules;
        Ok(())
    }

    pub fn reload_template_engines(&mut self) -> Result<(), anyhow::Error> {
        self.renderers.tera.reload()?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn run_pipelines(&self, linked_assets: &LinkedAssets) -> Result<(), anyhow::Error> {
        trace!("running pipelines");

        let engine: &Engine = &self;
        for pipeline in engine.rules.pipelines() {
            for asset in linked_assets.iter() {
                if pipeline.is_match(asset.target().to_string_lossy().to_string()) {
                    // Make a new target in order to create directories for the asset.
                    let mut target_dir = PathBuf::from(&engine.config.output_root);
                    let target_asset = if let Ok(path) = asset.target().strip_prefix("/") {
                        path
                    } else {
                        &asset.target()
                    };
                    target_dir.push(target_asset);
                    let target_dir = target_dir.parent().expect("should have parent directory");
                    util::make_parent_dirs(target_dir)?;

                    pipeline.run(
                        &engine.config.src_root,
                        &engine.config.output_root,
                        target_asset,
                    )?;
                }
            }
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn process_frontmatter_hooks(&self) -> Result<(), anyhow::Error> {
        Ok(())
        // trace!("processing frontmatter hooks");

        // use crate::core::rules::FrontmatterHookResponse;

        // let engine: &Engine = &self;
        // let responses: Vec<(&Page, Vec<FrontmatterHookResponse>)> = engine
        //     .page_store
        //     .iter()
        //     .map(|page| {
        //         (
        //             page,
        //             engine
        //                 .rules
        //                 .frontmatter_hooks()
        //                 .map(|hook_fn| hook_fn(page))
        //                 .filter(|response| match response {
        //                     &FrontmatterHookResponse::Ok => false,
        //                     _ => true,
        //                 })
        //                 .collect(),
        //         )
        //     })
        //     .collect();

        // let mut abort = false;
        // for (page, issues) in responses.iter() {
        //     for issue in issues {
        //         match issue {
        //             FrontmatterHookResponse::Error(msg) => {
        //                 abort = true;
        //             }
        //             FrontmatterHookResponse::Warn(msg) => {}
        //             _ => (),
        //         }
        //     }
        // }

        // if abort {
        //     Err(anyhow!("frontmatter hook errors occurred"))
        // } else {
        //     Ok(())
        // }
    }

    #[instrument(skip(self), fields(page=?page.canonical_path.to_string()))]
    pub fn render(&self, page: &Page) -> Result<RenderedPage, anyhow::Error> {
        crate::render::page::render(&self, page)
    }

    #[instrument(skip_all)]
    pub fn render_all(&self) -> Result<RenderedPageCollection, anyhow::Error> {
        trace!("rendering pages");

        let engine: &Engine = &self;

        let rendered: Vec<RenderedPage> = engine
            .page_store
            .iter()
            .map(|(_, page)| self.render(page))
            .try_collect()?;

        Ok(RenderedPageCollection::from_vec(rendered))
    }

    #[instrument(skip_all)]
    pub fn process_user_script(&mut self) -> Result<(), anyhow::Error> {
        todo!();
        // self.unload_user_config()?;
        // trace!("processing user configuration script");

        // // This will be the configuration supplied by the user scripts.
        // // For now, we are just hard-coding configuration until the scripting
        // // engine is integrated.
        // pub fn add_copy_pipeline(engine: &mut Engine) -> Result<(), anyhow::Error> {
        //     use crate::pipeline::*;
        //     let mut copy_pipeline = Pipeline::new("**/*.png", AutorunTrigger::TargetGlob)?;
        //     copy_pipeline.push_op(Operation::Copy);
        //     engine.rules().add_pipeline(copy_pipeline);
        //     Ok(())
        // }

        // pub fn add_sed_pipeline(engine: &mut Engine) -> Result<(), anyhow::Error> {
        //     use crate::pipeline::*;
        //     let mut sed_pipeline = Pipeline::new("sample.txt", AutorunTrigger::TargetGlob)?;
        //     sed_pipeline.push_op(Operation::Shell(ShellCommand::new(
        //         r"sed 's/hello/goodbye/g' $INPUT > $OUTPUT",
        //     )));
        //     sed_pipeline.push_op(Operation::Shell(ShellCommand::new(
        //         r"sed 's/bye/ day/g' $INPUT > $OUTPUT",
        //     )));
        //     sed_pipeline.push_op(Operation::Copy);

        //     engine.rules().add_pipeline(sed_pipeline);
        //     Ok(())
        // }

        // pub fn add_frontmatter_hook(engine: &mut Engine) {
        //     use crate::core::rules::FrontmatterHookResponse;

        //     let hook = Box::new(|page: &Page| -> FrontmatterHookResponse {
        //         if page.canonical_path.as_str().starts_with("/db") {
        //             if !page.frontmatter.meta.contains_key("section") {
        //                 FrontmatterHookResponse::Error("require 'section' in metadata".to_owned())
        //             } else {
        //                 FrontmatterHookResponse::Ok
        //             }
        //         } else {
        //             FrontmatterHookResponse::Ok
        //         }
        //     });
        //     engine.rules().add_frontmatter_hook(hook);
        // }

        // pub fn add_ctx_generator(engine: &mut Engine) -> Result<(), anyhow::Error> {
        //     use crate::core::rules::gctx::{ContextItem, Matcher};

        //     let matcher = Matcher::Glob(vec!["**/ctxgen/index.md".try_into()?]);
        //     let ctx_fn = GeneratorFunc::new(Box::new(|page_store, page| -> ContextItem {
        //         ContextItem::new("ctxgen_context", "hello!".into())
        //     }));
        //     engine.rules().add_context_generator(matcher, ctx_fn);
        //     Ok(())
        // }

        // self.rules().set_global_context({
        //     let mut map = HashMap::new();
        //     map.insert(
        //         "globular".to_owned(),
        //         "haaaay db sample custom variable!".to_owned(),
        //     );
        //     map
        // })?;

        // add_copy_pipeline(self)?;
        // add_sed_pipeline(self)?;

        // add_frontmatter_hook(self);

        // add_ctx_generator(self)?;

        // Ok(())
    }

    #[instrument(skip_all)]
    pub fn rebuild_page_store(&mut self) -> Result<(), anyhow::Error> {
        trace!("rebuilding the page store");
        self.page_store = do_build_page_store(&self.config.src_root, &self.renderers)?;
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
        trace!("running build");
        self.process_frontmatter_hooks()?;

        let mut rendered = self.render_all()?;
        trace!("writing rendered pages to disk");
        rendered.write_to_disk()?;

        let assets = crate::discover::linked_assets(rendered.as_mut_slice())?;

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

        let devserver = DevServer::run(engine_broker.clone(), &engine_config.output_root, bind);
        Ok(devserver)
    }
}

fn do_build_page_store<P: AsRef<Path>>(
    src_root: P,
    renderers: &Renderers,
) -> Result<PageStore, anyhow::Error> {
    let src_root = src_root.as_ref();
    let mut pages: Vec<_> = crate::discover::get_all_paths(src_root, &|path: &Path| -> bool {
        path.extension() == Some(OsStr::new("md"))
    })?
    .iter()
    .map(|path| Page::new(path.as_path(), &src_root, &renderers))
    .try_collect()?;

    let template_names = renderers.tera.get_template_names().collect::<HashSet<_>>();
    for page in pages.iter_mut() {
        page.set_template(&template_names)?;
    }

    let mut page_store = PageStore::new(&src_root);
    page_store.insert_batch(pages);

    Ok(page_store)
}

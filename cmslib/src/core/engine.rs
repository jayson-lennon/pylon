use anyhow::anyhow;
use itertools::Itertools;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    thread::{self, JoinHandle},
};
use tracing::{instrument, trace, trace_span};

use crate::{
    core::broker::EngineMsg,
    devserver::{DevServer, DevServerMsg},
    page::Page,
    pagestore::PageStore,
    render::Renderers,
    site_context::SiteContext,
    util::RetargetablePathBuf,
};

use super::{broker::EngineBroker, config::EngineConfig, rules::Rules};

#[derive(Clone, Debug, Serialize, Default, Eq, PartialEq, Hash)]
pub struct LinkedAsset {
    target: PathBuf,
}

impl LinkedAsset {
    pub fn new<P: AsRef<Path>>(asset: P) -> Self {
        Self {
            target: asset.as_ref().to_path_buf(),
        }
    }
}

#[derive(Debug)]
pub struct LinkedAssets {
    assets: HashSet<LinkedAsset>,
}

impl LinkedAssets {
    pub fn new(assets: HashSet<LinkedAsset>) -> Self {
        Self { assets }
    }

    pub fn iter(&self) -> impl Iterator<Item = &LinkedAsset> {
        self.assets.iter()
    }
}

#[derive(Debug)]
pub struct RenderedPage {
    pub html: String,
    pub target: RetargetablePathBuf,
}

impl RenderedPage {
    pub fn new<S: Into<String> + std::fmt::Debug>(html: S, target: &RetargetablePathBuf) -> Self {
        Self {
            html: html.into(),
            target: target.clone(),
        }
    }
}

#[derive(Debug)]
pub struct RenderedPageCollection {
    pages: Vec<RenderedPage>,
}

impl RenderedPageCollection {
    pub fn write_to_disk(&self) -> Result<(), std::io::Error> {
        use std::fs;
        for page in self.pages.iter() {
            let target = page.target.to_path_buf();
            make_parent_dirs(target.parent().expect("should have a parent path"))?;
            let _ = fs::write(&target, &page.html)?;
        }

        Ok(())
    }

    #[instrument(skip_all)]
    pub fn find_assets(&self) -> Result<LinkedAssets, anyhow::Error> {
        trace!("searching for linked external assets in rendered pages");
        let mut all_assets = HashSet::new();
        for page in self.pages.iter() {
            let page_assets = crate::discover::find_assets(&page.html)
                .iter()
                .map(|asset| {
                    if asset.starts_with("/") {
                        // absolute path assets don't need any modifications
                        LinkedAsset::new(PathBuf::from(asset))
                    } else {
                        // relative path assets need the parent directory of the page applied
                        let mut target = page
                            .target
                            .as_target()
                            .parent()
                            .expect("should have a parent")
                            .to_path_buf();
                        target.push(asset);
                        LinkedAsset::new(target)
                    }
                })
                .collect::<HashSet<_>>();
            all_assets.extend(page_assets);
        }

        Ok(LinkedAssets::new(all_assets))
    }
}

#[derive(Debug)]
pub struct Engine {
    rt: Arc<tokio::runtime::Runtime>,
    config: EngineConfig,
    rules: Rules,
    page_store: PageStore,
    renderers: Renderers,
    broker: EngineBroker,
    devserver: Option<DevServer>,
}

impl Engine {
    #[instrument]
    pub fn new(
        config: EngineConfig,
    ) -> Result<(JoinHandle<Result<(), anyhow::Error>>, EngineBroker), anyhow::Error> {
        let renderers = Renderers::new(&config.template_root);

        let page_store = do_build_page_store(&config.src_root, &renderers)?;

        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_io()
                .enable_time()
                .build()?,
        );

        let handle = rt.handle().clone();

        let broker = EngineBroker::new(handle);
        let broker_clone = broker.clone();

        trace!("spawning engine thread");
        let engine_handle = thread::spawn(move || {
            let mut engine = Self {
                rt,
                config,
                rules: Rules::new(),
                page_store,
                renderers,
                broker,
                devserver: None,
            };

            engine.process_user_script()?;

            loop {
                match &engine.broker.recv_engine_msg_sync() {
                    Ok(msg) => match msg {
                        EngineMsg::Build => {
                            engine.build()?;
                        }
                        EngineMsg::Rebuild => {
                            engine.rebuild()?;
                        }
                        EngineMsg::ReloadUserConfig => {
                            engine.process_user_script()?;
                        }
                        EngineMsg::StartDevServer(bind, debounce_ms) => {
                            engine.devserver = Some(engine.start_devserver(*bind, *debounce_ms)?);
                        }
                        EngineMsg::Quit => {
                            break;
                        }
                    },
                    Err(e) => panic!("problem receiving from engine channel"),
                }
                if engine.devserver.is_some() {
                    engine
                        .broker
                        .send_devserver_msg_sync(DevServerMsg::ReloadPage)?;
                }
            }
            Ok(())
        });

        Ok((engine_handle, broker_clone))
    }

    pub fn rules(&mut self) -> &mut Rules {
        &mut self.rules
    }

    #[instrument(skip_all)]
    pub fn run_pipelines(&self, linked_assets: &LinkedAssets) -> Result<(), anyhow::Error> {
        trace!("running pipelines");

        let engine: &Engine = &self;
        for pipeline in engine.rules.pipelines() {
            for asset in linked_assets.iter() {
                if pipeline.is_match(asset.target.to_string_lossy().to_string()) {
                    {
                        // Make a new target in order to create directories for the asset.
                        let mut target_dir = PathBuf::from(&engine.config.output_root);
                        target_dir.push(&asset.target);
                        let target_dir = target_dir.parent().expect("should have parent directory");
                        make_parent_dirs(target_dir)?;
                    }
                    pipeline.run(
                        &engine.config.src_root,
                        &engine.config.output_root,
                        &asset.target,
                    )?;
                }
            }
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn process_frontmatter_hooks(&self) -> Result<(), anyhow::Error> {
        trace!("processing frontmatter hooks");

        use crate::core::rules::FrontmatterHookResponse;

        let engine: &Engine = &self;
        let responses: Vec<(&Page, Vec<FrontmatterHookResponse>)> = engine
            .page_store
            .iter()
            .map(|page| {
                (
                    page,
                    engine
                        .rules
                        .frontmatter_hooks()
                        .map(|hook_fn| hook_fn(page))
                        .filter(|response| match response {
                            &FrontmatterHookResponse::Ok => false,
                            _ => true,
                        })
                        .collect(),
                )
            })
            .collect();

        let mut abort = false;
        for (page, issues) in responses.iter() {
            for issue in issues {
                match issue {
                    FrontmatterHookResponse::Error(msg) => {
                        abort = true;
                    }
                    FrontmatterHookResponse::Warn(msg) => {}
                    _ => (),
                }
            }
        }

        if abort {
            Err(anyhow!("frontmatter hook errors occurred"))
        } else {
            Ok(())
        }
    }

    #[instrument(skip_all)]
    pub fn render(&self) -> Result<RenderedPageCollection, anyhow::Error> {
        trace!("rendering pages");

        let engine: &Engine = &self;
        let site_ctx = SiteContext::new("sample");

        Ok(RenderedPageCollection {
            pages: engine
                .page_store
                .iter()
                .map(|page| match page.frontmatter.template_path.as_ref() {
                    Some(template) => {
                        let mut tera_ctx = tera::Context::new();
                        tera_ctx.insert("site", &site_ctx);
                        tera_ctx.insert("content", &page.markdown);
                        tera_ctx.insert("page_store", {
                            &engine.page_store.iter().collect::<Vec<_>>()
                        });
                        if let Some(global) = engine.rules.global_context() {
                            tera_ctx.insert("global", global);
                        }

                        let meta_ctx = tera::Context::from_serialize(&page.frontmatter.meta)
                            .expect("failed converting page metadata into tera context");
                        tera_ctx.extend(meta_ctx);

                        let user_ctx = engine
                            .rules
                            .page_context()
                            .build_context(&self.page_store, page)?;

                        for ctx in user_ctx {
                            let mut user_ctx = tera::Context::new();
                            user_ctx.insert(ctx.identifier, &ctx.data);
                            tera_ctx.extend(user_ctx);
                        }

                        let renderer = &engine.renderers.tera;
                        renderer
                            .render(template, &tera_ctx)
                            .map(|html| {
                                // change file extension to 'html'
                                let target_path = page
                                    .system_path
                                    .with_root::<&Path>(&engine.config.output_root)
                                    .with_extension("html");
                                RenderedPage::new(html, &target_path)
                            })
                            .map_err(|e| anyhow!("{}", e))
                    }
                    None => Err(anyhow!(
                        "no template declared for page '{}'",
                        page.canonical_path.to_string()
                    )),
                })
                .try_collect()?,
        })
    }

    #[instrument(skip_all)]
    pub fn unload_user_config(&mut self) -> Result<(), anyhow::Error> {
        trace!("user configuration unloaded");
        self.rules = Rules::new();
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn process_user_script(&mut self) -> Result<(), anyhow::Error> {
        self.unload_user_config()?;
        trace!("processing user configuration script");

        // This will be the configuration supplied by the user scripts.
        // For now, we are just hard-coding configuration until the scripting
        // engine is integrated.
        pub fn add_copy_pipeline(engine: &mut Engine) -> Result<(), anyhow::Error> {
            use crate::pipeline::*;
            let mut copy_pipeline = Pipeline::new("**/*.png", AutorunTrigger::TargetGlob)?;
            copy_pipeline.push_op(Operation::Copy);
            engine.rules().add_pipeline(copy_pipeline);
            Ok(())
        }

        pub fn add_sed_pipeline(engine: &mut Engine) -> Result<(), anyhow::Error> {
            use crate::pipeline::*;
            let mut sed_pipeline = Pipeline::new("sample.txt", AutorunTrigger::TargetGlob)?;
            sed_pipeline.push_op(Operation::Shell(ShellCommand::new(
                r"sed 's/hello/goodbye/g' $INPUT > $OUTPUT",
            )));
            sed_pipeline.push_op(Operation::Shell(ShellCommand::new(
                r"sed 's/bye/ day/g' $INPUT > $OUTPUT",
            )));
            sed_pipeline.push_op(Operation::Copy);

            engine.rules().add_pipeline(sed_pipeline);
            Ok(())
        }

        pub fn add_frontmatter_hook(engine: &mut Engine) {
            use crate::core::rules::FrontmatterHookResponse;

            let hook = Box::new(|page: &Page| -> FrontmatterHookResponse {
                if page.canonical_path.as_str().starts_with("/db") {
                    if !page.frontmatter.meta.contains_key("section") {
                        FrontmatterHookResponse::Error("require 'section' in metadata".to_owned())
                    } else {
                        FrontmatterHookResponse::Ok
                    }
                } else {
                    FrontmatterHookResponse::Ok
                }
            });
            engine.rules().add_frontmatter_hook(hook);
        }

        pub fn add_ctx_generator(engine: &mut Engine) -> Result<(), anyhow::Error> {
            use crate::gctx::{ContextItem, GeneratorFunc, Matcher};

            let matcher = Matcher::Glob(vec!["**/ctxgen/index.md".try_into()?]);
            let ctx_fn = GeneratorFunc::new(Box::new(|page_store, page| -> ContextItem {
                ContextItem::new("ctxgen_context", "hello!".into())
            }));
            engine.rules().add_context_generator(matcher, ctx_fn);
            Ok(())
        }

        self.rules().set_global_context({
            let mut map = HashMap::new();
            map.insert(
                "globular".to_owned(),
                "haaaay db sample custom variable!".to_owned(),
            );
            map
        })?;

        add_copy_pipeline(self)?;
        add_sed_pipeline(self)?;

        add_frontmatter_hook(self);

        add_ctx_generator(self)?;

        Ok(())
    }

    #[instrument(skip_all)]
    pub fn rebuild_page_store(&mut self) -> Result<(), anyhow::Error> {
        trace!("rebuilding the page store");
        self.page_store = do_build_page_store(&self.config.src_root, &self.renderers)?;
        Ok(())
    }

    #[instrument(skip_all)]
    pub fn rebuild(&mut self) -> Result<(), anyhow::Error> {
        trace!("rebuilding everything");
        self.renderers.tera.reload()?;
        self.rebuild_page_store()?;
        self.build()
    }

    #[instrument(skip_all)]
    pub fn build(&mut self) -> Result<(), anyhow::Error> {
        trace!("running build");
        self.process_frontmatter_hooks()?;

        let rendered = self.render()?;
        trace!("writing rendered pages to disk");
        rendered.write_to_disk()?;

        let assets = rendered.find_assets()?;

        self.run_pipelines(&assets)?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn start_devserver(
        &self,
        bind: SocketAddr,
        debounce_ms: u64,
    ) -> Result<DevServer, anyhow::Error> {
        trace!("starting devserver");
        use crate::devserver;
        use std::time::Duration;

        let engine: &Engine = &self;

        // spawn filesystem monitoring thread
        {
            let watch_dirs = vec![&engine.config.template_root, &engine.config.src_root];
            devserver::fswatcher::start_watching(
                &watch_dirs,
                engine.broker.clone(),
                Duration::from_millis(debounce_ms),
            )?;
        }

        let devserver = DevServer::run(engine.broker.clone(), &engine.config.output_root, bind);
        Ok(devserver)
    }
}

fn make_parent_dirs<P: AsRef<Path>>(dir: P) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dir)
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

    let page_store = page_store;

    Ok(page_store)
}

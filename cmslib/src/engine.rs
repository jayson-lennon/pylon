use anyhow::anyhow;
use itertools::Itertools;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::runtime::{Builder, Handle, Runtime};

use crate::{
    devserver::{DevServerMsg, DevServerReceiver, DevServerSender},
    page::{Page, PageStore},
    pipeline::Pipeline,
    render::Renderers,
    site_context::SiteContext,
    util::RetargetablePathBuf,
};

#[derive(Debug, Clone)]
pub enum EngineMsg {
    TriggerRebuild,
}

type EngineSender = async_channel::Sender<EngineMsg>;
type EngineReceiver = async_channel::Receiver<EngineMsg>;

#[derive(Debug, Clone)]
pub enum FrontmatterHookResponse {
    Ok,
    Warn(String),
    Error(String),
}

type FrontmatterHook = Box<dyn Fn(&Page) -> FrontmatterHookResponse>;

pub struct FrontmatterHooks {
    inner: Vec<Box<dyn Fn(&Page) -> FrontmatterHookResponse>>,
}

impl FrontmatterHooks {
    pub fn new() -> Self {
        Self { inner: vec![] }
    }
    pub fn add(&mut self, hook: FrontmatterHook) {
        self.inner.push(hook);
    }
    pub fn iter(&self) -> impl Iterator<Item = &FrontmatterHook> {
        self.inner.iter()
    }
}

impl std::fmt::Debug for FrontmatterHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("FrontmatterHook count: {}", self.inner.len()))
    }
}

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
    pub fn new<S: Into<String>>(html: S, target: &RetargetablePathBuf) -> Self {
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

    pub fn find_assets(&self) -> Result<LinkedAssets, anyhow::Error> {
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
pub struct EngineConfig {
    pub src_root: PathBuf,
    pub output_root: PathBuf,
    pub template_root: PathBuf,
}

impl EngineConfig {
    pub fn new<P: AsRef<Path>>(src_root: P, target_root: P, template_root: P) -> Self {
        Self {
            src_root: src_root.as_ref().to_path_buf(),
            output_root: target_root.as_ref().to_path_buf(),
            template_root: template_root.as_ref().to_path_buf(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineBroker {
    handle: Handle,
    devserver: (DevServerSender, DevServerReceiver),
    engine: (EngineSender, EngineReceiver),
}

impl EngineBroker {
    fn new(handle: Handle) -> Self {
        Self {
            handle,
            devserver: async_channel::unbounded(),
            engine: async_channel::unbounded(),
        }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub async fn send_devserver_msg(&self, msg: DevServerMsg) -> Result<(), anyhow::Error> {
        Ok(self.devserver.0.send(msg).await?)
    }

    pub async fn send_engine_msg(&self, msg: EngineMsg) -> Result<(), anyhow::Error> {
        Ok(self.engine.0.send(msg).await?)
    }

    pub fn send_engine_msg_sync(&self, msg: EngineMsg) -> Result<(), anyhow::Error> {
        self.handle
            .block_on(async { self.send_engine_msg(msg).await })
    }

    pub fn send_devserver_msg_sync(&self, msg: DevServerMsg) -> Result<(), anyhow::Error> {
        self.handle
            .block_on(async { self.send_devserver_msg(msg).await })
    }

    pub async fn recv_devserver_msg(&self) -> Result<DevServerMsg, anyhow::Error> {
        Ok(self.devserver.1.recv().await?)
    }

    pub fn recv_devserver_msg_sync(&self) -> Result<DevServerMsg, anyhow::Error> {
        self.handle
            .block_on(async { self.recv_devserver_msg().await })
    }
}

#[derive(Debug)]
pub struct Engine {
    rt: Arc<Runtime>,
    config: EngineConfig,
    pipelines: Vec<Pipeline>,
    page_store: PageStore,
    renderers: Renderers,
    frontmatter_hooks: FrontmatterHooks,
    global_ctx: Option<serde_json::Value>,
    broker: EngineBroker,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<(Self, EngineBroker), anyhow::Error> {
        let renderers = Renderers::new(&config.template_root);

        let mut pages: Vec<_> =
            crate::discover::get_all_paths(&config.src_root, &|path: &Path| -> bool {
                path.extension() == Some(OsStr::new("md"))
            })?
            .iter()
            .map(|path| Page::new(path.as_path(), &config.src_root.as_path(), &renderers))
            .try_collect()?;

        let template_names = renderers.tera.get_template_names().collect::<HashSet<_>>();
        for page in pages.iter_mut() {
            page.set_default_template(&template_names)?;
        }

        let mut page_store = PageStore::new(&config.src_root);
        page_store.insert_batch(pages);

        let rt = Arc::new(
            Builder::new_multi_thread()
                .worker_threads(2)
                .enable_io()
                .enable_time()
                .build()?,
        );

        let handle = rt.handle().clone();

        let broker = EngineBroker::new(handle);
        let broker_clone = broker.clone();

        let engine = Self {
            rt,
            config,
            pipelines: vec![],
            page_store,
            renderers,
            frontmatter_hooks: FrontmatterHooks::new(),
            global_ctx: None,
            broker,
        };

        Ok((engine, broker_clone))
    }

    pub fn add_pipeline(&mut self, pipeline: Pipeline) {
        self.pipelines.push(pipeline)
    }

    pub fn run_pipelines(&self, linked_assets: &LinkedAssets) -> Result<(), anyhow::Error> {
        let engine: &Engine = &self;
        for pipeline in &engine.pipelines {
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

    pub fn add_frontmatter_hook(&mut self, hook: FrontmatterHook) {
        self.frontmatter_hooks.add(hook);
    }

    pub fn process_frontmatter_hooks(&self) -> Result<(), anyhow::Error> {
        let engine: &Engine = &self;
        let responses: Vec<(&Page, Vec<FrontmatterHookResponse>)> = engine
            .page_store
            .iter()
            .map(|page| {
                (
                    page,
                    engine
                        .frontmatter_hooks
                        .iter()
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
                        println!("{:?}: {}", page.canonical_path, msg)
                    }
                    FrontmatterHookResponse::Warn(msg) => {
                        println!("{:?}: {}", page.canonical_path, msg)
                    }
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

    pub fn set_global_ctx<S: Serialize>(
        &mut self,
        ctx_fn: &dyn Fn(&PageStore) -> S,
    ) -> Result<(), anyhow::Error> {
        let ctx = ctx_fn(&self.page_store);
        let value = serde_json::to_value(ctx)?;
        self.global_ctx = Some(value);
        Ok(())
    }

    pub fn render<S: Serialize>(
        &self,
        ctx_fn: Box<dyn Fn(&PageStore, &Page) -> S>,
    ) -> Result<RenderedPageCollection, anyhow::Error> {
        let engine: &Engine = &self;
        let site_ctx = SiteContext::new("sample");
        let page_store = engine.page_store.iter().collect::<Vec<_>>();

        Ok(RenderedPageCollection {
            pages: engine
                .page_store
                .iter()
                .map(|page| match page.frontmatter.template_path.as_ref() {
                    Some(template) => {
                        let mut tera_ctx = tera::Context::new();
                        tera_ctx.insert("site", &site_ctx);
                        tera_ctx.insert("content", &page.markdown);
                        tera_ctx.insert("page_store", &page_store);
                        if let Some(global) = engine.global_ctx.as_ref() {
                            tera_ctx.insert("global", global);
                        }

                        let meta_ctx = tera::Context::from_serialize(&page.frontmatter.meta)
                            .expect("failed converting page metadata into tera context");
                        tera_ctx.extend(meta_ctx);

                        let user_ctx = ctx_fn(&engine.page_store, &page);
                        let user_ctx = tera::Context::from_serialize(user_ctx)
                            .expect("failed converting user supplied object into tera context");
                        tera_ctx.extend(user_ctx);

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

    pub fn refresh_clients(&self) -> Result<(), anyhow::Error> {
        self.rt.block_on(async {
            self.broker
                .send_devserver_msg(DevServerMsg::ReloadPage)
                .await
        })
    }

    pub fn process_user_config(&mut self) -> Result<(), anyhow::Error> {
        // This will be the configuration supplied by the user scripts.
        // For now, we are just hard-coding configuration until the scripting
        // engine is integrated.
        pub fn add_copy_pipeline(engine: &mut Engine) -> Result<(), anyhow::Error> {
            use crate::pipeline::*;
            let mut copy_pipeline = Pipeline::new("**/*.png", AutorunTrigger::TargetGlob)?;
            copy_pipeline.push_op(Operation::Copy);
            engine.add_pipeline(copy_pipeline);
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

            engine.add_pipeline(sed_pipeline);
            Ok(())
        }

        pub fn add_frontmatter_hook(engine: &mut Engine) {
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
            engine.add_frontmatter_hook(hook);
        }

        self.set_global_ctx(&|page_store: &PageStore| -> HashMap<String, String> {
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

        self.process_frontmatter_hooks()?;

        let rendered = self.render(Box::new(
            |page_store: &PageStore, page: &Page| -> HashMap<String, String> {
                let mut map = HashMap::new();
                map.insert(
                    "dbsample".to_owned(),
                    "haaaay db sample custom variable!".to_owned(),
                );
                map
            },
        ))?;
        rendered.write_to_disk()?;

        let assets = rendered.find_assets()?;

        self.run_pipelines(&assets)?;
        Ok(())
    }

    pub fn start_devserver(&self, bind: SocketAddr, debounce_ms: u64) -> Result<(), anyhow::Error> {
        use std::time::Duration;

        let engine: &Engine = &self;
        engine.broker.handle().block_on(async move {
            crate::devserver::run(
                engine.broker.clone(),
                &engine.config.output_root,
                bind,
                Duration::from_millis(debounce_ms),
            )
            .await
        })
    }
}

fn make_parent_dirs<P: AsRef<Path>>(dir: P) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dir)
}

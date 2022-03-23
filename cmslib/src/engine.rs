use anyhow::anyhow;
use itertools::Itertools;
use serde::Serialize;
use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use crate::{
    devserver::{DevServerEvent, DevServerReceiver, DevServerSender},
    page::{Page, PageStore},
    pipeline::Pipeline,
    render::Renderers,
    site_context::SiteContext,
    util::RetargetablePathBuf,
};

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

#[derive(Debug)]
pub struct EngineBroker {
    pub devserver_channel: (DevServerSender, DevServerReceiver),
}

impl EngineBroker {
    fn new() -> Self {
        Self {
            devserver_channel: async_channel::unbounded(),
        }
    }

    async fn send_devserver_event(&self, event: DevServerEvent) -> Result<(), anyhow::Error> {
        Ok(self.devserver_channel.0.send(event).await?)
    }
}

#[derive(Debug)]
pub struct Engine {
    config: EngineConfig,
    pipelines: Vec<Pipeline>,
    page_store: PageStore,
    renderers: Renderers,
    frontmatter_hooks: FrontmatterHooks,
    global_ctx: Option<serde_json::Value>,
    pub broker: EngineBroker,
}

impl Engine {
    pub fn new(config: EngineConfig, renderers: Renderers) -> Result<Self, anyhow::Error> {
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

        Ok(Self {
            config,
            pipelines: vec![],
            page_store,
            renderers,
            frontmatter_hooks: FrontmatterHooks::new(),
            global_ctx: None,
            broker: EngineBroker::new(),
        })
    }

    pub fn add_pipeline(&mut self, pipeline: Pipeline) {
        self.pipelines.push(pipeline)
    }

    pub fn run_pipelines(&self, linked_assets: &LinkedAssets) -> Result<(), anyhow::Error> {
        for pipeline in &self.pipelines {
            for asset in linked_assets.iter() {
                if pipeline.is_match(asset.target.to_string_lossy().to_string()) {
                    {
                        // Make a new target in order to create directories for the asset.
                        let mut target_dir = PathBuf::from(&self.config.output_root);
                        target_dir.push(&asset.target);
                        let target_dir = target_dir.parent().expect("should have parent directory");
                        make_parent_dirs(target_dir)?;
                    }
                    pipeline.run(
                        &self.config.src_root,
                        &self.config.output_root,
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
        let responses: Vec<(&Page, Vec<FrontmatterHookResponse>)> = self
            .page_store
            .iter()
            .map(|page| {
                (
                    page,
                    self.frontmatter_hooks
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
        let site_ctx = SiteContext::new("sample");
        let page_store = &self.page_store.iter().collect::<Vec<_>>();

        Ok(RenderedPageCollection {
            pages: self
                .page_store
                .iter()
                .map(|page| match page.frontmatter.template_path.as_ref() {
                    Some(template) => {
                        let mut tera_ctx = tera::Context::new();
                        tera_ctx.insert("site", &site_ctx);
                        tera_ctx.insert("content", &page.markdown);
                        tera_ctx.insert("page_store", &page_store);
                        if let Some(global) = self.global_ctx.as_ref() {
                            tera_ctx.insert("global", global);
                        }

                        let meta_ctx = tera::Context::from_serialize(&page.frontmatter.meta)
                            .expect("failed converting page metadata into tera context");
                        tera_ctx.extend(meta_ctx);

                        let user_ctx = ctx_fn(&self.page_store, &page);
                        let user_ctx = tera::Context::from_serialize(user_ctx)
                            .expect("failed converting user supplied object into tera context");
                        tera_ctx.extend(user_ctx);

                        let renderer = &self.renderers.tera;
                        renderer
                            .render(template, &tera_ctx)
                            .map(|html| {
                                // change file extension to 'html'
                                let target_path = page
                                    .system_path
                                    .with_root::<&Path>(&self.config.output_root)
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
}

fn make_parent_dirs<P: AsRef<Path>>(dir: P) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dir)
}
